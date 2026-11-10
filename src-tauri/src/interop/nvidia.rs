//! NVIDIA overlay / ShadowPlay (Instant Replay & recording) microphone selection.
//!
//! NVIDIA keeps its recording settings in
//! `HKCU\Software\NVIDIA Corporation\Global\ShadowPlay\NVSPCAPS`. The selected recording
//! microphone is stored as two UTF-16, zero-padded `REG_BINARY` values (520 bytes each):
//! the friendly name and the Windows endpoint id (`{0.0.1.00000000}.{guid}`).
//! Both are empty when the overlay follows the Windows default microphone.

use serde::Serialize;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_READ, KEY_SET_VALUE, REG_BINARY, REG_VALUE_TYPE,
};

const NVSPCAPS_KEY: PCWSTR = w!("Software\\NVIDIA Corporation\\Global\\ShadowPlay\\NVSPCAPS");
const MIC_NAME_VALUE: PCWSTR = w!("{FCDCC7A2-EF61-4782-AF8A-74FC89A6A11A}");
const MIC_ID_VALUE: PCWSTR = w!("{E8541E5B-8DA8-4BB1-82B5-F841FDA12F5B}");
const FIELD_BYTES: usize = 520;

#[derive(Debug, Clone, Serialize, Default)]
pub struct NvidiaRecordingStatus {
    /// NVIDIA ShadowPlay settings exist and recording/overlay is enabled.
    pub available: bool,
    #[serde(rename = "micDeviceId")]
    pub mic_device_id: Option<String>,
    #[serde(rename = "micDeviceName")]
    pub mic_device_name: Option<String>,
}

struct RegKey(HKEY);
impl Drop for RegKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<RegKey> {
    unsafe {
        let mut key = HKEY::default();
        let r = RegOpenKeyExW(HKEY_CURRENT_USER, NVSPCAPS_KEY, 0, access, &mut key);
        if r == ERROR_SUCCESS {
            Some(RegKey(key))
        } else {
            None
        }
    }
}

fn read_bytes(key: &RegKey, name: PCWSTR) -> Option<Vec<u8>> {
    unsafe {
        let mut kind = REG_VALUE_TYPE(0);
        let mut len: u32 = 0;
        if RegQueryValueExW(key.0, name, None, Some(&mut kind), None, Some(&mut len)) != ERROR_SUCCESS {
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        if RegQueryValueExW(
            key.0,
            name,
            None,
            Some(&mut kind),
            Some(buf.as_mut_ptr()),
            Some(&mut len),
        ) != ERROR_SUCCESS
        {
            return None;
        }
        buf.truncate(len as usize);
        Some(buf)
    }
}

fn read_flag(key: &RegKey, name: PCWSTR) -> bool {
    read_bytes(key, name)
        .map(|b| b.iter().any(|&x| x != 0))
        .unwrap_or(false)
}

fn decode_utf16(bytes: &[u8]) -> Option<String> {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    if units.is_empty() {
        None
    } else {
        Some(String::from_utf16_lossy(&units))
    }
}

fn encode_field(text: &str) -> Vec<u8> {
    let mut out = vec![0u8; FIELD_BYTES];
    // Always keep a trailing NUL: at most (FIELD_BYTES / 2 - 1) UTF-16 units.
    for (i, u) in text.encode_utf16().take(FIELD_BYTES / 2 - 1).enumerate() {
        let b = u.to_le_bytes();
        out[i * 2] = b[0];
        out[i * 2 + 1] = b[1];
    }
    out
}

fn write_bytes(key: &RegKey, name: PCWSTR, data: &[u8]) -> Result<(), String> {
    unsafe {
        let r = RegSetValueExW(key.0, name, 0, REG_BINARY, Some(data));
        if r == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("Could not write NVIDIA setting (error {})", r.0))
        }
    }
}

pub fn get_nvidia_recording_status() -> NvidiaRecordingStatus {
    let Some(key) = open(KEY_READ) else {
        return NvidiaRecordingStatus::default();
    };
    let enabled = read_flag(&key, w!("IsShadowPlayEnabled"))
        || read_flag(&key, w!("IsShadowPlayEnabledUser"))
        || read_flag(&key, w!("RecEnabled"));
    if !enabled {
        return NvidiaRecordingStatus::default();
    }
    NvidiaRecordingStatus {
        available: true,
        mic_device_id: read_bytes(&key, MIC_ID_VALUE).and_then(|b| decode_utf16(&b)),
        mic_device_name: read_bytes(&key, MIC_NAME_VALUE).and_then(|b| decode_utf16(&b)),
    }
}

/// Sets the NVIDIA recording microphone. `None` restores "follow Windows default".
pub fn set_nvidia_recording_mic(device: Option<(&str, &str)>) -> Result<(), String> {
    if !get_nvidia_recording_status().available {
        return Err("NVIDIA recording is not enabled on this PC".to_string());
    }
    let key = open(KEY_SET_VALUE).ok_or("Could not open NVIDIA recording settings")?;
    let (id, name) = device.unwrap_or(("", ""));
    write_bytes(&key, MIC_NAME_VALUE, &encode_field(name))?;
    write_bytes(&key, MIC_ID_VALUE, &encode_field(id))?;
    Ok(())
}
