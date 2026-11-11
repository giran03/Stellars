//! Keeps the user's own speakers and microphone as the Windows defaults when a newly installed
//! virtual cable takes over. Windows often makes `CABLE Input` the default playback device (and
//! `CABLE Output` the default recording device) right after VB-CABLE installs, which silences
//! every app that is not routed and can feed system sound straight into the virtual mic.

#![allow(non_snake_case)]

use crate::interop::cables::is_virtual_audio_endpoint_name;
use crate::interop::sessions::{enumerate_audio_endpoints, AudioEndpointInfo};
use std::ffi::c_void;
use std::thread;
use std::time::Duration;
use windows::core::{interface, IUnknown, IUnknown_Vtbl, GUID, HRESULT, HSTRING, PCWSTR};
use windows::Win32::Media::Audio::{eCommunications, eConsole, eMultimedia, ERole};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
use windows::core::PROPVARIANT;

/// `CPolicyConfigClient`, the COM class Windows' own Sound settings use to change defaults.
const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// How many times, and how far apart, to check for a takeover after the drivers install.
/// Windows can switch the default a moment after the new endpoint appears.
const HIJACK_CHECKS: u32 = 6;
const HIJACK_CHECK_INTERVAL: Duration = Duration::from_millis(500);

// Undocumented but stable since Windows 7 (used by Sound settings, EarTrumpet, SoundSwitch).
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    fn GetMixFormat(&self, device: PCWSTR, format: *mut *mut c_void) -> HRESULT;
    fn GetDeviceFormat(&self, device: PCWSTR, default: i32, format: *mut *mut c_void) -> HRESULT;
    fn ResetDeviceFormat(&self, device: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(&self, device: PCWSTR, endpoint: *mut c_void, mix: *mut c_void) -> HRESULT;
    fn GetProcessingPeriod(&self, device: PCWSTR, default: i32, def: *mut i64, min: *mut i64) -> HRESULT;
    fn SetProcessingPeriod(&self, device: PCWSTR, period: *mut i64) -> HRESULT;
    fn GetShareMode(&self, device: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn SetShareMode(&self, device: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn GetPropertyValue(&self, device: PCWSTR, fx_store: i32, key: *const PROPERTYKEY, value: *mut PROPVARIANT) -> HRESULT;
    fn SetPropertyValue(&self, device: PCWSTR, fx_store: i32, key: *const PROPERTYKEY, value: *mut PROPVARIANT) -> HRESULT;
    fn SetDefaultEndpoint(&self, device: PCWSTR, role: ERole) -> HRESULT;
    fn SetEndpointVisibility(&self, device: PCWSTR, visible: i32) -> HRESULT;
}

/// The Windows default playback and recording devices at one moment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DefaultDevices {
    pub render: Option<String>,
    pub capture: Option<String>,
}

impl DefaultDevices {
    pub fn from_endpoints(endpoints: &[AudioEndpointInfo]) -> Self {
        let default_for = |flow: &str| {
            endpoints
                .iter()
                .find(|e| e.flow == flow && e.is_default)
                .map(|e| e.id.clone())
        };
        Self {
            render: default_for("Render"),
            capture: default_for("Capture"),
        }
    }

    pub fn capture_now() -> Self {
        enumerate_audio_endpoints()
            .map(|eps| Self::from_endpoints(&eps))
            .unwrap_or_default()
    }
}

/// Devices to put back as the default: one per flow where a virtual cable replaced a physical
/// device that is still connected. A virtual default the user chose before install is left alone.
pub fn devices_to_restore<'a>(
    before: &DefaultDevices,
    endpoints: &'a [AudioEndpointInfo],
) -> Vec<&'a AudioEndpointInfo> {
    [("Render", before.render.as_deref()), ("Capture", before.capture.as_deref())]
        .into_iter()
        .filter_map(|(flow, before_id)| {
            let before_id = before_id?;
            let current = endpoints.iter().find(|e| e.flow == flow && e.is_default)?;
            if current.id == before_id || !is_virtual_audio_endpoint_name(&current.name) {
                return None;
            }
            endpoints
                .iter()
                .find(|e| e.flow == flow && e.id == before_id && !is_virtual_audio_endpoint_name(&e.name))
        })
        .collect()
}

fn set_default_endpoint(device_id: &str) -> Result<(), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let policy: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
            .map_err(|e| format!("Could not open the Windows sound policy: {e}"))?;
        let id = HSTRING::from(device_id);
        for role in [eConsole, eMultimedia, eCommunications] {
            policy
                .SetDefaultEndpoint(PCWSTR(id.as_ptr()), role)
                .ok()
                .map_err(|e| format!("Could not set the default device: {e}"))?;
        }
    }
    Ok(())
}

/// Puts the user's own devices back as the Windows defaults if the cable install took them over.
/// Returns a log line for each device restored.
pub fn restore_if_taken_over(before: &DefaultDevices) -> Vec<String> {
    let mut actions = Vec::new();
    for _ in 0..HIJACK_CHECKS {
        if let Ok(endpoints) = enumerate_audio_endpoints() {
            for ep in devices_to_restore(before, &endpoints) {
                let kind = if ep.flow == "Render" { "playback" } else { "recording" };
                match set_default_endpoint(&ep.id) {
                    Ok(()) => actions.push(format!("Kept '{}' as the Windows default {kind} device", ep.name)),
                    Err(err) => actions.push(format!("Could not restore '{}' as the default {kind} device: {err}", ep.name)),
                }
            }
        }
        thread::sleep(HIJACK_CHECK_INTERVAL);
    }
    actions.dedup();
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn before() -> DefaultDevices {
        DefaultDevices { render: Some("spk".into()), capture: Some("mic".into()) }
    }

    #[test]
    fn restores_physical_devices_replaced_by_a_cable() {
        let eps = vec![
            ep("spk", "Speakers (Realtek Audio)", "Render", false),
            ep("cab-in", "CABLE Input (VB-Audio Virtual Cable)", "Render", true),
            ep("mic", "Microphone (USB Audio)", "Capture", false),
            ep("cab-out", "CABLE Output (VB-Audio Virtual Cable)", "Capture", true),
        ];
        let ids: Vec<_> = devices_to_restore(&before(), &eps).iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["spk", "mic"]);
    }

    #[test]
    fn leaves_defaults_alone_when_nothing_changed_or_user_picked_another_physical_device() {
        let eps = vec![
            ep("spk", "Speakers (Realtek Audio)", "Render", false),
            ep("hdmi", "LG Monitor (NVIDIA High Definition Audio)", "Render", true),
            ep("mic", "Microphone (USB Audio)", "Capture", true),
        ];
        assert!(devices_to_restore(&before(), &eps).is_empty());
    }

    #[test]
    fn leaves_a_virtual_default_the_user_already_had() {
        let eps = vec![
            ep("cab-in", "CABLE Input (VB-Audio Virtual Cable)", "Render", true),
            ep("hifi", "Hi-Fi Cable Input (VB-Audio Hi-Fi Cable)", "Render", false),
        ];
        let had_cable = DefaultDevices { render: Some("hifi".into()), capture: None };
        assert!(devices_to_restore(&had_cable, &eps).is_empty());
    }

    #[test]
    fn skips_a_previous_default_that_is_no_longer_connected() {
        let eps = vec![ep("cab-in", "CABLE Input (VB-Audio Virtual Cable)", "Render", true)];
        assert!(devices_to_restore(&before(), &eps).is_empty());
    }

    /// Checks the COM class and vtable layout on this PC with read-only calls.
    #[test]
    fn policy_config_interface_answers_read_only_calls() {
        let Some(render) = DefaultDevices::capture_now().render else {
            return;
        };
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let policy: IPolicyConfig =
                CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL).expect("CPolicyConfigClient");
            let id = HSTRING::from(render.as_str());
            let mut format: *mut c_void = std::ptr::null_mut();
            assert!(policy.GetMixFormat(PCWSTR(id.as_ptr()), &mut format).is_ok());
            assert!(!format.is_null());
            windows::Win32::System::Com::CoTaskMemFree(Some(format));

            // Slot 8 (two before SetDefaultEndpoint): the device description must match Windows'.
            let key = PROPERTYKEY {
                fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
                pid: 2,
            };
            let mut value = PROPVARIANT::default();
            assert!(policy.GetPropertyValue(PCWSTR(id.as_ptr()), 0, &key, &mut value).is_ok());
            println!("[POLICY] default render description: {}", value.to_string());
            assert!(!value.to_string().is_empty());
        }
    }
}
