use crate::audio::resampler::AdaptiveResampler;
use crate::audio::ring_stream::{
    create_pid_ring_stream, PidRingConsumer, PidRingProducer, PidStreamMeta, PID_RING_CAPACITY_SAMPLES,
};
use crate::dsp::biquad::DEFAULT_SAMPLE_RATE;
use crate::dsp::ducking::{block_rms, AtomicDuckParams, DuckState};
use crate::dsp::dynamics::{AtomicDynamicsParams, DynamicsProcessor};
use crate::dsp::eq_bank::{EqBank, EqBankControl};
use crate::dsp::extras::{AtomicExtrasParams, BusExtras};
use crate::dsp::meter::{AllBusMetersPayload, LufsMeter, SharedMeterBank};
use crate::interop::loopback::{standard_float32_stereo_format, ProcessLoopbackWorker};
use crate::interop::policy::{reset_pid_to_default_endpoint, route_pid_to_dummy_sink};
use crate::interop::sessions::{
    find_silent_dummy_sink_endpoint, list_active_audio_processes, query_process_image_info,
    resolve_mmdevice, ActiveAudioProcess,
};
use crate::state::config::{
    BusConfig, BusId, CompressorConfig, DeEsserConfig, DuckingConfig, EqBandConfig, LimiterConfig,
    NoiseGateConfig, NoiseSuppressionConfig, RouterConfig,
};
use rtrb::{Consumer, Producer, RingBuffer};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    eCapture, eRender, IAudioCaptureClient, IAudioClient, IAudioRenderClient,
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
};
use windows::Win32::System::Com::{
    CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW,
    WaitForSingleObject,
};

pub const MAX_MANAGED_PIDS: usize = 32;
pub const MAX_BLOCK_FRAMES: usize = 2048;
pub const MAX_BLOCK_SAMPLES: usize = MAX_BLOCK_FRAMES * 2;
/// -0.1 dBFS ceiling (~0.988553) for soft-limiter
pub const LIMITER_CEILING: f32 = 0.988_553_1;

/// Computes ChatMix crossfader gains `(G_Game, G_Chat)` for `m` in `[-1.0, +1.0]`.
/// Section 3.4:
/// G_Game(m) = 1.0 if m <= 0.0, else (1.0 - m)^2
/// G_Chat(m) = (1.0 + m)^2 if m < 0.0, else 1.0
#[inline(always)]
pub fn compute_chat_mix_gains(m: f32) -> (f32, f32) {
    let m_clamped = m.clamp(-1.0, 1.0);
    let g_game = if m_clamped <= 0.0 {
        1.0
    } else {
        let d = 1.0 - m_clamped;
        d * d
    };
    let g_chat = if m_clamped < 0.0 {
        let d = 1.0 + m_clamped;
        d * d
    } else {
        1.0
    };
    (g_game, g_chat)
}

/// Fast soft-clipper & -0.1 dBFS ceiling limiter.
#[inline(always)]
pub fn soft_limit_sample(x: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    (x / LIMITER_CEILING).tanh() * LIMITER_CEILING
}

/// Lock-free atomic scalar parameters for a single bus (`Pre-Gain`, `Post-Gain`, `Mute`).
pub struct AtomicBusScalars {
    pre_gain_lin: AtomicU32,
    post_gain_lin: AtomicU32,
    muted: AtomicBool,
}

impl AtomicBusScalars {
    pub fn new(cfg: &BusConfig) -> Arc<Self> {
        let s = Arc::new(Self {
            pre_gain_lin: AtomicU32::new(1.0_f32.to_bits()),
            post_gain_lin: AtomicU32::new(1.0_f32.to_bits()),
            muted: AtomicBool::new(false),
        });
        s.update_from_config(cfg);
        s
    }

    pub fn update_from_config(&self, cfg: &BusConfig) {
        let pre_lin = 10.0_f32.powf(cfg.pre_gain_db.clamp(-24.0, 24.0) * 0.05);
        self.pre_gain_lin.store(pre_lin.to_bits(), Ordering::Relaxed);
        self.post_gain_lin
            .store(cfg.volume.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
        self.muted.store(cfg.muted, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn pre_gain(&self) -> f32 {
        f32::from_bits(self.pre_gain_lin.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn post_gain(&self) -> f32 {
        if self.muted.load(Ordering::Relaxed) {
            0.0
        } else {
            f32::from_bits(self.post_gain_lin.load(Ordering::Relaxed))
        }
    }
}

/// Real-time DSP strip for a single Bus owned exclusively by the audio render thread.
struct RealTimeBusStrip {
    scalars: Arc<AtomicBusScalars>,
    eq: EqBank,
    dynamics: DynamicsProcessor,
    extras: BusExtras,
}

impl RealTimeBusStrip {
    #[inline]
    fn process_buffer(&mut self, interleaved_stereo: &mut [f32]) {
        self.process_pre_fader(interleaved_stereo);
        self.process_output_fader(interleaved_stereo);
        self.extras.process_limiter(interleaved_stereo);
    }

    /// Pre-gain -> noise suppression -> EQ -> de-esser -> gate/compressor.
    #[inline]
    fn process_pre_fader(&mut self, interleaved_stereo: &mut [f32]) {
        let pre = self.scalars.pre_gain();
        if (pre - 1.0).abs() > 1e-5 {
            for s in interleaved_stereo.iter_mut() {
                *s *= pre;
            }
        }

        self.extras.process_noise_suppression(interleaved_stereo);
        self.eq.process_interleaved(interleaved_stereo);
        self.extras.process_de_esser(interleaved_stereo);
        self.dynamics.process_interleaved(interleaved_stereo);
    }

    /// `[gate, compressor, de-esser, limiter]` gain reduction (dB) from the last block.
    #[inline]
    fn reductions(&self) -> [f32; 4] {
        let (gate, comp) = self.dynamics.reductions_db();
        let (de_esser, limiter) = self.extras.reductions_db();
        [gate, comp, de_esser, limiter]
    }

    #[inline]
    fn process_output_fader(&self, interleaved_stereo: &mut [f32]) {
        let post = self.scalars.post_gain();
        if (post - 1.0).abs() > 1e-5 {
            for s in interleaved_stereo.iter_mut() {
                *s *= post;
            }
        }
    }
}

/// Control handle for a single bus held by the Tauri command thread.
struct BusControlHandle {
    scalars: Arc<AtomicBusScalars>,
    eq_ctrl: EqBankControl,
    dynamics_params: Arc<AtomicDynamicsParams>,
    extras_params: Arc<AtomicExtrasParams>,
}

enum RenderThreadCommand {
    AddPidStream(PidRingConsumer),
    RemovePidStream(u32),
}

struct PidSlotOnRenderThread {
    consumer: PidRingConsumer,
    resampler: AdaptiveResampler,
}

pub struct EngineDiagnostics {
    pub callback_count: AtomicU64,
    pub underrun_count: AtomicU64,
    pub master_running: AtomicBool,
    pub mic_running: AtomicBool,
    /// Total time spent mixing (microseconds) and total time those blocks lasted.
    pub busy_us_total: AtomicU64,
    pub budget_us_total: AtomicU64,
    /// Heaviest block since the last report, as busy time per block time in permille.
    pub worst_load_permille: AtomicU32,
}

impl Default for EngineDiagnostics {
    fn default() -> Self {
        Self {
            callback_count: AtomicU64::new(0),
            underrun_count: AtomicU64::new(0),
            master_running: AtomicBool::new(false),
            mic_running: AtomicBool::new(false),
            busy_us_total: AtomicU64::new(0),
            budget_us_total: AtomicU64::new(0),
            worst_load_permille: AtomicU32::new(0),
        }
    }
}

impl EngineDiagnostics {
    /// Called once per mixed block on the audio thread (atomics only).
    pub fn record_block(&self, busy: std::time::Duration, frames: usize) {
        let busy_us = busy.as_micros() as u64;
        let budget_us = (frames as u64 * 1_000_000 / DEFAULT_SAMPLE_RATE as u64).max(1);
        self.busy_us_total.fetch_add(busy_us, Ordering::Relaxed);
        self.budget_us_total.fetch_add(budget_us, Ordering::Relaxed);
        let load = (busy_us * 1000 / budget_us).min(u32::MAX as u64) as u32;
        self.worst_load_permille.fetch_max(load, Ordering::Relaxed);
    }

    /// Average CPU load of the mixing thread in percent, and the worst block since the last call.
    pub fn take_load_report(&self) -> (f32, f32) {
        let budget = self.budget_us_total.load(Ordering::Relaxed).max(1);
        let average = self.busy_us_total.load(Ordering::Relaxed) as f32 * 100.0 / budget as f32;
        let worst = self.worst_load_permille.swap(0, Ordering::Relaxed) as f32 / 10.0;
        (average, worst)
    }
}

struct ManagedProcessEntry {
    /// Lower-cased executable name, used to match per-app volume settings.
    pub exe: String,
    pub bus: BusId,
    pub meta: Arc<PidStreamMeta>,
    pub worker: ProcessLoopbackWorker,
}

/// Thread-safe controller for the entire Multi-Bus Audio Router Engine.
pub struct AudioRouterEngine {
    pub config: Mutex<RouterConfig>,
    pub stream: Mutex<super::stream::StreamOutput>,
    pub mic_to_monitor_atomic: Arc<AtomicBool>,
    stream_mic_commands: Mutex<Producer<PidRingProducer>>,
    bus_controls: Mutex<BTreeMap<BusId, BusControlHandle>>,
    chat_mix_atomic: Arc<AtomicU32>,
    master_vol_atomic: Arc<AtomicU32>,
    pub meters: Arc<SharedMeterBank>,
    pub diagnostics: Arc<EngineDiagnostics>,
    duck_params: Arc<AtomicDuckParams>,
    render_cmd_tx: Mutex<Producer<RenderThreadCommand>>,
    managed_pids: Mutex<BTreeMap<u32, ManagedProcessEntry>>,
    scan_lock: Mutex<()>,
    master_stop: Arc<AtomicBool>,
    master_thread: Mutex<Option<JoinHandle<()>>>,
    mic_stop: Arc<AtomicBool>,
    mic_thread: Mutex<Option<JoinHandle<()>>>,
}

impl AudioRouterEngine {
    pub fn start(mut initial_config: RouterConfig) -> Arc<Self> {
        initial_config = initial_config.normalized();

        // Auto-detect silent dummy sink if not yet configured
        if initial_config.dummy_sink_device_id.is_none() {
            if let Some(dummy) = find_silent_dummy_sink_endpoint() {
                initial_config.dummy_sink_device_id = Some(dummy.id);
            }
        }

        let meters = Arc::new(SharedMeterBank::new());
        let diagnostics = Arc::new(EngineDiagnostics::default());
        let chat_mix_atomic = Arc::new(AtomicU32::new(initial_config.chat_mix_balance.to_bits()));
        let master_vol_atomic = Arc::new(AtomicU32::new(initial_config.master_volume.to_bits()));
        let mic_to_monitor_atomic = Arc::new(AtomicBool::new(initial_config.mic_to_monitor));
        let (monitor_mic_tx, monitor_mic_rx) = create_pid_ring_stream(0, BusId::Mic);

        let mut bus_controls = BTreeMap::new();
        let mut playback_strips: Vec<(BusId, RealTimeBusStrip)> = Vec::new();
        let mut mic_strip_opt: Option<RealTimeBusStrip> = None;

        for bus in BusId::ALL {
            let bus_cfg = initial_config
                .buses
                .get(&bus)
                .cloned()
                .unwrap_or_else(|| BusConfig::default_for_bus(bus));

            let scalars = AtomicBusScalars::new(&bus_cfg);
            let (eq, eq_ctrl) = EqBank::new(&bus_cfg.eq_bands, DEFAULT_SAMPLE_RATE);
            let (dynamics, dynamics_params) =
                DynamicsProcessor::new(&bus_cfg.noise_gate, &bus_cfg.compressor);
            let (extras, extras_params) = BusExtras::new(
                &bus_cfg.limiter,
                &bus_cfg.de_esser,
                &bus_cfg.noise_suppression,
            );

            let strip = RealTimeBusStrip {
                scalars: Arc::clone(&scalars),
                eq,
                dynamics,
                extras,
            };

            bus_controls.insert(
                bus,
                BusControlHandle {
                    scalars,
                    eq_ctrl,
                    dynamics_params,
                    extras_params,
                },
            );

            if bus == BusId::Mic {
                mic_strip_opt = Some(strip);
            } else {
                playback_strips.push((bus, strip));
            }
        }

        let (render_cmd_tx, render_cmd_rx) = RingBuffer::new(64);
        let master_stop = Arc::new(AtomicBool::new(false));
        let mic_stop = Arc::new(AtomicBool::new(false));

        let [(_, game_strip), (_, chat_strip), (_, media_strip), (_, aux_strip)]: [(
            BusId,
            RealTimeBusStrip,
        ); 4] = playback_strips
            .try_into()
            .unwrap_or_else(|_| unreachable!("Always 4 playback buses"));

        let (stream_mic_tx, stream_mic_rx) = create_pid_ring_stream(0, BusId::Mic);
        let (stream_mic_commands, stream_mic_commands_rx) = RingBuffer::new(64);
        let stream = super::stream::StreamOutput::start(&initial_config.stream, stream_mic_rx, Arc::clone(&meters));
        let stream_mic_post_fader = stream.mic_post_fader_flag();

        let master_device_id = initial_config.master_output_device_id.clone();
        let master_stop_clone = Arc::clone(&master_stop);
        let meters_clone = Arc::clone(&meters);
        let diag_clone = Arc::clone(&diagnostics);
        let chat_mix_clone = Arc::clone(&chat_mix_atomic);
        let master_vol_clone = Arc::clone(&master_vol_atomic);
        let mic_to_mon_clone = Arc::clone(&mic_to_monitor_atomic);
        let duck_params = AtomicDuckParams::new(&initial_config.ducking);
        let duck_clone = Arc::clone(&duck_params);

        let master_handle = thread::Builder::new()
            .name("sonar-master-wasapi-render".to_string())
            .spawn(move || {
                run_master_render_thread(
                    master_device_id,
                    [game_strip, chat_strip, media_strip, aux_strip],
                    render_cmd_rx,
                    monitor_mic_rx,
                    chat_mix_clone,
                    master_vol_clone,
                    mic_to_mon_clone,
                    duck_clone,
                    meters_clone,
                    diag_clone,
                    master_stop_clone,
                );
            })
            .ok();

        let mic_handle = if let Some(mic_strip) = mic_strip_opt {
            let phys_mic_id = initial_config.physical_mic_device_id.clone();
            let virt_mic_id = initial_config.virtual_mic_output_device_id.clone();
            let mic_stop_clone = Arc::clone(&mic_stop);
            let meters_mic = Arc::clone(&meters);
            let diag_mic = Arc::clone(&diagnostics);

            thread::Builder::new()
                .name("sonar-mic-wasapi-pipeline".to_string())
                .spawn(move || {
                    run_mic_pipeline_thread(
                        phys_mic_id,
                        virt_mic_id,
                        mic_strip,
                        meters_mic,
                        diag_mic,
                        mic_stop_clone,
                        stream_mic_tx,
                        stream_mic_commands_rx,
                        monitor_mic_tx,
                        stream_mic_post_fader,
                    );
                })
                .ok()
        } else {
            None
        };

        let engine = Arc::new(Self {
            config: Mutex::new(initial_config),
            stream: Mutex::new(stream),
            stream_mic_commands: Mutex::new(stream_mic_commands),
            bus_controls: Mutex::new(bus_controls),
            chat_mix_atomic,
            master_vol_atomic,
            mic_to_monitor_atomic,
            meters,
            diagnostics,
            duck_params,
            render_cmd_tx: Mutex::new(render_cmd_tx),
            managed_pids: Mutex::new(BTreeMap::new()),
            scan_lock: Mutex::new(()),
            master_stop,
            master_thread: Mutex::new(master_handle),
            mic_stop,
            mic_thread: Mutex::new(mic_handle),
        });
        // Discovery remains alive with the WebView closed. No strong reference is retained
        // while sleeping, so this controller cannot keep an obsolete engine alive.
        let weak = Arc::downgrade(&engine);
        let stop = Arc::clone(&engine.master_stop);
        let _ = thread::Builder::new().name("stellars-source-discovery".into()).spawn(move || {
            while !stop.load(Ordering::Acquire) {
                if let Some(engine) = weak.upgrade() {
                    let _ = engine.scan_and_sync_processes();
                } else { break; }
                for _ in 0..8 {
                    if stop.load(Ordering::Acquire) { break; }
                    thread::sleep(std::time::Duration::from_millis(250));
                }
            }
        });
        engine
    }

    pub fn set_chat_mix(&self, balance: f32) {
        let clamped = balance.clamp(-1.0, 1.0);
        self.chat_mix_atomic
            .store(clamped.to_bits(), Ordering::Relaxed);
        if let Ok(mut cfg) = self.config.lock() {
            cfg.chat_mix_balance = clamped;
        }
    }

    /// Replace only the recording endpoint. Headphone playback and PID routing keep running.
    pub fn update_stream_output(&self, stream_cfg: &crate::state::config::StreamConfig) -> Result<(), String> {
        let previous = self.config.lock().map_err(|_| "Config lock poisoned")?.stream.clone();
        let mut output = self.stream.lock().map_err(|_| "Stream lock poisoned")?;
        let failed = output.error.lock().map_err(|_| "Stream status lock poisoned")?.is_some();
        if previous.output_device_id != stream_cfg.output_device_id || failed {
            let (producer, consumer) = create_pid_ring_stream(0, BusId::Mic);
            self.stream_mic_commands.lock().map_err(|_| "Mic tap lock poisoned")?
                .push(producer).map_err(|_| "Mic tap update queue is full. Try again shortly.")?;
            output.shutdown();
            *output = super::stream::StreamOutput::start(stream_cfg, consumer, Arc::clone(&self.meters));
        } else {
            output.update(stream_cfg);
        }
        Ok(())
    }

    pub fn set_mic_to_monitor(&self, enabled: bool) {
        self.meters.set_monitor_mic_gain(1.0);
        self.mic_to_monitor_atomic.store(enabled, Ordering::Relaxed);
        if let Ok(mut cfg) = self.config.lock() {
            cfg.mic_to_monitor = enabled;
        }
    }

    pub fn set_master_volume(&self, volume: f32) {
        let clamped = volume.clamp(0.0, 2.0);
        self.master_vol_atomic
            .store(clamped.to_bits(), Ordering::Relaxed);
        if let Ok(mut cfg) = self.config.lock() {
            cfg.master_volume = clamped;
        }
    }

    pub fn update_bus_volume_mute(&self, bus: BusId, volume: f32, muted: bool, pre_gain_db: f32) {
        if let Ok(mut cfg) = self.config.lock() {
            if let Some(b) = cfg.buses.get_mut(&bus) {
                b.volume = volume.clamp(0.0, 2.0);
                b.muted = muted;
                b.pre_gain_db = pre_gain_db.clamp(-24.0, 24.0);
                if let Ok(ctrls) = self.bus_controls.lock() {
                    if let Some(ctrl) = ctrls.get(&bus) {
                        ctrl.scalars.update_from_config(b);
                    }
                }
            }
        }
    }

    /// Temporary monitor level for stream playback: changes what the headphones hear
    /// without touching the saved config, so the real faders come back untouched.
    pub fn audition_bus(&self, bus: BusId, volume: f32, muted: bool) {
        let volume = if volume.is_finite() { volume.clamp(0.0, 2.0) } else { 0.0 };
        if bus == BusId::Mic {
            self.meters.set_monitor_mic_gain(if muted { 0.0 } else { volume });
            return;
        }
        let Ok(cfg) = self.config.lock() else { return };
        let Some(bus_cfg) = cfg.buses.get(&bus) else { return };
        let mut audition = bus_cfg.clone();
        audition.volume = volume;
        audition.muted = muted;
        if let Ok(ctrls) = self.bus_controls.lock() {
            if let Some(ctrl) = ctrls.get(&bus) {
                ctrl.scalars.update_from_config(&audition);
            }
        }
    }

    /// State of the DeepFilterNet3 engine on each channel that asks for it.
    pub fn deep_filter_states(&self) -> Vec<(BusId, &'static str)> {
        let Ok(ctrls) = self.bus_controls.lock() else { return Vec::new() };
        ctrls
            .iter()
            .filter_map(|(bus, ctrl)| {
                let shared = &ctrl.extras_params.df_shared;
                if !shared.active.load(Ordering::Relaxed) {
                    None
                } else if shared.is_ready() {
                    Some((*bus, "ready"))
                } else if shared.is_failed() {
                    Some((*bus, "failed"))
                } else {
                    Some((*bus, "loading"))
                }
            })
            .collect()
    }

    pub fn update_eq_band(&self, bus: BusId, band_index: usize, band_cfg: EqBandConfig) {
        let clamped = band_cfg.clamped();
        if let Ok(mut cfg) = self.config.lock() {
            if let Some(b) = cfg.buses.get_mut(&bus) {
                if band_index < b.eq_bands.len() {
                    b.eq_bands[band_index] = clamped;
                }
            }
        }
        if let Ok(mut ctrls) = self.bus_controls.lock() {
            if let Some(ctrl) = ctrls.get_mut(&bus) {
                ctrl.eq_ctrl
                    .update_band(band_index, &clamped, DEFAULT_SAMPLE_RATE);
            }
        }
    }

    pub fn update_bus_dynamics(
        &self,
        bus: BusId,
        gate: NoiseGateConfig,
        compressor: CompressorConfig,
    ) {
        if let Ok(mut cfg) = self.config.lock() {
            if let Some(b) = cfg.buses.get_mut(&bus) {
                b.noise_gate = gate;
                b.compressor = compressor;
            }
        }
        if let Ok(ctrls) = self.bus_controls.lock() {
            if let Some(ctrl) = ctrls.get(&bus) {
                ctrl.dynamics_params.update_gate(&gate, DEFAULT_SAMPLE_RATE);
                ctrl.dynamics_params
                    .update_compressor(&compressor, DEFAULT_SAMPLE_RATE);
            }
        }
    }

    pub fn update_bus_extras(
        &self,
        bus: BusId,
        limiter: LimiterConfig,
        de_esser: DeEsserConfig,
        noise: NoiseSuppressionConfig,
    ) {
        let (limiter, de_esser, noise) = (limiter.normalized(), de_esser.normalized(), noise.normalized());
        if let Ok(mut cfg) = self.config.lock() {
            if let Some(b) = cfg.buses.get_mut(&bus) {
                b.limiter = limiter;
                b.de_esser = de_esser;
                b.noise_suppression = noise;
            }
        }
        if let Ok(ctrls) = self.bus_controls.lock() {
            if let Some(ctrl) = ctrls.get(&bus) {
                ctrl.extras_params.update_limiter(&limiter);
                ctrl.extras_params.update_de_esser(&de_esser);
                ctrl.extras_params.update_noise_suppression(&noise);
            }
        }
    }

    pub fn update_ducking(&self, ducking: DuckingConfig) {
        let ducking = ducking.normalized();
        self.duck_params.update(&ducking);
        if let Ok(mut cfg) = self.config.lock() {
            cfg.ducking = ducking;
        }
    }

    /// Sets the volume (0.0–2.0) of every running instance of `exe_name` and remembers it.
    pub fn set_process_volume(&self, exe_name: &str, volume: f32) {
        let key = exe_name.trim().to_ascii_lowercase();
        if key.is_empty() {
            return;
        }
        let volume = if volume.is_finite() { volume.clamp(0.0, 2.0) } else { 1.0 };
        if let Ok(mut cfg) = self.config.lock() {
            if (volume - 1.0).abs() < 0.005 {
                cfg.process_volumes.remove(&key);
            } else {
                cfg.process_volumes.insert(key.clone(), volume);
            }
        }
        if let Ok(managed) = self.managed_pids.lock() {
            for entry in managed.values().filter(|e| e.exe == key) {
                entry.meta.set_gain(volume);
            }
        }
    }

    /// Re-reads `process_volumes` from the config and applies it to every running stream.
    fn refresh_process_gains(&self) {
        let volumes = match self.config.lock() {
            Ok(cfg) => cfg.process_volumes.clone(),
            Err(_) => return,
        };
        if let Ok(managed) = self.managed_pids.lock() {
            for entry in managed.values() {
                entry.meta.set_gain(volumes.get(&entry.exe).copied().unwrap_or(1.0));
            }
        }
    }

    /// Applies the mixing parts of `target` (buses, EQ, dynamics, extras, stream mix, ducking,
    /// routing rules and per-app volumes) to the running engine without restarting it. Device
    /// selections in `target` are ignored.
    pub fn apply_scene_live(&self, target: &RouterConfig) -> Result<(), String> {
        for bus in BusId::ALL {
            if let Some(b) = target.buses.get(&bus) {
                self.update_bus_volume_mute(bus, b.volume, b.muted, b.pre_gain_db);
                for (index, band) in b.eq_bands.iter().enumerate() {
                    self.update_eq_band(bus, index, *band);
                }
                self.update_bus_dynamics(bus, b.noise_gate, b.compressor);
                self.update_bus_extras(bus, b.limiter, b.de_esser, b.noise_suppression);
            }
        }
        self.set_chat_mix(target.chat_mix_balance);
        self.set_master_volume(target.master_volume);
        self.set_mic_to_monitor(target.mic_to_monitor);
        self.update_ducking(target.ducking.clone());
        if let Ok(mut cfg) = self.config.lock() {
            cfg.process_routing_rules = target.process_routing_rules.clone();
            cfg.process_volumes = target.process_volumes.clone();
        }
        self.refresh_process_gains();
        self.update_stream_output(&target.stream)?;
        if let Ok(mut cfg) = self.config.lock() {
            cfg.stream = target.stream.clone();
        }
        let _ = self.scan_and_sync_processes();
        Ok(())
    }

    /// Assigns a process PID to a target playback bus (`Game`, `Chat`, `Media`, `Aux`):
    /// 1. Redirects the process to the silent WHQL dummy sink via `IAudioPolicyConfig` (if configured).
    /// 2. Starts a per-process WASAPI loopback capture stream (`VAD\Process_Loopback`) into the bus.
    pub fn assign_process_to_bus(
        &self,
        pid: u32,
        exe_name: Option<String>,
        bus: BusId,
    ) -> Result<(), String> {
        if self.master_stop.load(Ordering::Acquire) {
            return Err("Audio engine is stopped".into());
        }
        if bus == BusId::Mic {
            return Err(
                "Process streams can only be assigned to playback buses (Game, Chat, Media, Aux)"
                    .to_string(),
            );
        }

        let exe_key = exe_name
            .clone()
            .or_else(|| query_process_image_info(pid).map(|(name, _)| name))
            .map(|name| name.to_ascii_lowercase())
            .unwrap_or_default();

        let (dummy_sink_id, initial_gain) = {
            let mut cfg = self.config.lock().map_err(|_| "Config lock poisoned")?;
            if let Some(ref name) = exe_name {
                cfg.process_routing_rules
                    .insert(name.to_ascii_lowercase(), bus);
            } else if pid == 0 {
                cfg.process_routing_rules
                    .insert("systemsounds".to_string(), bus);
            }
            (
                cfg.dummy_sink_device_id.clone(),
                cfg.process_volumes.get(&exe_key).copied().unwrap_or(1.0),
            )
        };

        if pid == 0 {
            // Windows System Sounds is an internal OS audio session (PID 0) managed by AudioSrv.
            // Persist the routing rule and assign it logically to the target bus without attaching
            // a per-process WASAPI loopback worker (since PID 0 is not a Win32 process tree).
            return Ok(());
        }

        // Fast path: if PID is already captured, just switch its bus atomically!
        let mut managed = self
            .managed_pids
            .lock()
            .map_err(|_| "Process routing lock poisoned")?;
        if let Some(entry) = managed.get_mut(&pid) {
            entry.bus = bus;
            entry.meta.set_bus(bus);
            return Ok(());
        }

        if managed.len() >= MAX_MANAGED_PIDS {
            return Err(format!(
                "Maximum managed process capacity ({MAX_MANAGED_PIDS}) reached"
            ));
        }

        // Route PID to silent dummy sink if available so raw dry audio is muted
        if let Some(ref sink_id) = dummy_sink_id {
            if !sink_id.trim().is_empty() {
                let _ = route_pid_to_dummy_sink(pid, sink_id);
            }
        }

        let (producer, consumer) = create_pid_ring_stream(pid, bus);
        let meta = Arc::clone(&producer.meta);
        meta.set_gain(initial_gain);
        let worker = ProcessLoopbackWorker::spawn(pid, producer);

        if let Ok(mut cmd_tx) = self.render_cmd_tx.lock() {
            let _ = cmd_tx.push(RenderThreadCommand::AddPidStream(consumer));
        }

        managed.insert(
            pid,
            ManagedProcessEntry {
                exe: exe_key,
                bus,
                meta,
                worker,
            },
        );

        Ok(())
    }

    /// Stops capture and resets endpoint for a dead/exited process PID.
    fn release_pid_stream(&self, pid: u32) {
        if let Ok(mut managed) = self.managed_pids.lock() {
            if let Some(mut entry) = managed.remove(&pid) {
                entry.worker.stop();
            }
        }
        if let Ok(mut cmd_tx) = self.render_cmd_tx.lock() {
            let _ = cmd_tx.push(RenderThreadCommand::RemovePidStream(pid));
        }
        let _ = reset_pid_to_default_endpoint(pid);
    }

    /// Resets a process PID's custom routing rule and routes it to the default `Aux` bus
    /// (or releases it if the process has exited).
    pub fn unroute_process(&self, pid: u32) -> Result<(), String> {
        if pid == 0 {
            if let Ok(mut cfg) = self.config.lock() {
                cfg.process_routing_rules.remove("systemsounds");
                cfg.process_routing_rules.remove("system sounds");
            }
            return self.assign_process_to_bus(0, Some("SystemSounds".to_string()), BusId::Aux);
        }
        if let Some((exe_name, _)) = query_process_image_info(pid) {
            if let Ok(mut cfg) = self.config.lock() {
                cfg.process_routing_rules
                    .remove(&exe_name.to_ascii_lowercase());
                cfg.process_routing_rules.remove(&exe_name);
            }
            return self.assign_process_to_bus(pid, None, BusId::Aux);
        }
        self.release_pid_stream(pid);
        Ok(())
    }

    /// Lists all active audio processes, routes any process matching `processRoutingRules`,
    /// and automatically routes all remaining unrouted apps/listeners to `BusId::Aux`.
    pub fn scan_and_sync_processes(&self) -> Result<Vec<ActiveAudioProcess>, String> {
        let _scan = self.scan_lock.lock().map_err(|_| "Process scan lock poisoned")?;
        if self.master_stop.load(Ordering::Acquire) {
            return Ok(Vec::new());
        }
        let mut processes = list_active_audio_processes()?;
        let rules = {
            let cfg = self.config.lock().map_err(|_| "Config lock poisoned")?;
            cfg.process_routing_rules.clone()
        };

        // Prune dead PIDs that are no longer running
        let active_pids: std::collections::BTreeSet<u32> =
            processes.iter().map(|p| p.pid).collect();
        let stale_pids: Vec<u32> = if let Ok(managed) = self.managed_pids.lock() {
            managed
                .keys()
                .copied()
                .filter(|pid| *pid != 0 && !active_pids.contains(pid))
                .collect()
        } else {
            Vec::new()
        };
        for stale in stale_pids {
            self.release_pid_stream(stale);
        }

        // Pass 1: Route processes matching explicit saved rules first so they get priority slots
        for proc_info in &mut processes {
            let lower_exe = proc_info.exe_name.to_ascii_lowercase();
            if let Some(&rule_bus) = rules.get(&lower_exe) {
                if self
                    .assign_process_to_bus(proc_info.pid, Some(proc_info.exe_name.clone()), rule_bus)
                    .is_ok()
                {
                    proc_info.assigned_bus = Some(rule_bus.as_str().to_string());
                    proc_info.is_routed = true;
                }
            }
        }

        // Pass 2: Auto-route all remaining unrouted apps/listeners to the Aux category
        for proc_info in &mut processes {
            if proc_info.is_routed {
                continue;
            }
            let already_bus = self
                .managed_pids
                .lock()
                .ok()
                .and_then(|m| m.get(&proc_info.pid).map(|e| e.bus));

            if let Some(bus) = already_bus {
                proc_info.assigned_bus = Some(bus.as_str().to_string());
                proc_info.is_routed = true;
            } else if self
                .assign_process_to_bus(proc_info.pid, Some(proc_info.exe_name.clone()), BusId::Aux)
                .is_ok()
            {
                proc_info.assigned_bus = Some(BusId::Aux.as_str().to_string());
                proc_info.is_routed = true;
            }
        }

        if let (Ok(cfg), Ok(mut stream)) = (self.config.lock(), self.stream.lock()) {
            stream.sync(&cfg.stream, &processes);
        }
        Ok(processes)
    }

    pub fn meters_snapshot(&self) -> AllBusMetersPayload {
        let mut snapshot = self.meters.snapshot();
        snapshot.stream_error = self
            .stream
            .lock()
            .ok()
            .and_then(|output| output.error.lock().ok().and_then(|error| error.clone()));
        snapshot
    }

    pub fn shutdown(&self) {
        if let Ok(mut stream) = self.stream.lock() {
            stream.shutdown();
        }
        self.master_stop.store(true, Ordering::Release);
        self.mic_stop.store(true, Ordering::Release);

        if let Ok(mut managed) = self.managed_pids.lock() {
            for (&pid, entry) in managed.iter_mut() {
                entry.worker.stop();
                let _ = reset_pid_to_default_endpoint(pid);
            }
            managed.clear();
        }

        if let Ok(mut handle_guard) = self.master_thread.lock() {
            if let Some(h) = handle_guard.take() {
                let _ = h.join();
            }
        }
        if let Ok(mut handle_guard) = self.mic_thread.lock() {
            if let Some(h) = handle_guard.take() {
                let _ = h.join();
            }
        }
    }
}

impl Drop for AudioRouterEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Master Clock WASAPI Render Thread:
/// Driven by a 10ms event-driven WASAPI Shared render client on the physical headphones
/// with MMCSS "Pro Audio" thread priority.
/// Zero heap allocations and zero mutex locks inside the render loop.
#[allow(clippy::too_many_arguments)]
fn run_master_render_thread(
    master_device_id: Option<String>,
    mut bus_strips: [RealTimeBusStrip; 4],
    mut cmd_rx: Consumer<RenderThreadCommand>,
    mut monitor_mic_rx: PidRingConsumer,
    chat_mix_atomic: Arc<AtomicU32>,
    master_vol_atomic: Arc<AtomicU32>,
    mic_to_monitor_atomic: Arc<AtomicBool>,
    duck_params: Arc<AtomicDuckParams>,
    meters: Arc<SharedMeterBank>,
    diagnostics: Arc<EngineDiagnostics>,
    stop_flag: Arc<AtomicBool>,
) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mut mmcss_task_idx = 0u32;
        let mmcss_handle =
            AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut mmcss_task_idx).ok();

        // Pre-allocate fixed-size PID stream array and bus mixing buffers once before entering RT loop
        let mut pid_slots: [Option<PidSlotOnRenderThread>; MAX_MANAGED_PIDS] =
            std::array::from_fn(|_| None);
        let mut bus_buffers = [[0.0_f32; MAX_BLOCK_SAMPLES]; 4];
        let mut master_scratch = [0.0_f32; MAX_BLOCK_SAMPLES];
        let mut monitor_mic_scratch = [0.0_f32; MAX_BLOCK_SAMPLES];
        let mut pid_scratch = [0.0_f32; MAX_BLOCK_SAMPLES];
        let mut monitor_mic_resampler = AdaptiveResampler::new();
        let mut duck = DuckState::new();
        // Indices 0..4 = Game/Chat/Media/Aux buses, 4 = master.
        let mut lufs_meters: [LufsMeter; 5] = std::array::from_fn(|_| LufsMeter::new());

        let _ = master_wasapi_loop(
            master_device_id.as_deref(),
            &mut bus_strips,
            &mut cmd_rx,
            &mut monitor_mic_rx,
            &mut monitor_mic_scratch,
            &mut monitor_mic_resampler,
            &mut pid_slots,
            &mut bus_buffers,
            &mut master_scratch,
            &mut pid_scratch,
            &chat_mix_atomic,
            &master_vol_atomic,
            &mic_to_monitor_atomic,
            &duck_params,
            &mut duck,
            &mut lufs_meters,
            &meters,
            &diagnostics,
            &stop_flag,
        );

        diagnostics.master_running.store(false, Ordering::Release);
        if let Some(h) = mmcss_handle {
            let _ = AvRevertMmThreadCharacteristics(h);
        }
        CoUninitialize();
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn master_wasapi_loop(
    master_device_id: Option<&str>,
    bus_strips: &mut [RealTimeBusStrip; 4],
    cmd_rx: &mut Consumer<RenderThreadCommand>,
    monitor_mic_rx: &mut PidRingConsumer,
    monitor_mic_scratch: &mut [f32; MAX_BLOCK_SAMPLES],
    monitor_mic_resampler: &mut AdaptiveResampler,
    pid_slots: &mut [Option<PidSlotOnRenderThread>; MAX_MANAGED_PIDS],
    bus_buffers: &mut [[f32; MAX_BLOCK_SAMPLES]; 4],
    master_scratch: &mut [f32; MAX_BLOCK_SAMPLES],
    pid_scratch: &mut [f32; MAX_BLOCK_SAMPLES],
    chat_mix_atomic: &AtomicU32,
    master_vol_atomic: &AtomicU32,
    mic_to_monitor_atomic: &AtomicBool,
    duck_params: &AtomicDuckParams,
    duck: &mut DuckState,
    lufs_meters: &mut [LufsMeter; 5],
    meters: &SharedMeterBank,
    diagnostics: &EngineDiagnostics,
    stop_flag: &AtomicBool,
) -> Result<(), String> {
    let device = resolve_mmdevice(master_device_id, eRender)?;
    let audio_client: IAudioClient = device
        .Activate(CLSCTX_ALL, None)
        .map_err(|e| format!("IMMDevice::Activate(IAudioClient) failed: {e}"))?;

    let wave_format = standard_float32_stereo_format();
    let stream_flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK
        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;

    // 10 ms period (100,000 in 100-ns units)
    let hns_period: i64 = 100_000;
    audio_client
        .Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            stream_flags,
            hns_period,
            0,
            &wave_format,
            None,
        )
        .map_err(|e| format!("Master IAudioClient::Initialize failed: {e}"))?;

    let render_event = CreateEventW(None, false, false, None)
        .map_err(|e| format!("CreateEventW(render_event) failed: {e}"))?;

    if let Err(e) = audio_client.SetEventHandle(render_event) {
        let _ = CloseHandle(render_event);
        return Err(format!("Master SetEventHandle failed: {e}"));
    }

    let buffer_frame_count = audio_client
        .GetBufferSize()
        .map_err(|e| format!("GetBufferSize failed: {e}"))?;

    let render_client: IAudioRenderClient = match audio_client.GetService() {
        Ok(rc) => rc,
        Err(e) => {
            let _ = CloseHandle(render_event);
            return Err(format!("GetService(IAudioRenderClient) failed: {e}"));
        }
    };

    // Prime initial silence block before starting
    if let Ok(ptr) = render_client.GetBuffer(buffer_frame_count) {
        std::ptr::write_bytes(ptr, 0, (buffer_frame_count as usize) * 8);
        let _ = render_client.ReleaseBuffer(buffer_frame_count, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32);
    }

    if let Err(e) = audio_client.Start() {
        let _ = CloseHandle(render_event);
        return Err(format!("Master IAudioClient::Start failed: {e}"));
    }

    diagnostics.master_running.store(true, Ordering::Release);

    // REAL-TIME RENDER CALLBACK LOOP (Zero heap allocations, zero locks)
    while !stop_flag.load(Ordering::Relaxed) {
        let wait_res = WaitForSingleObject(render_event, 50);
        if wait_res != WAIT_OBJECT_0 {
            continue;
        }

        // 1. Drain lock-free control commands
        while let Ok(cmd) = cmd_rx.pop() {
            match cmd {
                RenderThreadCommand::AddPidStream(consumer) => {
                    for slot in pid_slots.iter_mut() {
                        if slot.is_none() {
                            *slot = Some(PidSlotOnRenderThread {
                                consumer,
                                resampler: AdaptiveResampler::new(),
                            });
                            break;
                        }
                    }
                }
                RenderThreadCommand::RemovePidStream(pid) => {
                    for slot in pid_slots.iter_mut() {
                        if let Some(ref s) = slot {
                            if s.consumer.meta.pid == pid {
                                *slot = None;
                            }
                        }
                    }
                }
            }
        }

        let padding_frames = match audio_client.GetCurrentPadding() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let avail_frames = buffer_frame_count
            .saturating_sub(padding_frames)
            .min(MAX_BLOCK_FRAMES as u32) as usize;
        if avail_frames == 0 {
            continue;
        }
        // The device ran completely dry before we refilled it: the listener heard a gap.
        if padding_frames == 0 && diagnostics.callback_count.load(Ordering::Relaxed) > 4 {
            diagnostics.underrun_count.fetch_add(1, Ordering::Relaxed);
        }
        let block_started = std::time::Instant::now();

        let sample_len = avail_frames * 2;

        // Drain monitor mic tap
        monitor_mic_scratch[..sample_len].fill(0.0);
        monitor_mic_resampler.process_and_accumulate(
            monitor_mic_rx,
            &mut monitor_mic_scratch[..sample_len],
        );
        let mic_to_mon = mic_to_monitor_atomic.load(Ordering::Relaxed);
        let mic_monitor_gain = meters.monitor_mic_gain();

        // 2. Zero the 4 bus buffers for this callback slice
        for bus_buf in bus_buffers.iter_mut() {
            bus_buf[..sample_len].fill(0.0);
        }

        // 3. Pull & resample each active PID stream into its assigned bus buffer
        for slot_opt in pid_slots.iter_mut() {
            let mut should_drop = false;
            if let Some(ref mut slot) = slot_opt {
                if !slot.consumer.meta.is_active() {
                    should_drop = true;
                } else {
                    let bus_idx = slot.consumer.meta.bus().index().min(3);
                    let app_gain = slot.consumer.meta.gain();
                    if (app_gain - 1.0).abs() < 1e-4 {
                        slot.resampler.process_and_accumulate(
                            &mut slot.consumer,
                            &mut bus_buffers[bus_idx][..sample_len],
                        );
                    } else {
                        // Per-app volume: resample into scratch, then mix in scaled.
                        pid_scratch[..sample_len].fill(0.0);
                        slot.resampler.process_and_accumulate(
                            &mut slot.consumer,
                            &mut pid_scratch[..sample_len],
                        );
                        for (dst, src) in bus_buffers[bus_idx][..sample_len]
                            .iter_mut()
                            .zip(&pid_scratch[..sample_len])
                        {
                            *dst += app_gain * *src;
                        }
                    }
                }
            }
            if should_drop {
                *slot_opt = None;
            }
        }

        // 4. Run per-bus DSP chains (Pre-Gain -> 10-Band PEQ -> Gate/Comp -> Post-Gain/Mute) & meters
        for bus_idx in 0..4 {
            let slice = &mut bus_buffers[bus_idx][..sample_len];
            bus_strips[bus_idx].process_buffer(slice);
            meters.buses[bus_idx].set_reductions(bus_strips[bus_idx].reductions());
            meters.buses[bus_idx].update_from_interleaved(slice);
            meters.buses[bus_idx].set_lufs(lufs_meters[bus_idx].process(slice));
        }

        // 4b. Auto-ducking: the loudest trigger bus decides the gain applied to the target buses.
        let trigger_mask = duck_params.trigger_mask();
        let mut trigger_level = 0.0_f32;
        for bus_idx in 0..4 {
            if trigger_mask & (1 << bus_idx) != 0 {
                trigger_level = trigger_level.max(block_rms(&bus_buffers[bus_idx][..sample_len]));
            }
        }
        if trigger_mask & (1 << BusId::Mic.index()) != 0 {
            let mic = meters.buses[BusId::Mic.index()].read();
            trigger_level = trigger_level.max(0.5 * (mic.rms_left + mic.rms_right));
        }
        let (duck_from, duck_to) = duck.advance(duck_params, trigger_level, avail_frames);
        meters.set_duck_gain(duck_to);
        let duck_targets = duck_params.target_mask();

        // 5. ChatMix Crossfader & Master Summing (Section 3.4)
        let chat_mix = f32::from_bits(chat_mix_atomic.load(Ordering::Relaxed));
        let master_gain = f32::from_bits(master_vol_atomic.load(Ordering::Relaxed));
        let (g_game, g_chat) = compute_chat_mix_gains(chat_mix);

        let inv_frames = 1.0 / avail_frames as f32;
        for frame in 0..avail_frames {
            // Linear interpolation of the duck gain across the block avoids zipper noise.
            let duck_gain = duck_from + (duck_to - duck_from) * ((frame + 1) as f32 * inv_frames);
            let bus_gain = |bus_idx: usize, base: f32| {
                if duck_targets & (1 << bus_idx) != 0 { base * duck_gain } else { base }
            };
            let (gg, gc, gm, ga) = (bus_gain(0, g_game), bus_gain(1, g_chat), bus_gain(2, 1.0), bus_gain(3, 1.0));
            for ch in 0..2 {
                let i = frame * 2 + ch;
                let mixed = gg * bus_buffers[0][i]
                    + gc * bus_buffers[1][i]
                    + gm * bus_buffers[2][i]
                    + ga * bus_buffers[3][i]
                    + (if mic_to_mon { mic_monitor_gain * monitor_mic_scratch[i] } else { 0.0 });
                master_scratch[i] = soft_limit_sample(master_gain * mixed);
            }
        }

        meters.master.update_from_interleaved(&master_scratch[..sample_len]);
        meters.master.set_lufs(lufs_meters[4].process(&master_scratch[..sample_len]));

        // 6. Write to physical WASAPI render endpoint buffer
        if let Ok(out_ptr) = render_client.GetBuffer(avail_frames as u32) {
            let out_slice = std::slice::from_raw_parts_mut(out_ptr as *mut f32, sample_len);
            out_slice.copy_from_slice(&master_scratch[..sample_len]);
            let _ = render_client.ReleaseBuffer(avail_frames as u32, 0);
            diagnostics.record_block(block_started.elapsed(), avail_frames);
            diagnostics.callback_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    let _ = audio_client.Stop();
    let _ = CloseHandle(render_event);
    Ok(())
}

/// Physical Mic Capture -> [MIC BUS: Pre-Gain -> Gate -> 10-Band PEQ -> Comp -> Post-Gain/Mute] -> Signed Virtual Mic Render
fn run_mic_pipeline_thread(
    physical_mic_id: Option<String>,
    virtual_mic_render_id: Option<String>,
    mut mic_strip: RealTimeBusStrip,
    meters: Arc<SharedMeterBank>,
    diagnostics: Arc<EngineDiagnostics>,
    stop_flag: Arc<AtomicBool>,
    mut stream_mic_tx: PidRingProducer,
    mut stream_mic_commands: Consumer<PidRingProducer>,
    mut monitor_mic_tx: PidRingProducer,
    stream_mic_post_fader: Arc<AtomicBool>,
) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mut mmcss_task_idx = 0u32;
        let mmcss_handle =
            AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut mmcss_task_idx).ok();

        let _ = mic_wasapi_loop(
            physical_mic_id.as_deref(),
            virtual_mic_render_id.as_deref(),
            &mut mic_strip,
            &meters,
            &diagnostics,
            &stop_flag,
            &mut stream_mic_tx,
            &mut stream_mic_commands,
            &mut monitor_mic_tx,
            &stream_mic_post_fader,
        );

        diagnostics.mic_running.store(false, Ordering::Release);
        if let Some(h) = mmcss_handle {
            let _ = AvRevertMmThreadCharacteristics(h);
        }
        CoUninitialize();
    }
}

unsafe fn mic_wasapi_loop(
    physical_mic_id: Option<&str>,
    virtual_mic_render_id: Option<&str>,
    mic_strip: &mut RealTimeBusStrip,
    meters: &SharedMeterBank,
    diagnostics: &EngineDiagnostics,
    stop_flag: &AtomicBool,
    stream_mic_tx: &mut PidRingProducer,
    stream_mic_commands: &mut Consumer<PidRingProducer>,
    monitor_mic_tx: &mut PidRingProducer,
    stream_mic_post_fader: &AtomicBool,
) -> Result<(), String> {
    let mic_device = resolve_mmdevice(physical_mic_id, eCapture)?;
    let mic_client: IAudioClient = mic_device
        .Activate(CLSCTX_ALL, None)
        .map_err(|e| format!("Mic Activate(IAudioClient) failed: {e}"))?;

    let wave_format = standard_float32_stereo_format();
    let capture_flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK
        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;

    // Request minimum shared engine period (0 -> 10ms / 480 frames) with guaranteed IEEE Float32 format
    if mic_client
        .Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            capture_flags,
            0,
            0,
            &wave_format,
            None,
        )
        .is_err()
    {
        mic_client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                capture_flags,
                100_000,
                0,
                &wave_format,
                None,
            )
            .map_err(|e| format!("Mic IAudioClient::Initialize failed: {e}"))?;
    }

    let mic_event = CreateEventW(None, false, false, None)
        .map_err(|e| format!("CreateEventW(mic_event) failed: {e}"))?;
    mic_client
        .SetEventHandle(mic_event)
        .map_err(|e| format!("Mic SetEventHandle failed: {e}"))?;

    let capture_client: IAudioCaptureClient = mic_client
        .GetService()
        .map_err(|e| format!("Mic GetService(IAudioCaptureClient) failed: {e}"))?;

    // Optional Virtual Mic Render Endpoint (e.g., Stellars - Virtual Mic In / VB-Cable Input)
    let mut virtual_render_opt: Option<(IAudioClient, IAudioRenderClient, u32)> = None;
    if let Some(virt_id) = virtual_mic_render_id {
        if !virt_id.trim().is_empty() {
            if let Ok(virt_dev) = resolve_mmdevice(Some(virt_id), eRender) {
                if let Ok(virt_client) = virt_dev.Activate::<IAudioClient>(CLSCTX_ALL, None) {
                    let render_flags = AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
                    // Use 12ms (120_000 hns) shared buffer so every 10ms mic packet writes contiguously without underrun or chop
                    if virt_client
                        .Initialize(
                            AUDCLNT_SHAREMODE_SHARED,
                            render_flags,
                            120_000,
                            0,
                            &wave_format,
                            None,
                        )
                        .is_ok()
                    {
                        if let (Ok(rc), Ok(buf_sz)) = (
                            virt_client.GetService::<IAudioRenderClient>(),
                            virt_client.GetBufferSize(),
                        ) {
                            let _ = virt_client.Start();
                            virtual_render_opt = Some((virt_client, rc, buf_sz));
                        }
                    }
                }
            }
        }
    }

    let (mut ring_tx, mut ring_rx) = RingBuffer::<f32>::new(PID_RING_CAPACITY_SAMPLES);
    let mut scratch = [0.0_f32; MAX_BLOCK_SAMPLES];
    let mut mic_lufs = LufsMeter::new();

    mic_client
        .Start()
        .map_err(|e| format!("Mic IAudioClient::Start failed: {e}"))?;
    diagnostics.mic_running.store(true, Ordering::Release);

    while !stop_flag.load(Ordering::Relaxed) {
        let wait_res = WaitForSingleObject(mic_event, 50);
        while let Ok(producer) = stream_mic_commands.pop() {
            *stream_mic_tx = producer;
        }
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
            let mut frames_read: u32 = 0;
            let mut flags: u32 = 0;
            if capture_client
                .GetBuffer(&mut data_ptr, &mut frames_read, &mut flags, None, None)
                .is_err()
            {
                break;
            }

            let frames = (frames_read as usize).min(MAX_BLOCK_FRAMES);
            let samples = frames * 2;
            if (flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)) != 0 || data_ptr.is_null() {
                scratch[..samples].fill(0.0);
            } else {
                let src = std::slice::from_raw_parts(data_ptr as *const f32, samples);
                scratch[..samples].copy_from_slice(src);
            }
            let _ = capture_client.ReleaseBuffer(frames_read);

            // Share Mic DSP, then split independent Stream and Virtual Mic output levels.
            mic_strip.process_pre_fader(&mut scratch[..samples]);
            let stream_uses_virtual_mic = stream_mic_post_fader.load(Ordering::Relaxed);
            if !stream_uses_virtual_mic {
                stream_mic_tx.push_interleaved(&scratch[..samples]);
            }
            mic_strip.process_output_fader(&mut scratch[..samples]);
            mic_strip.extras.process_limiter(&mut scratch[..samples]);
            for s in &mut scratch[..samples] {
                *s = soft_limit_sample(*s);
            }
            meters.buses[BusId::Mic.index()].set_reductions(mic_strip.reductions());
            meters.buses[BusId::Mic.index()].update_from_interleaved(&scratch[..samples]);
            meters.buses[BusId::Mic.index()].set_lufs(mic_lufs.process(&scratch[..samples]));
            monitor_mic_tx.push_interleaved(&scratch[..samples]);

            // The stream mix hears the same signal the virtual mic carries (default).
            if stream_uses_virtual_mic {
                stream_mic_tx.push_interleaved(&scratch[..samples]);
            }

            // Forward processed mic frames contiguously to Virtual Mic Render endpoint
            if let Some((ref virt_client, ref virt_rc, buf_sz)) = virtual_render_opt {
                for sample in &scratch[..samples] {
                    let _ = ring_tx.push(*sample);
                }
                if let Ok(padding) = virt_client.GetCurrentPadding() {
                    let can_write =
                        (buf_sz.saturating_sub(padding) as usize).min(ring_rx.slots() / 2);
                    if can_write > 0 {
                        if let Ok(out_ptr) = virt_rc.GetBuffer(can_write as u32) {
                            let out_slice = std::slice::from_raw_parts_mut(
                                out_ptr as *mut f32,
                                can_write * 2,
                            );
                            for dst in out_slice.iter_mut() {
                                *dst = ring_rx.pop().unwrap_or(0.0);
                            }
                            let _ = virt_rc.ReleaseBuffer(can_write as u32, 0);
                        }
                    }
                    // Only discard excess if a device stall ever causes >40ms (1920 stereo frames) of unconsumed backlog
                    let stall_ceiling_samples = 1920 * 2;
                    if ring_rx.slots() > stall_ceiling_samples {
                        let keep_samples = 480 * 2;
                        while ring_rx.slots() > keep_samples {
                            let _ = ring_rx.pop();
                            let _ = ring_rx.pop();
                        }
                    }
                }
            }
        }
    }

    let _ = mic_client.Stop();
    if let Some((virt_client, _, _)) = virtual_render_opt {
        let _ = virt_client.Stop();
    }
    let _ = CloseHandle(mic_event);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_mix_crossfader_math() {
        // Center (0.0): both Game and Chat at unity (1.0)
        let (g_game, g_chat) = compute_chat_mix_gains(0.0);
        assert!((g_game - 1.0).abs() < 1e-6);
        assert!((g_chat - 1.0).abs() < 1e-6);

        // Full Game (-1.0): Game = 1.0, Chat = 0.0
        let (g_game, g_chat) = compute_chat_mix_gains(-1.0);
        assert!((g_game - 1.0).abs() < 1e-6);
        assert!(g_chat.abs() < 1e-6);

        // Full Chat (+1.0): Game = 0.0, Chat = 1.0
        let (g_game, g_chat) = compute_chat_mix_gains(1.0);
        assert!(g_game.abs() < 1e-6);
        assert!((g_chat - 1.0).abs() < 1e-6);

        // Half Chat (+0.5): Game = (1 - 0.5)^2 = 0.25, Chat = 1.0
        let (g_game, g_chat) = compute_chat_mix_gains(0.5);
        assert!((g_game - 0.25).abs() < 1e-6);
        assert!((g_chat - 1.0).abs() < 1e-6);
    }
}
