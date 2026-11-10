//! Dedicated recording endpoint. Independent process captures never redirect Windows playback.
use super::engine::{soft_limit_sample, MAX_BLOCK_FRAMES, MAX_BLOCK_SAMPLES, MAX_MANAGED_PIDS};
use super::resampler::AdaptiveResampler;
use super::ring_stream::{create_pid_ring_stream, PidRingConsumer, PidStreamMeta};
use crate::dsp::meter::SharedMeterBank;
use crate::interop::loopback::{standard_float32_stereo_format, ProcessLoopbackWorker};
use crate::interop::sessions::{resolve_mmdevice, ActiveAudioProcess};
use crate::state::config::{BusId, StreamConfig, StreamMicSource};
use rtrb::{Consumer, Producer, RingBuffer};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{eRender, IAudioClient, IAudioRenderClient,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{AvRevertMmThreadCharacteristics,
    AvSetMmThreadCharacteristicsW, CreateEventW, WaitForSingleObject};

struct Capture {
    meta: Arc<PidStreamMeta>,
    worker: ProcessLoopbackWorker,
}

struct Slot {
    consumer: PidRingConsumer,
    resampler: AdaptiveResampler,
    current_gain: f32,
}

fn selected_pids(cfg: &StreamConfig, processes: &[ActiveAudioProcess], own_pid: u32) -> BTreeSet<u32> {
    if !cfg.enabled || cfg.output_device_id.is_none() { return BTreeSet::new(); }
    let unique: BTreeSet<u32> = processes.iter()
        .filter(|p| p.pid != own_pid && cfg.includes_app(&p.exe_name,
            p.assigned_bus.as_deref().and_then(|name|
                BusId::PLAYBACK_BUSES.into_iter().find(|bus| bus.as_str() == name))))
        .map(|p| p.pid).collect();
    unique.into_iter().take(MAX_MANAGED_PIDS).collect()
}

pub struct StreamOutput {
    captures: BTreeMap<u32, Capture>,
    commands: Producer<PidRingConsumer>,
    gain: Arc<AtomicU32>,
    include_mic: Arc<AtomicBool>,
    /// True when the mic tap should be taken after the fader/virtual-mic levels (default).
    mic_post_fader: Arc<AtomicBool>,
    channel_gains: Arc<[AtomicU32; 5]>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub error: Arc<Mutex<Option<String>>>,
}

impl StreamOutput {
    pub fn start(cfg: &StreamConfig, mic: PidRingConsumer, meters: Arc<SharedMeterBank>) -> Self {
        let (commands, receiver) = RingBuffer::new(64);
        let gain = Arc::new(AtomicU32::new(0));
        let include_mic = Arc::new(AtomicBool::new(false));
        let channel_gains = Arc::new(std::array::from_fn(|_| AtomicU32::new(1.0_f32.to_bits())));
        let stop = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));
        let mic_post_fader = Arc::new(AtomicBool::new(true));
        let mut output = Self { captures: BTreeMap::new(), commands, gain, include_mic, mic_post_fader,
            channel_gains, stop, thread: None, error };
        output.update(cfg);
        // Never fall back to a default endpoint: the user must explicitly choose a cable.
        if let Some(endpoint) = cfg.output_device_id.clone() {
            let gain = Arc::clone(&output.gain);
            let include_mic = Arc::clone(&output.include_mic);
            let channel_gains = Arc::clone(&output.channel_gains);
            let stop = Arc::clone(&output.stop);
            let error = Arc::clone(&output.error);
            let thread_error = Arc::clone(&error);
            match thread::Builder::new().name("stellars-stream-output".into()).spawn(move || {
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                    let mut index = 0;
                    let priority = AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut index).ok();
                    if let Err(message) = render(&endpoint, receiver, mic, &gain, &include_mic, &channel_gains, &meters, &stop) {
                        if let Ok(mut err) = thread_error.lock() { *err = Some(message); }
                    }
                    meters.stream_running.store(false, Ordering::Release);
                    meters.stream.update_from_interleaved(&[0.0; 2]);
                    meters.stream.set_lufs(crate::dsp::meter::SILENCE_LUFS);
                    if let Some(priority) = priority { let _ = AvRevertMmThreadCharacteristics(priority); }
                    CoUninitialize();
                }
            }) {
                Ok(thread) => output.thread = Some(thread),
                Err(err) => { if let Ok(mut error) = error.lock() { *error = Some(err.to_string()); } }
            }
        }
        output
    }

    pub fn update(&self, cfg: &StreamConfig) {
        let gain = if cfg.enabled && !cfg.muted { cfg.volume } else { 0.0 };
        self.gain.store(gain.to_bits(), Ordering::Relaxed);
        for bus in BusId::ALL {
            let channel = cfg.channels.get(&bus).copied().unwrap_or_default();
            self.channel_gains[bus.index()].store(channel.gain().to_bits(), Ordering::Relaxed);
        }
        self.include_mic.store(cfg.enabled && cfg.categories.contains(&BusId::Mic), Ordering::Relaxed);
        self.mic_post_fader.store(cfg.mic_source == StreamMicSource::Virtual, Ordering::Relaxed);
    }

    /// Shared with the mic thread so it knows where to tap the mic.
    pub fn mic_post_fader_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.mic_post_fader)
    }

    /// Runs on the control thread, never inside WASAPI's render callback.
    pub fn sync(&mut self, cfg: &StreamConfig, processes: &[ActiveAudioProcess]) {
        if self.stop.load(Ordering::Acquire) { return; }
        self.update(cfg);
        let desired = selected_pids(cfg, processes, std::process::id());
        self.captures.retain(|pid, capture| {
            if desired.contains(pid) { true } else { capture.meta.stop(); capture.worker.stop(); false }
        });
        for pid in desired {
            let bus = processes.iter().find(|p| p.pid == pid)
                .and_then(|p| p.assigned_bus.as_deref())
                .and_then(|name| BusId::PLAYBACK_BUSES.into_iter().find(|bus| bus.as_str() == name))
                .unwrap_or(BusId::Aux);
            if let Some(capture) = self.captures.get(&pid) {
                capture.meta.set_bus(bus);
                continue;
            }
            let (producer, consumer) = create_pid_ring_stream(pid, bus);
            let meta = Arc::clone(&producer.meta);
            if self.commands.push(consumer).is_err() { continue; }
            let worker = ProcessLoopbackWorker::spawn(pid, producer);
            self.captures.insert(pid, Capture { meta, worker });
        }
    }

    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        for capture in self.captures.values_mut() { capture.meta.stop(); capture.worker.stop(); }
        self.captures.clear();
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}

impl Drop for StreamOutput { fn drop(&mut self) { self.shutdown(); } }

struct Event(HANDLE);
impl Drop for Event { fn drop(&mut self) { unsafe { let _ = CloseHandle(self.0); } } }

#[allow(clippy::too_many_arguments)]
unsafe fn render(endpoint: &str, mut commands: Consumer<PidRingConsumer>,
    mut mic: PidRingConsumer, gain: &AtomicU32, include_mic: &AtomicBool,
    channel_gains: &[AtomicU32; 5], meters: &SharedMeterBank, stop: &AtomicBool) -> Result<(), String> {
    let device = resolve_mmdevice(Some(endpoint), eRender)?;
    let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(|e| e.to_string())?;
    let format = standard_float32_stereo_format();
    client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK
        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
        100_000, 0, &format, None).map_err(|e| format!("Stream output could not open: {e}"))?;
    let event = Event(CreateEventW(None, false, false, None).map_err(|e| e.to_string())?);
    client.SetEventHandle(event.0).map_err(|e| e.to_string())?;
    let capacity = client.GetBufferSize().map_err(|e| e.to_string())?;
    let renderer: IAudioRenderClient = client.GetService().map_err(|e| e.to_string())?;
    let mut slots: [Option<Slot>; MAX_MANAGED_PIDS] = std::array::from_fn(|_| None);
    let mut scratch = [0.0; MAX_BLOCK_SAMPLES];
    let mut source_scratch = [0.0; MAX_BLOCK_SAMPLES];
    let mut mic_gain = 0.0;
    let mut mic_scratch = [0.0; MAX_BLOCK_SAMPLES];
    let mut mic_resampler = AdaptiveResampler::new();
    let mut stream_lufs = crate::dsp::meter::LufsMeter::new();
    client.Start().map_err(|e| e.to_string())?;
    meters.stream_running.store(true, Ordering::Release);
    // All storage and clients exist before the RT loop: no allocations or mutex locks below.
    let result = (|| -> Result<(), String> {
        while !stop.load(Ordering::Relaxed) {
            if WaitForSingleObject(event.0, 50) != WAIT_OBJECT_0 { continue; }
            for slot in slots.iter_mut() {
                if slot.as_ref().is_some_and(|s| !s.consumer.meta.is_active()) { *slot = None; }
            }
            while let Ok(consumer) = commands.pop() {
                if let Some(slot) = slots.iter_mut().find(|slot| slot.is_none()) {
                    *slot = Some(Slot { consumer, resampler: AdaptiveResampler::new(), current_gain: 0.0 });
                }
            }
            let padding = client.GetCurrentPadding().map_err(|e| e.to_string())?;
            let frames = capacity.saturating_sub(padding).min(MAX_BLOCK_FRAMES as u32);
            if frames == 0 { continue; }
            let len = frames as usize * 2;
            scratch[..len].fill(0.0);
            let mut bus_sends = [SendStats::default(); 5];
            for slot in slots.iter_mut().flatten() {
                if slot.consumer.meta.is_active() {
                    source_scratch[..len].fill(0.0);
                    slot.resampler.process_and_accumulate(&mut slot.consumer, &mut source_scratch[..len]);
                    let target = f32::from_bits(channel_gains[slot.consumer.meta.bus().index()].load(Ordering::Relaxed));
                    let stats = accumulate_send(&source_scratch[..len], &mut scratch[..len], &mut slot.current_gain, target);
                    bus_sends[slot.consumer.meta.bus().index()].merge(stats);
                }
            }
            // Always drain the mic tap, even while excluded, to avoid replaying buffered speech.
            mic_scratch[..len].fill(0.0);
            mic_resampler.process_and_accumulate(&mut mic, &mut mic_scratch[..len]);
            let mic_on = include_mic.load(Ordering::Relaxed);
            let target = if mic_on { f32::from_bits(channel_gains[BusId::Mic.index()].load(Ordering::Relaxed)) } else { 0.0 };
            bus_sends[BusId::Mic.index()].merge(accumulate_send(&mic_scratch[..len], &mut scratch[..len], &mut mic_gain, target));
            let level = f32::from_bits(gain.load(Ordering::Relaxed));
            let frames_f = (len / 2).max(1) as f32;
            for (meter, send) in meters.stream_sends.iter().zip(bus_sends) {
                meter.update_from_stats(
                    send.peak_left * level,
                    send.peak_right * level,
                    (send.sum_sq_left / frames_f).sqrt() * level,
                    (send.sum_sq_right / frames_f).sqrt() * level,
                );
            }
            for i in 0..len {
                scratch[i] = soft_limit_sample(scratch[i] * level);
            }
            let buffer = renderer.GetBuffer(frames).map_err(|e| e.to_string())?;
            std::slice::from_raw_parts_mut(buffer as *mut f32, len).copy_from_slice(&scratch[..len]);
            renderer.ReleaseBuffer(frames, 0).map_err(|e| e.to_string())?;
            meters.stream.update_from_interleaved(&scratch[..len]);
            meters.stream.set_lufs(stream_lufs.process(&scratch[..len]));
        }
        Ok(())
    })();
    let _ = client.Stop();
    result
}

// Five millisecond gain smoothing at 48 kHz; both stereo samples share a gain.
/// Peak and energy of what one source added to the mix during a block.
#[derive(Clone, Copy, Default)]
struct SendStats {
    peak_left: f32,
    peak_right: f32,
    sum_sq_left: f32,
    sum_sq_right: f32,
}

impl SendStats {
    /// Several sources of one channel: peaks take the maximum, energy adds up.
    fn merge(&mut self, other: SendStats) {
        self.peak_left = self.peak_left.max(other.peak_left);
        self.peak_right = self.peak_right.max(other.peak_right);
        self.sum_sq_left += other.sum_sq_left;
        self.sum_sq_right += other.sum_sq_right;
    }
}

fn accumulate_send(source: &[f32], output: &mut [f32], current: &mut f32, target: f32) -> SendStats {
    let mut stats = SendStats::default();
    for (input, out) in source.chunks_exact(2).zip(output.chunks_exact_mut(2)) {
        *current += (target - *current) * (1.0 / 240.0);
        if (*current - target).abs() < 0.00001 { *current = target; }
        let (l, r) = (input[0] * *current, input[1] * *current);
        out[0] += l;
        out[1] += r;
        stats.peak_left = stats.peak_left.max(l.abs());
        stats.peak_right = stats.peak_right.max(r.abs());
        stats.sum_sq_left += l * l;
        stats.sum_sq_right += r * r;
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(pid: u32, name: &str, bus: Option<&str>) -> ActiveAudioProcess {
        ActiveAudioProcess { pid, exe_name: name.into(), exe_path: String::new(),
            display_name: name.into(), assigned_bus: bus.map(str::to_string), is_routed: bus.is_some() }
    }

    #[test]
    fn independent_sends_scale_and_mute_audio_with_stereo_smoothing() {
        let mut output = [0.0; 4];
        let mut game = 0.25;
        accumulate_send(&[1.0, 0.5, 1.0, 0.5], &mut output, &mut game, 0.25);
        let mut chat = 0.5;
        accumulate_send(&[0.5; 4], &mut output, &mut chat, 0.5);
        assert_eq!(output, [0.5, 0.375, 0.5, 0.375]);
        let mut current = 1.0;
        let input = [1.0; 4800];
        let mut muted = [0.0; 4800];
        accumulate_send(&input, &mut muted, &mut current, 0.0);
        assert!(muted[0] < 1.0 && muted[0] > 0.99);
        assert_eq!(muted[0], muted[1]);
        assert!(muted[4799] < 0.0001);
        let mut silent = [0.0; 4];
        current = 0.0;
        accumulate_send(&[1.0; 4], &mut silent, &mut current, 0.0);
        assert_eq!(silent, [0.0; 4]);
    }

    #[test]
    fn stream_deduplicates_category_and_app_selection_and_excludes_self() {
        let mut cfg = StreamConfig { enabled: true, output_device_id: Some("cable".into()), ..StreamConfig::default() };
        cfg.app_overrides.insert("game.exe".into(), true);
        cfg.app_overrides.insert("chat.exe".into(), false);
        cfg.app_overrides.insert("browser.exe".into(), true);
        let apps = vec![app(1, "game.exe", Some("Game")), app(1, "game.exe", Some("Game")),
            app(2, "chat.exe", Some("Chat")), app(3, "browser.exe", None), app(4, "self.exe", Some("Game"))];
        assert_eq!(selected_pids(&cfg, &apps, 4), BTreeSet::from([1, 3]));
        cfg.enabled = false;
        assert!(selected_pids(&cfg, &apps, 4).is_empty());
        cfg.enabled = true;
        cfg.output_device_id = None;
        assert!(selected_pids(&cfg, &apps, 4).is_empty());
    }

    #[test]
    fn stream_selection_respects_fixed_realtime_capacity() {
        let cfg = StreamConfig { enabled: true, output_device_id: Some("cable".into()), ..StreamConfig::default() };
        let apps: Vec<_> = (1..=40).map(|pid| app(pid, "game.exe", Some("Game"))).collect();
        assert_eq!(selected_pids(&cfg, &apps, 999).len(), MAX_MANAGED_PIDS);
    }

    #[test]
    fn send_stats_report_the_level_each_source_adds() {
        let mut out = [0.0; 4];
        let mut gain = 0.5;
        let stats = accumulate_send(&[1.0, 0.5, 1.0, 0.5], &mut out, &mut gain, 0.5);
        assert!((stats.peak_left - 0.5).abs() < 1e-6);
        assert!((stats.peak_right - 0.25).abs() < 1e-6);
        assert!((stats.sum_sq_left - 0.5).abs() < 1e-6);
        let mut merged = SendStats::default();
        merged.merge(stats);
        merged.merge(stats);
        assert!((merged.peak_left - 0.5).abs() < 1e-6, "peaks take the max");
        assert!((merged.sum_sq_left - 1.0).abs() < 1e-6, "energy adds");
    }
}
