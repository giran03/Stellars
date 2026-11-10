#![allow(non_snake_case)]

use windows::core::{h, interface, GUID, HSTRING, HRESULT};
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, eRender, EDataFlow, ERole,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::WinRT::RoGetActivationFactory;

pub const DEVINTERFACE_AUDIO_RENDER_GUID: &str = "{e6327cad-dcec-4949-ae8a-991e976a79d2}";
pub const DEVINTERFACE_AUDIO_CAPTURE_GUID: &str = "{2eef81be-33fa-4800-9670-1cd474972c3f}";

// WinRT IInspectable base interface (3 vtable slots after IUnknown's 3 slots = 6 base slots total)
#[interface("af86e2e0-b12d-4c6a-9c5a-d7aa65101e90")]
pub unsafe trait IInspectableBase: windows::core::IUnknown {
    fn GetIids(&self, count: *mut u32, values: *mut *mut GUID) -> HRESULT;
    fn GetRuntimeClassName(&self, value: *mut *mut core::ffi::c_void) -> HRESULT;
    fn GetTrustLevel(&self, value: *mut i32) -> HRESULT;
}

// IAudioPolicyConfig derives from IInspectable (6 base vtable slots: IUnknown(3) + IInspectable(3))
// SetPersistedDefaultAudioEndpoint sits at slot index 25 (19 dummy slots after IInspectable)
#[interface("ab3d4648-e242-459f-b02f-541c70306324")]
pub unsafe trait IAudioPolicyConfig: IInspectableBase {
    // Slots 6..=24: Reserved vtable padding (19 usize function pointers)
    fn _pad6(&self) -> HRESULT;
    fn _pad7(&self) -> HRESULT;
    fn _pad8(&self) -> HRESULT;
    fn _pad9(&self) -> HRESULT;
    fn _pad10(&self) -> HRESULT;
    fn _pad11(&self) -> HRESULT;
    fn _pad12(&self) -> HRESULT;
    fn _pad13(&self) -> HRESULT;
    fn _pad14(&self) -> HRESULT;
    fn _pad15(&self) -> HRESULT;
    fn _pad16(&self) -> HRESULT;
    fn _pad17(&self) -> HRESULT;
    fn _pad18(&self) -> HRESULT;
    fn _pad19(&self) -> HRESULT;
    fn _pad20(&self) -> HRESULT;
    fn _pad21(&self) -> HRESULT;
    fn _pad22(&self) -> HRESULT;
    fn _pad23(&self) -> HRESULT;
    fn _pad24(&self) -> HRESULT;

    // Slot 25: Redirects target PID to endpoint (pass HSTRING::new() to reset to system default)
    pub unsafe fn SetPersistedDefaultAudioEndpoint(
        &self,
        process_id: u32,
        flow: EDataFlow,
        role: ERole,
        device_id: *const HSTRING,
    ) -> HRESULT;

    // Slot 26: Reads current endpoint for PID
    pub unsafe fn GetPersistedDefaultAudioEndpoint(
        &self,
        process_id: u32,
        flow: EDataFlow,
        role: ERole,
        device_id: *mut HSTRING,
    ) -> HRESULT;

    // Slot 27: Clears all persisted routing rules
    pub unsafe fn ClearAllPersistedApplicationDefaultEndpoints(&self) -> HRESULT;
}

/// Formats an MMDevice ID (`{0.0.0.00000000}.{guid}`) into the SWD MMDEVAPI interface path
/// expected by `IAudioPolicyConfig::SetPersistedDefaultAudioEndpoint`.
pub fn format_policy_endpoint_id(mmdevice_id: &str, flow: EDataFlow) -> HSTRING {
    let trimmed = mmdevice_id.trim();
    if trimmed.is_empty() {
        return HSTRING::new();
    }
    if trimmed.starts_with(r"\\?\SWD#MMDEVAPI#") {
        return HSTRING::from(trimmed);
    }
    let class_guid = if flow == eCapture {
        DEVINTERFACE_AUDIO_CAPTURE_GUID
    } else {
        DEVINTERFACE_AUDIO_RENDER_GUID
    };
    let formatted = format!(r"\\?\SWD#MMDEVAPI#{trimmed}#{class_guid}");
    HSTRING::from(formatted.as_str())
}

fn activate_policy_config() -> Result<IAudioPolicyConfig, String> {
    unsafe {
        // Initialize STA if COM is not yet initialized on this thread (compatible with Tauri/OleInitialize)
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        RoGetActivationFactory::<IAudioPolicyConfig>(h!("Windows.Media.Internal.AudioPolicyConfig"))
            .map_err(|e| format!("RoGetActivationFactory(AudioPolicyConfig) failed: {e}"))
    }
}

/// Redirects `process_id` audio for `eConsole`, `eMultimedia`, and `eCommunications` roles
/// to `target_mmdevice_id` (or resets to system default when `None` / empty).
pub fn set_process_default_endpoint(
    process_id: u32,
    flow: EDataFlow,
    target_mmdevice_id: Option<&str>,
) -> Result<(), String> {
    let policy = activate_policy_config()?;
    let hstr = match target_mmdevice_id {
        Some(id) if !id.trim().is_empty() => format_policy_endpoint_id(id, flow),
        _ => HSTRING::new(),
    };

    unsafe {
        for role in [eConsole, eMultimedia, eCommunications] {
            let raw_hstr_abi: *const HSTRING = std::mem::transmute_copy(&hstr);
            let hr = policy.SetPersistedDefaultAudioEndpoint(
                process_id,
                flow,
                role,
                raw_hstr_abi,
            );
            if hr.is_err() {
                return Err(format!(
                    "SetPersistedDefaultAudioEndpoint(pid={process_id}, role={role:?}) failed: 0x{:08X}",
                    hr.0 as u32
                ));
            }
        }
    }
    Ok(())
}

/// Convenience helper to route a target application's Render stream to the silent dummy sink.
pub fn route_pid_to_dummy_sink(process_id: u32, dummy_sink_mmdevice_id: &str) -> Result<(), String> {
    set_process_default_endpoint(process_id, eRender, Some(dummy_sink_mmdevice_id))
}

/// Resets a target application's Render stream back to the Windows default output device.
pub fn reset_pid_to_default_endpoint(process_id: u32) -> Result<(), String> {
    set_process_default_endpoint(process_id, eRender, None)
}

/// Queries the currently persisted Render endpoint ID for `process_id`.
pub fn get_process_default_endpoint(
    process_id: u32,
    flow: EDataFlow,
) -> Result<Option<String>, String> {
    let policy = activate_policy_config()?;
    unsafe {
        let mut out_hstr = HSTRING::new();
        let hr = policy.GetPersistedDefaultAudioEndpoint(
            process_id,
            flow,
            eConsole,
            &mut out_hstr as *mut HSTRING,
        );
        if hr.is_err() {
            return Err(format!(
                "GetPersistedDefaultAudioEndpoint(pid={process_id}) failed: 0x{:08X}",
                hr.0 as u32
            ));
        }
        let s = out_hstr.to_string_lossy();
        if s.is_empty() {
            Ok(None)
        } else {
            Ok(Some(s))
        }
    }
}

/// Clears all persisted per-application default audio endpoints across Windows.
pub fn clear_all_persisted_endpoints() -> Result<(), String> {
    let policy = activate_policy_config()?;
    unsafe {
        let hr = policy.ClearAllPersistedApplicationDefaultEndpoints();
        if hr.is_err() {
            return Err(format!(
                "ClearAllPersistedApplicationDefaultEndpoints failed: 0x{:08X}",
                hr.0 as u32
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    

    #[test]
    fn test_format_policy_endpoint_id() {
        let mm_id = "{0.0.0.00000000}.{12345678-1234-1234-1234-123456789abc}";
        let render_hstr = format_policy_endpoint_id(mm_id, eRender);
        assert_eq!(
            render_hstr.to_string_lossy(),
            r"\\?\SWD#MMDEVAPI#{0.0.0.00000000}.{12345678-1234-1234-1234-123456789abc}#{e6327cad-dcec-4949-ae8a-991e976a79d2}"
        );

        let capture_hstr = format_policy_endpoint_id(mm_id, eCapture);
        assert_eq!(
            capture_hstr.to_string_lossy(),
            r"\\?\SWD#MMDEVAPI#{0.0.0.00000000}.{12345678-1234-1234-1234-123456789abc}#{2eef81be-33fa-4800-9670-1cd474972c3f}"
        );
    }

    #[test]
    fn test_vtable_slot_offsets() {
        // Verify IAudioPolicyConfig_Vtbl has exact 28 pointers (slots 0..=27)
        assert_eq!(
            std::mem::size_of::<IAudioPolicyConfig_Vtbl>(),
            28 * std::mem::size_of::<usize>()
        );
    }

    #[test]
    fn test_audio_policy_config_com_activation() {
        let policy = activate_policy_config();
        assert!(
            policy.is_ok(),
            "Expected Windows.Media.Internal.AudioPolicyConfig factory activation to succeed: {:?}",
            policy.err()
        );
        let self_pid = std::process::id();
        let current = get_process_default_endpoint(self_pid, eRender);
        assert!(
            current.is_ok(),
            "Expected GetPersistedDefaultAudioEndpoint to succeed: {:?}",
            current.err()
        );
    }
}
