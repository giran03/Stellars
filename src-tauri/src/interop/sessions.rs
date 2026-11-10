use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use windows::core::{Interface, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, EDataFlow, IAudioClient, IAudioSessionControl2, IAudioSessionManager2,
    IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    STGM_READ,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioEndpointInfo {
    pub id: String,
    pub name: String,
    pub flow: String, // "Render" | "Capture"
    #[serde(rename = "isDefault")]
    pub is_default: bool,
    #[serde(rename = "isDummySinkCandidate")]
    pub is_dummy_sink_candidate: bool,
    #[serde(rename = "isVirtualMicCandidate")]
    pub is_virtual_mic_candidate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveAudioProcess {
    pub pid: u32,
    #[serde(rename = "exeName")]
    pub exe_name: String,
    #[serde(rename = "exePath")]
    pub exe_path: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "assignedBus")]
    pub assigned_bus: Option<String>,
    #[serde(rename = "isRouted")]
    pub is_routed: bool,
}

/// Checks if a playback device name matches known WHQL-signed virtual audio cables / dummy sinks.
pub fn is_known_dummy_sink_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("vb-audio")
        || lower.contains("stellars virtual")
        || lower.contains("stellars hi-fi")
        || lower.contains("stellars -")
        || lower.contains("cable input")
        || lower.contains("virtual cable")
        || lower.contains("virtual audio cable")
        || lower.contains("steam streaming speakers")
        || lower.contains("hi-fi cable")
        || lower.contains("oculus virtual audio device")
        || lower.contains("vaio")
        || lower.contains("sonar")
}

/// Checks if a render endpoint can serve as the input side of a virtual microphone cable.
pub fn is_known_virtual_mic_render_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("vb-audio")
        || lower.contains("stellars virtual")
        || lower.contains("stellars hi-fi")
        || lower.contains("stellars -")
        || lower.contains("cable input")
        || lower.contains("hi-fi cable")
        || lower.contains("steam streaming microphone")
        || lower.contains("virtual mic")
        || lower.contains("voice")
}

unsafe fn get_device_id(device: &IMMDevice) -> Option<String> {
    let raw_id: PWSTR = device.GetId().ok()?;
    let id_str = raw_id.to_string().ok();
    CoTaskMemFree(Some(raw_id.0 as *const _));
    id_str
}

unsafe fn get_device_friendly_name(device: &IMMDevice) -> String {
    if let Ok(store) = device.OpenPropertyStore(STGM_READ) {
        if let Ok(prop) = store.GetValue(&PKEY_Device_FriendlyName) {
            let s = prop.to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    "Unknown Audio Endpoint".to_string()
}

/// Enumerates all active Render and Capture endpoints on the system via `IMMDeviceEnumerator`.
pub fn enumerate_audio_endpoints() -> Result<Vec<AudioEndpointInfo>, String> {
    unsafe {
        // Use APARTMENTTHREADED (STA) so calling on the main thread is compatible with Tauri/tao OleInitialize;
        // if called on an MTA worker thread, RPC_E_CHANGED_MODE is ignored because COM is already active.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("CoCreateInstance(MMDeviceEnumerator) failed: {e}"))?;

        let mut results = Vec::new();

        for (flow, flow_label) in [(eRender, "Render"), (eCapture, "Capture")] {
            let default_id = enumerator
                .GetDefaultAudioEndpoint(flow, eConsole)
                .ok()
                .and_then(|d| get_device_id(&d));

            let collection = enumerator
                .EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)
                .map_err(|e| format!("EnumAudioEndpoints({flow_label}) failed: {e}"))?;

            let count = collection.GetCount().unwrap_or(0);
            for i in 0..count {
                if let Ok(device) = collection.Item(i) {
                    if let Some(id) = get_device_id(&device) {
                        let name = get_device_friendly_name(&device);
                        let is_default = default_id.as_deref() == Some(id.as_str());
                        let is_dummy_sink_candidate =
                            flow == eRender && is_known_dummy_sink_name(&name);
                        let is_virtual_mic_candidate =
                            flow == eRender && is_known_virtual_mic_render_name(&name);

                        results.push(AudioEndpointInfo {
                            id,
                            name,
                            flow: flow_label.to_string(),
                            is_default,
                            is_dummy_sink_candidate,
                            is_virtual_mic_candidate,
                        });
                    }
                }
            }
        }

        Ok(results)
    }
}

/// Shared-mode mix sample rate (Hz) of an endpoint, or `None` if it is missing or unreadable.
/// Used by diagnostics to warn when a device does not run at the engine's 48 kHz.
pub fn device_mix_rate(device_id: &str) -> Option<u32> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let wide: Vec<u16> = device_id.trim().encode_utf16().chain(std::iter::once(0)).collect();
        let device = enumerator.GetDevice(PWSTR(wide.as_ptr() as *mut u16)).ok()?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None).ok()?;
        let format = client.GetMixFormat().ok()?;
        if format.is_null() {
            return None;
        }
        let rate = (*format).nSamplesPerSec;
        CoTaskMemFree(Some(format as *const _));
        Some(rate)
    }
}

/// Locates an IMMDevice by its endpoint ID string (`{0.0.0.00000000}.{guid}`), or falls back to
/// the system default endpoint for `flow`.
pub fn resolve_mmdevice(device_id: Option<&str>, flow: EDataFlow) -> Result<IMMDevice, String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("CoCreateInstance(MMDeviceEnumerator) failed: {e}"))?;

        if let Some(id) = device_id {
            let trimmed = id.trim();
            if !trimmed.is_empty() {
                let wide: Vec<u16> = trimmed.encode_utf16().chain(std::iter::once(0)).collect();
                if let Ok(dev) = enumerator.GetDevice(PWSTR(wide.as_ptr() as *mut u16)) {
                    return Ok(dev);
                }
            }
        }

        enumerator
            .GetDefaultAudioEndpoint(flow, eConsole)
            .map_err(|e| format!("GetDefaultAudioEndpoint({flow:?}) failed: {e}"))
    }
}

/// Auto-detects the first available silent WHQL dummy sink endpoint ID, if present.
pub fn find_silent_dummy_sink_endpoint() -> Option<AudioEndpointInfo> {
    let endpoints = enumerate_audio_endpoints().ok()?;
    endpoints
        .into_iter()
        .find(|ep| ep.flow == "Render" && ep.is_dummy_sink_candidate && !ep.is_default)
}

/// Queries the executable path and filename for `pid` using ONLY `PROCESS_QUERY_LIMITED_INFORMATION` (0x1000).
/// Strictly obeys the Anti-Cheat Ring-3 Law: never requests VM_READ, VM_WRITE, or ALL_ACCESS.
pub fn query_process_image_info(pid: u32) -> Option<(String, String)> {
    if pid == 0 || pid == 4 {
        return None;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(handle);

        if !ok || size == 0 {
            return None;
        }
        let full_path = String::from_utf16_lossy(&buf[..size as usize]);
        let exe_name = std::path::Path::new(&full_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.exe")
            .to_string();
        Some((exe_name, full_path))
    }
}

/// Scans all active Render endpoints via `IAudioSessionManager2` to discover applications
/// with active or ready audio sessions.
pub fn list_active_audio_processes() -> Result<Vec<ActiveAudioProcess>, String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("CoCreateInstance(MMDeviceEnumerator) failed: {e}"))?;

        let collection = enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .map_err(|e| format!("EnumAudioEndpoints(eRender) failed: {e}"))?;

        let self_pid = std::process::id();
        let mut by_pid: BTreeMap<u32, ActiveAudioProcess> = BTreeMap::new();

        let count = collection.GetCount().unwrap_or(0);
        for idx in 0..count {
            let Ok(device) = collection.Item(idx) else {
                continue;
            };
            let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                continue;
            };
            let Ok(session_enum) = manager.GetSessionEnumerator() else {
                continue;
            };
            let session_count = session_enum.GetCount().unwrap_or(0);

            for s_idx in 0..session_count {
                let Ok(ctrl) = session_enum.GetSession(s_idx) else {
                    continue;
                };
                let Ok(ctrl2) = ctrl.cast::<IAudioSessionControl2>() else {
                    continue;
                };
                let Ok(pid) = ctrl2.GetProcessId() else {
                    continue;
                };
                if pid == self_pid {
                    continue;
                }

                if pid == 0 {
                    if !by_pid.contains_key(&0) {
                        by_pid.insert(
                            0,
                            ActiveAudioProcess {
                                pid: 0,
                                exe_name: "SystemSounds".to_string(),
                                exe_path: "C:\\Windows\\System32\\AudioSrv.dll".to_string(),
                                display_name: "System Sounds".to_string(),
                                assigned_bus: None,
                                is_routed: false,
                            },
                        );
                    }
                    continue;
                }

                if by_pid.contains_key(&pid) {
                    continue;
                }

                let Some((exe_name, exe_path)) = query_process_image_info(pid) else {
                    continue;
                };

                let session_display = ctrl
                    .GetDisplayName()
                    .ok()
                    .and_then(|pwstr| {
                        let s = pwstr.to_string().ok();
                        CoTaskMemFree(Some(pwstr.0 as *const _));
                        s
                    })
                    .filter(|s| !s.trim().is_empty() && !s.starts_with('@'))
                    .unwrap_or_else(|| exe_name.clone());

                by_pid.insert(
                    pid,
                    ActiveAudioProcess {
                        pid,
                        exe_name,
                        exe_path,
                        display_name: session_display,
                        assigned_bus: None,
                        is_routed: false,
                    },
                );
            }
        }

        // Ensure System Sounds is always available in the application pool
        if !by_pid.contains_key(&0) {
            by_pid.insert(
                0,
                ActiveAudioProcess {
                    pid: 0,
                    exe_name: "SystemSounds".to_string(),
                    exe_path: "C:\\Windows\\System32\\AudioSrv.dll".to_string(),
                    display_name: "System Sounds".to_string(),
                    assigned_bus: None,
                    is_routed: false,
                },
            );
        }

        Ok(by_pid.into_values().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    #[test]
    fn test_enumerate_endpoints_and_sessions() {
        let endpoints = enumerate_audio_endpoints();
        if let Ok(ref eps) = endpoints {
            for ep in eps {
                println!("[TEST_EP] flow={} default={} dummy={} mic={} name='{}' id='{}'", ep.flow, ep.is_default, ep.is_dummy_sink_candidate, ep.is_virtual_mic_candidate, ep.name, ep.id);
            }
        }
        let sessions = list_active_audio_processes();
        assert!(
            sessions.is_ok(),
            "list_active_audio_processes failed: {:?}",
            sessions.err()
        );
    }
}
