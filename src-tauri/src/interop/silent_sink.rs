//! Lets a spare output (for example a monitor's HDMI audio) be the silent sink, so both virtual
//! cables stay free for the Virtual Mic and the Stream Mix.
//!
//! Stellars keeps such an output muted in Windows. Muting the device does not affect Stellars'
//! per-app capture (verified: process loopback still receives full-level audio from apps playing
//! to a muted or zero-volume device), so routed apps are processed normally and never heard raw.

use crate::interop::cables::is_virtual_audio_endpoint_name;
use crate::interop::sessions::AudioEndpointInfo;
use crate::state::config::RouterConfig;
use serde::{Deserialize, Serialize};
use windows::core::HSTRING;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpareOutput {
    pub id: String,
    pub name: String,
}

/// Processing devices that forward to real speakers; muting them would cut that audio too.
fn is_forwarding_device(name: &str) -> bool {
    name.to_ascii_lowercase().contains("broadcast")
}

/// True for a physical (non-cable) playback device that is currently connected.
fn is_physical_output(id: &str, endpoints: &[AudioEndpointInfo]) -> bool {
    endpoints
        .iter()
        .any(|e| e.flow == "Render" && e.id == id && !is_virtual_audio_endpoint_name(&e.name))
}

/// Physical outputs that could become the silent sink: not the monitor output, not the Windows
/// default (apps that are not routed still play there) and not already the silent sink.
pub fn spare_outputs(cfg: &RouterConfig, endpoints: &[AudioEndpointInfo]) -> Vec<SpareOutput> {
    endpoints
        .iter()
        .filter(|e| e.flow == "Render" && !e.is_default)
        .filter(|e| !is_virtual_audio_endpoint_name(&e.name) && !is_forwarding_device(&e.name))
        .filter(|e| cfg.master_output_device_id.as_deref() != Some(e.id.as_str()))
        .filter(|e| cfg.dummy_sink_device_id.as_deref() != Some(e.id.as_str()))
        .map(|e| SpareOutput { id: e.id.clone(), name: e.name.clone() })
        .collect()
}

/// True when the silent sink is a physical output that Stellars keeps muted.
pub fn sink_is_muted_output(cfg: &RouterConfig, endpoints: &[AudioEndpointInfo]) -> bool {
    cfg.dummy_sink_device_id
        .as_deref()
        .is_some_and(|id| is_physical_output(id, endpoints))
}

/// What to change after the silent sink moved: `(device, mute)` pairs. A physical silent sink is
/// muted; a physical output that stopped being the silent sink gets its sound back.
pub fn mute_changes(
    previous_sink: Option<&str>,
    cfg: &RouterConfig,
    endpoints: &[AudioEndpointInfo],
) -> Vec<(String, bool)> {
    let current = cfg.dummy_sink_device_id.as_deref();
    let mut changes = Vec::new();
    if let Some(id) = current.filter(|id| is_physical_output(id, endpoints)) {
        changes.push((id.to_string(), true));
    }
    if let Some(prev) = previous_sink.filter(|prev| Some(*prev) != current) {
        if is_physical_output(prev, endpoints) {
            changes.push((prev.to_string(), false));
        }
    }
    changes
}

fn set_output_muted(device_id: &str, muted: bool) -> Result<(), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .map_err(|e| format!("Could not open the audio devices: {e}"))?;
        let volume: IAudioEndpointVolume = enumerator
            .GetDevice(&HSTRING::from(device_id))
            .and_then(|d| d.Activate(CLSCTX_ALL, None))
            .map_err(|e| format!("Could not open the device volume: {e}"))?;
        volume
            .SetMute(muted, std::ptr::null())
            .map_err(|e| format!("Could not change the device mute: {e}"))
    }
}

/// Mutes a physical silent sink and unmutes a physical output that is no longer one.
/// Returns a log line per change.
pub fn sync_mute(
    previous_sink: Option<&str>,
    cfg: &RouterConfig,
    endpoints: &[AudioEndpointInfo],
) -> Vec<String> {
    mute_changes(previous_sink, cfg, endpoints)
        .into_iter()
        .map(|(id, muted)| {
            let name = endpoints.iter().find(|e| e.id == id).map(|e| e.name.as_str()).unwrap_or("output");
            match set_output_muted(&id, muted) {
                Ok(()) if muted => format!("Muted '{name}' (silent sink)"),
                Ok(()) => format!("Unmuted '{name}' (no longer the silent sink)"),
                Err(err) => format!("Could not update '{name}': {err}"),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(id: &str, name: &str, is_default: bool) -> AudioEndpointInfo {
        AudioEndpointInfo {
            id: id.into(),
            name: name.into(),
            flow: "Render".into(),
            is_default,
            is_dummy_sink_candidate: false,
            is_virtual_mic_candidate: false,
        }
    }

    fn endpoints() -> Vec<AudioEndpointInfo> {
        vec![
            ep("phones", "Headphones (Realtek USB2.0 Audio)", true),
            ep("monitor", "MAG 245F X24 (NVIDIA High Definition Audio)", false),
            ep("tv", "LG ULTRAWIDE (NVIDIA High Definition Audio)", false),
            ep("bcast", "Speakers (NVIDIA Broadcast)", false),
            ep("hifi", "Hi-Fi Cable Input (VB-Audio Hi-Fi Cable)", false),
            ep("vb", "CABLE Input (VB-Audio Virtual Cable)", false),
        ]
    }

    fn cfg(master: Option<&str>, sink: Option<&str>) -> RouterConfig {
        let mut c = RouterConfig::default();
        c.master_output_device_id = master.map(Into::into);
        c.dummy_sink_device_id = sink.map(Into::into);
        c
    }

    fn spare_ids(c: &RouterConfig) -> Vec<String> {
        spare_outputs(c, &endpoints()).into_iter().map(|s| s.id).collect()
    }

    #[test]
    fn spare_outputs_are_unused_physical_outputs_only() {
        assert_eq!(spare_ids(&cfg(Some("phones"), Some("hifi"))), vec!["monitor", "tv"]);
    }

    #[test]
    fn spare_outputs_skip_the_windows_default_and_the_current_sink() {
        assert_eq!(spare_ids(&cfg(None, Some("monitor"))), vec!["tv"]);
    }

    #[test]
    fn moving_the_sink_to_a_monitor_mutes_it() {
        let changes = mute_changes(Some("hifi"), &cfg(Some("phones"), Some("monitor")), &endpoints());
        assert_eq!(changes, vec![("monitor".to_string(), true)]);
    }

    #[test]
    fn moving_the_sink_back_to_a_cable_unmutes_the_monitor() {
        let changes = mute_changes(Some("monitor"), &cfg(Some("phones"), Some("hifi")), &endpoints());
        assert_eq!(changes, vec![("monitor".to_string(), false)]);
    }

    #[test]
    fn an_old_sink_that_becomes_the_monitor_output_gets_its_sound_back() {
        let changes = mute_changes(Some("monitor"), &cfg(Some("monitor"), Some("hifi")), &endpoints());
        assert_eq!(changes, vec![("monitor".to_string(), false)]);
    }

    #[test]
    fn a_cable_sink_is_never_muted() {
        assert!(mute_changes(None, &cfg(Some("phones"), Some("hifi")), &endpoints()).is_empty());
        assert!(!sink_is_muted_output(&cfg(Some("phones"), Some("hifi")), &endpoints()));
        assert!(sink_is_muted_output(&cfg(Some("phones"), Some("tv")), &endpoints()));
    }
}
