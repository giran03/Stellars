use crate::audio::ring_stream::PidRingProducer;
use std::mem::size_of;
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use windows::core::{implement, w, Interface, Result as WinResult, GUID, HRESULT, PROPVARIANT};
use windows::Win32::Foundation::{CloseHandle, HANDLE, S_OK, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl,
    IAudioCaptureClient, IAudioClient, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, WAVEFORMATEX,
};
use windows::Win32::Media::Multimedia::WAVE_FORMAT_IEEE_FLOAT;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW, SetEvent,
    WaitForSingleObject,
};

pub const PROCESS_LOOPBACK_MODE_INCLUDE_TREE: u32 = 0;
pub const PROCESS_LOOPBACK_MODE_EXCLUDE_TREE: u32 = 1;
pub const AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK: u32 = 1;
pub const VT_BLOB_VAL: u16 = 65;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AudioClientProcessLoopbackParams {
    pub target_process_id: u32,
    pub process_loopback_mode: u32, // 0 = INCLUDE_TARGET_PROCESS_TREE, 1 = EXCLUDE
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AudioClientActivationParams {
    pub activation_type: u32, // 1 = AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK
    pub process_loopback_params: AudioClientProcessLoopbackParams,
}

/// Binary-exact 64-bit Win32 PROPVARIANT layout for VT_BLOB (65).
#[repr(C)]
#[derive(Clone, Copy)]
struct RawPropVariantBlob {
    vt: u16,
    w_reserved1: u16,
    w_reserved2: u16,
    w_reserved3: u16,
    cb_size: u32,
    _pad: u32,
    p_blob_data: *mut u8,
}

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct LoopbackActivationCompletionHandler {
    event: HANDLE,
    result: Mutex<Option<WinResult<IAudioClient>>>,
}

impl LoopbackActivationCompletionHandler {
    fn new(event: HANDLE) -> Self {
        Self {
            event,
            result: Mutex::new(None),
        }
    }
}

impl IActivateAudioInterfaceCompletionHandler_Impl for LoopbackActivationCompletionHandler_Impl {
    fn ActivateCompleted(
        &self,
        activateoperation: Option<&IActivateAudioInterfaceAsyncOperation>,
    ) -> WinResult<()> {
        let outcome = unsafe {
            if let Some(op) = activateoperation {
                let mut activate_hr = HRESULT(0);
                let mut punk = None;
                op.GetActivateResult(&mut activate_hr, &mut punk)?;
                activate_hr.ok()?;
                let unk = punk.ok_or_else(|| windows::core::Error::from_hresult(HRESULT(-1)))?;
                unk.cast::<IAudioClient>()
            } else {
                Err(windows::core::Error::from_hresult(HRESULT(-1)))
            }
        };

        if let Ok(mut guard) = self.result.lock() {
            *guard = Some(outcome);
        }
        unsafe {
            let _ = SetEvent(self.event);
        }
        Ok(())
    }
}

/// Builds the canonical 48000 Hz, 32-bit IEEE Float, 2-channel interleaved stereo WAVEFORMATEX.
pub const fn standard_float32_stereo_format() -> WAVEFORMATEX {
    let channels = 2u16;
    let sample_rate = 48000u32;
    let bits_per_sample = 32u16;
    let block_align = (channels * bits_per_sample) / 8;
    let avg_bytes_per_sec = sample_rate * (block_align as u32);
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_IEEE_FLOAT as u16,
        nChannels: channels,
        nSamplesPerSec: sample_rate,
        nAvgBytesPerSec: avg_bytes_per_sec,
        nBlockAlign: block_align,
        wBitsPerSample: bits_per_sample,
        cbSize: 0,
    }
}

/// Activates a per-process WASAPI loopback `IAudioClient` via `VAD\Process_Loopback`.
/// Does NOT open a process handle to `target_pid` (100% Ring-3 Anti-Cheat safe).
pub fn activate_process_loopback_client(target_pid: u32) -> Result<IAudioClient, String> {
    unsafe {
        let mut params = AudioClientActivationParams {
            activation_type: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            process_loopback_params: AudioClientProcessLoopbackParams {
                target_process_id: target_pid,
                process_loopback_mode: PROCESS_LOOPBACK_MODE_INCLUDE_TREE,
            },
        };

        let raw_prop = RawPropVariantBlob {
            vt: VT_BLOB_VAL,
            w_reserved1: 0,
            w_reserved2: 0,
            w_reserved3: 0,
            cb_size: size_of::<AudioClientActivationParams>() as u32,
            _pad: 0,
            p_blob_data: &mut params as *mut _ as *mut u8,
        };

        let event = CreateEventW(None, true, false, None)
            .map_err(|e| format!("CreateEventW failed: {e}"))?;

        let handler_impl = LoopbackActivationCompletionHandler::new(event);
        let handler: IActivateAudioInterfaceCompletionHandler = handler_impl.into();

        let op_res = ActivateAudioInterfaceAsync(
            w!("VAD\\Process_Loopback"),
            &IAudioClient::IID as *const GUID,
            Some(&raw_prop as *const RawPropVariantBlob as *const PROPVARIANT),
            &handler,
        );

        if let Err(e) = op_res {
            let _ = CloseHandle(event);
            return Err(format!("ActivateAudioInterfaceAsync(pid={target_pid}) failed: {e}"));
        }

        let wait_status = WaitForSingleObject(event, 5000);
        let _ = CloseHandle(event);

        if wait_status != WAIT_OBJECT_0 {
            return Err(format!(
                "Timed out waiting for ActivateAudioInterfaceAsync on PID {target_pid}"
            ));
        }

        let handler_ref: &LoopbackActivationCompletionHandler =
            windows_core::AsImpl::as_impl(&handler);
        let mut guard = handler_ref
            .result
            .lock()
            .map_err(|_| "Failed to lock activation result".to_string())?;
        match guard.take() {
            Some(Ok(client)) => Ok(client),
            Some(Err(e)) => Err(format!(
                "Process loopback activation failed for PID {target_pid}: {e}"
            )),
            None => Err(format!(
                "Process loopback completion returned no result for PID {target_pid}"
            )),
        }
    }
}

pub struct ProcessLoopbackWorker {
    pub pid: u32,
    stop_meta: std::sync::Arc<crate::audio::ring_stream::PidStreamMeta>,
    join_handle: Option<JoinHandle<()>>,
}

impl ProcessLoopbackWorker {
    /// Spawns a dedicated MMCSS "Pro Audio" worker thread that captures per-process float32 stereo
    /// frames from `target_pid` into `producer`.
    pub fn spawn(target_pid: u32, mut producer: PidRingProducer) -> Self {
        let stop_meta = std::sync::Arc::clone(&producer.meta);
        let join_handle = thread::Builder::new()
            .name(format!("sonar-pid-loopback-{target_pid}"))
            .spawn(move || unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                let mut mmcss_task_idx = 0u32;
                let mmcss_handle =
                    AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut mmcss_task_idx).ok();

                let _ = run_loopback_capture_loop(target_pid, &mut producer);

                if let Some(h) = mmcss_handle {
                    let _ = AvRevertMmThreadCharacteristics(h);
                }
                CoUninitialize();
            })
            .ok();

        Self {
            pid: target_pid,
            stop_meta,
            join_handle,
        }
    }

    pub fn stop(&mut self) {
        self.stop_meta.stop();
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ProcessLoopbackWorker {
    fn drop(&mut self) {
        self.stop_meta.stop();
    }
}

unsafe fn run_loopback_capture_loop(
    target_pid: u32,
    producer: &mut PidRingProducer,
) -> Result<(), String> {
    let audio_client = activate_process_loopback_client(target_pid)?;
    let wave_format = standard_float32_stereo_format();

    let stream_flags = AUDCLNT_STREAMFLAGS_LOOPBACK
        | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;

    // 10 ms buffer duration (100,000 in 100-ns units)
    let hns_buffer_duration: i64 = 100_000;
    audio_client
        .Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            stream_flags,
            hns_buffer_duration,
            0,
            &wave_format,
            None,
        )
        .map_err(|e| format!("IAudioClient::Initialize(pid={target_pid}) failed: {e}"))?;

    let sample_event = CreateEventW(None, false, false, None)
        .map_err(|e| format!("CreateEventW(sample_event) failed: {e}"))?;

    if let Err(e) = audio_client.SetEventHandle(sample_event) {
        let _ = CloseHandle(sample_event);
        return Err(format!("SetEventHandle failed: {e}"));
    }

    let capture_client: IAudioCaptureClient = match audio_client.GetService() {
        Ok(c) => c,
        Err(e) => {
            let _ = CloseHandle(sample_event);
            return Err(format!("GetService(IAudioCaptureClient) failed: {e}"));
        }
    };

    if let Err(e) = audio_client.Start() {
        let _ = CloseHandle(sample_event);
        return Err(format!("IAudioClient::Start failed: {e}"));
    }

    // Pre-allocated stack silence block (max 1024 stereo frames = 2048 floats) for AUDCLNT_BUFFERFLAGS_SILENT
    let silence_frames = [0.0_f32; 2048];

    // Real-Time Capture Loop: Zero heap allocations, zero locks
    while producer.meta.is_active() {
        let wait_res = WaitForSingleObject(sample_event, 50);
        if wait_res != WAIT_OBJECT_0 {
            continue;
        }

        loop {
            let packet_frames = match capture_client.GetNextPacketSize() {
                Ok(n) => n,
                Err(_) => break,
            };
            if packet_frames == 0 {
                break;
            }

            let mut data_ptr: *mut u8 = std::ptr::null_mut();
            let mut frames_to_read: u32 = 0;
            let mut flags: u32 = 0;

            if capture_client
                .GetBuffer(
                    &mut data_ptr,
                    &mut frames_to_read,
                    &mut flags,
                    None,
                    None,
                )
                .is_err()
            {
                break;
            }

            let sample_count = (frames_to_read as usize) * 2;
            if (flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)) != 0 || data_ptr.is_null() {
                let mut rem = sample_count;
                while rem > 0 {
                    let chunk = rem.min(silence_frames.len());
                    producer.push_interleaved(&silence_frames[..chunk]);
                    rem -= chunk;
                }
            } else {
                let slice = std::slice::from_raw_parts(data_ptr as *const f32, sample_count);
                producer.push_interleaved(slice);
            }

            let _ = capture_client.ReleaseBuffer(frames_to_read);
        }
    }

    let _ = audio_client.Stop();
    let _ = CloseHandle(sample_event);
    let _ = S_OK;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_propvariant_blob_layout() {
        assert_eq!(size_of::<RawPropVariantBlob>(), size_of::<PROPVARIANT>());
        assert_eq!(size_of::<AudioClientActivationParams>(), 12);
    }



}
