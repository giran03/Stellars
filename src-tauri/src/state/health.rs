//! Setup checks shown on the diagnostics page and in the first-run wizard, the latency estimate
//! and the redaction used for the support file. Pure functions, so they are easy to test.

use crate::interop::cables::{is_virtual_audio_endpoint_name, RouteSlotStatus, VirtualCableStatusPayload};
use crate::interop::sessions::AudioEndpointInfo;
use serde::Serialize;

/// How far the resampler's ring buffer is filled on purpose (see `audio/resampler.rs`).
const RING_TARGET_MS: f32 = 20.0;
/// Extra delay of the high-quality noise suppression model (1440 frames at 48 kHz).
const HIGH_QUALITY_NOISE_MS: f32 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthLevel {
    Ok,
    Info,
    Warn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub id: String,
    pub level: HealthLevel,
    pub title: String,
    pub detail: String,
}

fn check(id: &str, level: HealthLevel, title: &str, detail: impl Into<String>) -> HealthCheck {
    HealthCheck { id: id.into(), level, title: title.into(), detail: detail.into() }
}

fn route_check(id: &str, title: &str, slot: &RouteSlotStatus, optional: bool) -> HealthCheck {
    match slot.status.as_str() {
        "ready" => check(id, HealthLevel::Ok, title, slot.device_name.clone().unwrap_or_default()),
        "unassigned" if optional => check(id, HealthLevel::Info, title, slot.hint.clone()),
        _ => check(id, HealthLevel::Warn, title, slot.hint.clone()),
    }
}

/// Checks that the virtual cables, routes and Windows defaults are in a usable state.
pub fn setup_checks(
    engine_running: bool,
    cables: &VirtualCableStatusPayload,
    endpoints: &[AudioEndpointInfo],
) -> Vec<HealthCheck> {
    let mut out = Vec::new();
    out.push(if engine_running {
        check("engine", HealthLevel::Ok, "Audio engine", "Running")
    } else {
        check("engine", HealthLevel::Warn, "Audio engine", "Not running. Pick a monitor output in Settings → Audio.")
    });
    out.push(if cables.vb_cable_installed {
        check("vbCable", HealthLevel::Ok, "VB-CABLE", "Installed")
    } else {
        check("vbCable", HealthLevel::Warn, "VB-CABLE", "Not found. Use One-click Install in Settings → Audio.")
    });
    out.push(if cables.hifi_cable_installed {
        check("hifiCable", HealthLevel::Ok, "Hi-Fi Cable", "Installed")
    } else {
        check("hifiCable", HealthLevel::Info, "Hi-Fi Cable", "Not found. Without it the Stream Mix has no cable of its own.")
    });
    out.push(route_check("silentSink", "Silent routing sink", &cables.silent_sink, false));
    out.push(route_check("virtualMic", "Virtual microphone", &cables.virtual_mic, false));
    out.push(route_check("streamMix", "Stream mix output", &cables.stream_mix, true));

    for (flow, id, label) in [
        ("Render", "defaultSpeakers", "Windows default speakers"),
        ("Capture", "defaultMic", "Windows default microphone"),
    ] {
        match endpoints.iter().find(|e| e.flow == flow && e.is_default) {
            Some(ep) if is_virtual_audio_endpoint_name(&ep.name) => out.push(check(
                id,
                HealthLevel::Warn,
                label,
                format!("{} is a virtual cable. Apps you don't route would be silent or feed the cable. Use Restore Windows audio to put your own device back.", ep.name),
            )),
            Some(ep) => out.push(check(id, HealthLevel::Ok, label, ep.name.clone())),
            None => out.push(check(id, HealthLevel::Warn, label, "No default device found")),
        }
    }
    out
}

/// Estimated delay of the monitor path in milliseconds: the Windows render buffer, the
/// resampler's target fill and the high-quality noise model when it is running.
pub fn estimate_latency_ms(render_buffer_frames: u32, sample_rate_hz: u32, high_quality_noise: bool) -> f32 {
    let rate = sample_rate_hz.max(1) as f32;
    let buffer_ms = render_buffer_frames as f32 * 1000.0 / rate;
    buffer_ms + RING_TARGET_MS + if high_quality_noise { HIGH_QUALITY_NOISE_MS } else { 0.0 }
}

/// Replaces the Windows user name in `C:\Users\<name>\…` paths so a support file can be shared.
pub fn redact_user_names(text: &str) -> String {
    const NEEDLE: &str = "\\users\\";
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(NEEDLE) {
        let name_start = at + found + NEEDLE.len();
        out.push_str(&text[at..name_start]);
        let name_len = text[name_start..]
            .find(|c: char| matches!(c, '\\' | '/' | '"' | '\'' | '\n' | '\r'))
            .unwrap_or(text.len() - name_start);
        out.push_str("<user>");
        at = name_start + name_len;
    }
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::config::RouterConfig;

    fn ep(id: &str, name: &str, flow: &str, is_default: bool) -> AudioEndpointInfo {
        AudioEndpointInfo {
            id: id.into(),
            name: name.into(),
            flow: flow.into(),
            is_default,
            is_dummy_sink_candidate: false,
            is_virtual_mic_candidate: false,
        }
    }

    fn level_of(checks: &[HealthCheck], id: &str) -> HealthLevel {
        checks.iter().find(|c| c.id == id).map(|c| c.level).expect("check present")
    }

    #[test]
    fn healthy_setup_has_no_warnings() {
        let eps = vec![
            ep("sink-in", "Stellars - Silent Sink (Stellars Hi-Fi Virtual Cable)", "Render", false),
            ep("mic-in", "Stellars - Virtual Mic In (Stellars Virtual Audio Cable)", "Render", false),
            ep("mic-out", "Stellars - Microphone (Stellars Virtual Audio Cable)", "Capture", false),
            ep("spk", "Speakers (Realtek Audio)", "Render", true),
            ep("mic", "Microphone (USB Audio)", "Capture", true),
        ];
        let mut cfg = RouterConfig::default();
        let _ = crate::interop::cables::auto_assign_virtual_routes(&mut cfg, &eps, false);
        let cables = crate::interop::cables::inspect_virtual_cables(&cfg, &eps);
        let checks = setup_checks(true, &cables, &eps);
        assert_eq!(level_of(&checks, "engine"), HealthLevel::Ok);
        assert_eq!(level_of(&checks, "defaultSpeakers"), HealthLevel::Ok);
        assert_eq!(level_of(&checks, "defaultMic"), HealthLevel::Ok);
    }

    #[test]
    fn warns_when_a_cable_is_the_windows_default_or_the_engine_is_down() {
        let eps = vec![
            ep("cab-in", "CABLE Input (VB-Audio Virtual Cable)", "Render", true),
            ep("cab-out", "CABLE Output (VB-Audio Virtual Cable)", "Capture", true),
        ];
        let cables = crate::interop::cables::inspect_virtual_cables(&RouterConfig::default(), &eps);
        let checks = setup_checks(false, &cables, &eps);
        assert_eq!(level_of(&checks, "engine"), HealthLevel::Warn);
        assert_eq!(level_of(&checks, "defaultSpeakers"), HealthLevel::Warn);
        assert_eq!(level_of(&checks, "defaultMic"), HealthLevel::Warn);
        assert_eq!(level_of(&checks, "hifiCable"), HealthLevel::Info);
        assert_eq!(level_of(&checks, "vbCable"), HealthLevel::Ok);
    }

    #[test]
    fn latency_adds_the_buffer_the_ring_and_the_noise_model() {
        assert_eq!(estimate_latency_ms(480, 48_000, false), 30.0);
        assert_eq!(estimate_latency_ms(480, 48_000, true), 60.0);
        assert!(estimate_latency_ms(0, 0, false).is_finite());
    }

    #[test]
    fn user_names_are_redacted_in_paths() {
        let text = "log at C:\\Users\\Jane Doe\\AppData\\Roaming\\x and \"c:\\USERS\\bob\" ok";
        let out = redact_user_names(text);
        assert!(!out.contains("Jane"));
        assert!(!out.contains("bob"));
        assert!(out.contains("C:\\Users\\<user>\\AppData\\Roaming\\x"));
        assert_eq!(redact_user_names("nothing here"), "nothing here");
    }
}
