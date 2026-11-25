//! Reads and sets the Windows (hardware) volume of an output device, the same level as the
//! Windows volume mixer. A missing or unset device falls back to the Windows default output,
//! exactly like the audio engine does.

use crate::interop::sessions::resolve_mmdevice;
use serde::Serialize;
use windows::Win32::Media::Audio::eRender;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::System::Com::CLSCTX_ALL;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputVolume {
    /// Device volume as a 0.0..=1.0 scalar (the percentage Windows shows).
    pub volume: f32,
    pub muted: bool,
}

fn endpoint(device_id: Option<&str>) -> Result<IAudioEndpointVolume, String> {
    let device = resolve_mmdevice(device_id, eRender)?;
    unsafe { device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }
        .map_err(|e| format!("Could not open the device volume: {e}"))
}

pub fn get_output_volume(device_id: Option<&str>) -> Result<OutputVolume, String> {
    let volume = endpoint(device_id)?;
    unsafe {
        Ok(OutputVolume {
            volume: volume
                .GetMasterVolumeLevelScalar()
                .map_err(|e| format!("Could not read the device volume: {e}"))?,
            muted: volume.GetMute().map(|m| m.as_bool()).unwrap_or(false),
        })
    }
}

pub fn set_output_volume(device_id: Option<&str>, level: f32) -> Result<f32, String> {
    let level = if level.is_finite() { level.clamp(0.0, 1.0) } else { 0.0 };
    let volume = endpoint(device_id)?;
    unsafe {
        volume
            .SetMasterVolumeLevelScalar(level, std::ptr::null())
            .map_err(|e| format!("Could not set the device volume: {e}"))?;
    }
    Ok(level)
}
