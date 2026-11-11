use crate::audio::engine::AudioRouterEngine;
use crate::dsp::biquad::{BiquadCoeffs, DEFAULT_SAMPLE_RATE};
use crate::dsp::eq_bank::evaluate_composite_response_db;
use crate::dsp::meter::AllBusMetersPayload;
use crate::interop::cables::{
    apply_system_virtual_cable_names, auto_assign_virtual_routes, inspect_virtual_cables,
    is_virtual_audio_endpoint_name, run_elevated_vbcable_installer, VirtualCableSetupResult,
    VirtualCableStatusPayload,
};
use crate::interop::policy::clear_all_persisted_endpoints;
use crate::interop::silent_sink;
use crate::interop::sessions::{
    enumerate_audio_endpoints, ActiveAudioProcess, AudioEndpointInfo,
};
use crate::interop::autostart;
use crate::interop::sessions::device_mix_rate;
use crate::state::app_settings::AppSettings;
use crate::state::backup::{self, BackupInfo, BackupKind};
use crate::state::health::{estimate_latency_ms, HealthCheck};
use crate::state::logging;
use crate::state::meter_pacing::{self, MeterPacing};
use crate::state::presets::{self, OutputProfile, PresetStore, Scene, SceneSnapshot};
use crate::state::config::{
    BusId, CompressorConfig, DeEsserConfig, DuckingConfig, EqBandConfig, LimiterConfig,
    NoiseGateConfig, NoiseSuppressionConfig, RouterConfig, StreamConfig,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use crate::state::debounced_save::DebouncedSaver;
use tauri::ipc::Channel;
use tauri::State;

/// How often the meter thread re-checks while paused.
const PAUSED_POLL: Duration = Duration::from_millis(100);

pub struct AppState {
    pub engine: Arc<Mutex<Arc<AudioRouterEngine>>>,
    pub config_path: PathBuf,
    pub meter_stream_cancel: Mutex<Option<Arc<AtomicBool>>>,
    /// How often the meter stream sends; shared with the stream thread.
    pub meter_pacing: Arc<MeterPacing>,
    pub settings: Mutex<AppSettings>,
    pub settings_path: PathBuf,
    pub started_at: std::time::Instant,
    pub presets: Mutex<PresetStore>,
    pub presets_path: PathBuf,
    /// Buses that were already muted when "mute all" was triggered, so un-muting restores them.
    mute_all_snapshot: Mutex<Option<Vec<BusId>>>,
    /// Writes the live config shortly after the last change instead of on every one.
    config_saver: DebouncedSaver,
    /// True when the previous session did not end cleanly (set once at startup).
    pub recovered_from_crash: AtomicBool,
}

fn log_actions(actions: Vec<String>) {
    for line in actions {
        logging::info(&line);
    }
}

impl AppState {
    pub fn new(config_path: PathBuf) -> Self {
        let is_first_run = !config_path.exists();
        let settings_path = AppSettings::default_path();
        let mut settings = AppSettings::load_from_path(&settings_path);
        // Existing installs (config present, no app settings yet) should not see the first-run wizard.
        if !settings_path.exists() && !is_first_run {
            settings.onboarding_completed = true;
            let _ = settings.save_to_path(&settings_path);
        }
        let meter_pacing = Arc::new(MeterPacing::new());
        meter_pacing.set_saving_enabled(settings.save_resources_in_background);
        let mut initial_cfg = RouterConfig::load_from_path(&config_path);
        let startup_endpoints = enumerate_audio_endpoints().ok();
        if let Some(endpoints) = &startup_endpoints {
            if is_first_run {
                let _ = auto_assign_virtual_routes(&mut initial_cfg, endpoints, false);
                let _ = initial_cfg.save_to_path(&config_path);
            }
            // A spare output used as the silent sink must stay muted, even if it was unmuted
            // in Windows while Stellars was closed.
            log_actions(silent_sink::sync_mute(None, &initial_cfg, endpoints));
        }
        let _ = apply_system_virtual_cable_names(&initial_cfg);
        let engine = Arc::new(Mutex::new(AudioRouterEngine::start(initial_cfg)));
        let saver_engine = Arc::clone(&engine);
        let saver_path = config_path.clone();
        let config_saver = DebouncedSaver::spawn(Duration::from_millis(400), move || {
            let current = saver_engine.lock().ok().map(|g| Arc::clone(&*g));
            if let Some(current) = current {
                if let Ok(cfg) = current.config.lock() {
                    let _ = cfg.save_to_path(&saver_path);
                }
            }
        });
        let presets_path = presets::presets_path(config_path.parent().unwrap_or_else(|| std::path::Path::new(".")));
        Self {
            engine,
            presets: Mutex::new(PresetStore::load(&presets_path)),
            presets_path,
            config_path,
            meter_stream_cancel: Mutex::new(None),
            meter_pacing,
            settings: Mutex::new(settings),
            settings_path,
            started_at: std::time::Instant::now(),
            mute_all_snapshot: Mutex::new(None),
            config_saver,
            recovered_from_crash: AtomicBool::new(false),
        }
    }

    pub fn presets_snapshot(&self) -> PresetStore {
        self.presets
            .lock()
            .map(|p| p.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    /// Applies `mutate` to the preset store and persists it; the store is unchanged on error.
    fn update_presets<R>(
        &self,
        mutate: impl FnOnce(&mut PresetStore) -> Result<R, String>,
    ) -> Result<(PresetStore, R), String> {
        let mut guard = self.presets.lock().map_err(|_| "Presets lock poisoned".to_string())?;
        let mut next = guard.clone();
        let result = mutate(&mut next)?;
        next.save(&self.presets_path)?;
        *guard = next.clone();
        Ok((next, result))
    }

    /// Applies a scene to the running engine without interrupting audio, and returns the config.
    pub fn apply_scene_by_id(&self, id: &str) -> Result<RouterConfig, String> {
        let scene = self
            .presets_snapshot()
            .scene(id)
            .cloned()
            .ok_or_else(|| "That scene no longer exists".to_string())?;
        let engine = self.current_engine();
        let mut cfg = engine
            .config
            .lock()
            .map_err(|_| "Config lock poisoned".to_string())?
            .clone();
        scene.snapshot.apply_to(&mut cfg);
        engine.apply_scene_live(&cfg.normalized())?;
        self.persist_config(&engine);
        self.update_presets(|store| {
            store.active_scene_id = Some(scene.id.clone());
            Ok(())
        })?;
        logging::info(&format!("Scene applied: {}", scene.name));
        let applied = engine
            .config
            .lock()
            .map_err(|_| "Config lock poisoned".to_string())?
            .clone();
        Ok(applied)
    }

    pub fn data_dir(&self) -> PathBuf {
        self.config_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Stops the running engine and starts a fresh one from `cfg` (re-binds every device).
    pub fn restart_engine_with(&self, cfg: RouterConfig) -> Result<(), String> {
        let mut guard = self
            .engine
            .lock()
            .map_err(|_| "Engine lock poisoned".to_string())?;
        guard.shutdown();
        *guard = AudioRouterEngine::start(cfg);
        let _ = guard.scan_and_sync_processes();
        Ok(())
    }

    pub fn restart_engine(&self) -> Result<(), String> {
        let cfg = self
            .current_engine()
            .config
            .lock()
            .map_err(|_| "Config lock poisoned".to_string())?
            .clone();
        self.restart_engine_with(cfg)
    }

    /// True while "mute all" from the tray is active.
    pub fn is_all_muted(&self) -> bool {
        self.mute_all_snapshot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_some()
    }

    /// Mutes every bus, or restores the previous mute state if called again. Returns `true`
    /// when everything is now muted.
    pub fn toggle_mute_all(&self) -> bool {
        let engine = self.current_engine();
        let buses: Vec<(BusId, f32, bool, f32)> = match engine.config.lock() {
            Ok(cfg) => BusId::ALL
                .iter()
                .filter_map(|b| cfg.buses.get(b).map(|c| (*b, c.volume, c.muted, c.pre_gain_db)))
                .collect(),
            Err(_) => return false,
        };
        let mut snapshot = self
            .mute_all_snapshot
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let now_muted = match snapshot.take() {
            Some(previously_muted) => {
                for (bus, volume, _, pre_gain) in &buses {
                    engine.update_bus_volume_mute(*bus, *volume, previously_muted.contains(bus), *pre_gain);
                }
                false
            }
            None => {
                let already: Vec<BusId> = buses.iter().filter(|b| b.2).map(|b| b.0).collect();
                for (bus, volume, _, pre_gain) in &buses {
                    engine.update_bus_volume_mute(*bus, *volume, true, *pre_gain);
                }
                *snapshot = Some(already);
                true
            }
        };
        self.persist_config(&engine);
        now_muted
    }

    /// Snapshot of the current app settings (poison-tolerant).
    pub fn app_settings(&self) -> AppSettings {
        self.settings
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    pub fn current_engine(&self) -> Arc<AudioRouterEngine> {
        Arc::clone(&self.engine.lock().expect("Engine lock poisoned"))
    }

    /// Queues a save of the live config; rapid changes collapse into one write.
    pub fn persist_config(&self, _engine: &AudioRouterEngine) {
        self.config_saver.request();
    }

    /// Writes the live config right now (used before the app quits).
    pub fn flush_config(&self) {
        let engine = self.current_engine();
        let cfg = engine.config.lock().map(|cfg| cfg.clone());
        if let Ok(cfg) = cfg {
            let _ = cfg.save_to_path(&self.config_path);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsSnapshot {
    #[serde(rename = "masterRunning")]
    pub master_running: bool,
    #[serde(rename = "micRunning")]
    pub mic_running: bool,
    #[serde(rename = "callbackCount")]
    pub callback_count: u64,
    #[serde(rename = "underrunCount")]
    pub underrun_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullRouterStatePayload {
    pub config: RouterConfig,
    pub endpoints: Vec<AudioEndpointInfo>,
    pub processes: Vec<ActiveAudioProcess>,
    pub diagnostics: DiagnosticsSnapshot,
    #[serde(rename = "cableStatus")]
    pub cable_status: VirtualCableStatusPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqResponsePayload {
    pub frequencies: Vec<f32>,
    #[serde(rename = "compositeDb")]
    pub composite_db: Vec<f32>,
    #[serde(rename = "perBandDb")]
    pub per_band_db: Vec<Vec<f32>>,
}

pub fn build_log_frequencies(num_points: usize) -> Vec<f32> {
    let n = num_points.max(16);
    let min_f = 20.0_f32.ln();
    let max_f = 20000.0_f32.ln();
    (0..n)
        .map(|i| {
            let t = (i as f32) / ((n - 1) as f32);
            (min_f + t * (max_f - min_f)).exp()
        })
        .collect()
}

pub fn compute_eq_response_payload(bands: &[EqBandConfig], num_points: usize) -> EqResponsePayload {
    let frequencies = build_log_frequencies(num_points);
    let composite_db = evaluate_composite_response_db(bands, &frequencies, DEFAULT_SAMPLE_RATE);
    let per_band_db = bands
        .iter()
        .map(|b| {
            let c = BiquadCoeffs::from_config(b, DEFAULT_SAMPLE_RATE);
            frequencies
                .iter()
                .map(|&f| c.magnitude_db(f, DEFAULT_SAMPLE_RATE))
                .collect()
        })
        .collect();

    EqResponsePayload {
        frequencies,
        composite_db,
        per_band_db,
    }
}

#[tauri::command]
pub fn get_router_state(state: State<'_, AppState>) -> Result<FullRouterStatePayload, String> {
    let engine = state.current_engine();
    let endpoints = enumerate_audio_endpoints().unwrap_or_default();
    let processes = engine.scan_and_sync_processes().unwrap_or_default();
    let config = engine
        .config
        .lock()
        .map_err(|_| "Config lock poisoned".to_string())?
        .clone();

    let diagnostics = DiagnosticsSnapshot {
        master_running: engine.diagnostics.master_running.load(Ordering::Acquire),
        mic_running: engine.diagnostics.mic_running.load(Ordering::Acquire),
        callback_count: engine.diagnostics.callback_count.load(Ordering::Relaxed),
        underrun_count: engine.diagnostics.underrun_count.load(Ordering::Relaxed),
    };

    let cable_status = inspect_virtual_cables(&config, &endpoints);

    Ok(FullRouterStatePayload {
        config,
        endpoints,
        processes,
        diagnostics,
        cable_status,
    })
}

#[tauri::command]
pub fn refresh_audio_processes(
    state: State<'_, AppState>,
) -> Result<Vec<ActiveAudioProcess>, String> {
    let engine = state.current_engine();
    engine.scan_and_sync_processes()
}

#[tauri::command]
pub fn set_chat_mix(state: State<'_, AppState>, balance: f32) -> Result<f32, String> {
    let engine = state.current_engine();
    engine.set_chat_mix(balance);
    state.persist_config(&engine);
    Ok(balance.clamp(-1.0, 1.0))
}

#[tauri::command]
pub fn set_master_volume(state: State<'_, AppState>, volume: f32) -> Result<f32, String> {
    let engine = state.current_engine();
    engine.set_master_volume(volume);
    state.persist_config(&engine);
    Ok(volume.clamp(0.0, 2.0))
}

fn validate_stream_destination(cfg: &RouterConfig, stream: &StreamConfig) -> Result<(), String> {
    if !stream.enabled || stream.muted {
        return Ok(());
    }
    if stream.enabled && stream.output_device_id.is_none() {
        return Err("Choose a dedicated streaming cable before enabling Stream.".into());
    }
    if let Some(id) = &stream.output_device_id {
        if [
            &cfg.master_output_device_id,
            &cfg.dummy_sink_device_id,
            &cfg.virtual_mic_output_device_id,
        ]
        .into_iter()
        .any(|other| other.as_ref() == Some(id))
        {
            return Err(
                "Stream needs a separate output from headphones, the routing sink, and virtual mic."
                    .into(),
            );
        }
        let endpoints = enumerate_audio_endpoints()?;
        if !endpoints.iter().any(|e| e.id == *id && e.flow == "Render") {
            return Err(
                "The selected streaming output is unavailable. Reconnect the cable and refresh devices."
                    .into(),
            );
        }
        if cfg.master_output_device_id.is_none()
            && endpoints.iter().any(|e| e.id == *id && e.is_default)
        {
            return Err(
                "Stream cannot use the default headphone output. Choose a separate cable.".into(),
            );
        }
    }
    Ok(())
}

#[tauri::command]
pub fn set_stream_config(
    state: State<'_, AppState>,
    stream: StreamConfig,
) -> Result<RouterConfig, String> {
    let stream = stream.normalized();
    let guard = state.engine.lock().map_err(|_| "Engine lock poisoned")?;
    let mut cfg = guard
        .config
        .lock()
        .map_err(|_| "Config lock poisoned")?
        .clone();
    let previous = cfg.clone();
    validate_stream_destination(&cfg, &stream)?;
    cfg.stream = stream;
    let _ = apply_system_virtual_cable_names(&cfg);
    // Persist first; a failed write leaves the running engine and UI configuration untouched.
    cfg.save_to_path(&state.config_path)?;
    if let Err(error) = guard.update_stream_output(&cfg.stream) {
        let _ = previous.save_to_path(&state.config_path);
        return Err(error);
    }
    *guard.config.lock().map_err(|_| "Config lock poisoned")? = cfg.clone();
    let _ = guard.scan_and_sync_processes();
    Ok(cfg)
}

#[tauri::command]
pub fn set_stream_channel(
    state: State<'_, AppState>, bus: BusId, volume: f32, muted: bool,
) -> Result<(), String> {
    // Serialize against full stream settings saves and engine replacements.
    let engine = state.engine.lock().map_err(|_| "Engine lock poisoned")?;
    let mut cfg = engine.config.lock().map_err(|_| "Config lock poisoned")?.clone();
    let channel = crate::state::config::StreamChannelConfig { volume, muted }.normalized();
    cfg.stream.channels.insert(bus, channel);
    cfg.save_to_path(&state.config_path)?;
    engine.stream.lock().map_err(|_| "Stream lock poisoned")?.update(&cfg.stream);
    *engine.config.lock().map_err(|_| "Config lock poisoned")? = cfg;
    Ok(())
}

/// Stream playback audition: live-only monitor level, never saved.
#[tauri::command]
pub fn set_bus_audition(state: State<'_, AppState>, bus: BusId, volume: f32, muted: bool) -> Result<(), String> {
    let engine = state.current_engine();
    engine.audition_bus(bus, volume, muted);
    if bus == BusId::Mic {
        engine.mic_to_monitor_atomic.store(!muted, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub fn set_bus_volume_mute(
    state: State<'_, AppState>,
    bus: BusId,
    volume: f32,
    muted: bool,
    pre_gain_db: f32,
) -> Result<(), String> {
    let engine = state.current_engine();
    engine.update_bus_volume_mute(bus, volume, muted, pre_gain_db);
    state.persist_config(&engine);
    Ok(())
}

#[tauri::command]
pub fn set_eq_band(
    state: State<'_, AppState>,
    bus: BusId,
    band_index: usize,
    band: EqBandConfig,
) -> Result<EqResponsePayload, String> {
    let engine = state.current_engine();
    engine.update_eq_band(bus, band_index, band);
    state.persist_config(&engine);

    let bands = {
        let cfg = engine
            .config
            .lock()
            .map_err(|_| "Config lock poisoned".to_string())?;
        cfg.buses
            .get(&bus)
            .map(|b| b.eq_bands.clone())
            .unwrap_or_default()
    };
    Ok(compute_eq_response_payload(&bands, 240))
}

#[tauri::command]
pub fn get_eq_curve(
    state: State<'_, AppState>,
    bus: BusId,
    num_points: Option<usize>,
) -> Result<EqResponsePayload, String> {
    let engine = state.current_engine();
    let bands = {
        let cfg = engine
            .config
            .lock()
            .map_err(|_| "Config lock poisoned".to_string())?;
        cfg.buses
            .get(&bus)
            .map(|b| b.eq_bands.clone())
            .unwrap_or_default()
    };
    Ok(compute_eq_response_payload(&bands, num_points.unwrap_or(240)))
}

#[tauri::command]
pub fn set_bus_dynamics(
    state: State<'_, AppState>,
    bus: BusId,
    noise_gate: NoiseGateConfig,
    compressor: CompressorConfig,
) -> Result<(), String> {
    let engine = state.current_engine();
    engine.update_bus_dynamics(bus, noise_gate, compressor);
    state.persist_config(&engine);
    Ok(())
}

#[tauri::command]
pub fn route_process_to_bus(
    state: State<'_, AppState>,
    pid: u32,
    exe_name: Option<String>,
    bus: BusId,
) -> Result<(), String> {
    let engine = state.current_engine();
    engine.assign_process_to_bus(pid, exe_name, bus)?;
    state.persist_config(&engine);
    Ok(())
}

#[tauri::command]
pub fn unroute_process(state: State<'_, AppState>, pid: u32) -> Result<(), String> {
    let engine = state.current_engine();
    engine.unroute_process(pid)?;
    state.persist_config(&engine);
    Ok(())
}

#[tauri::command]
pub fn reset_all_endpoints(state: State<'_, AppState>) -> Result<(), String> {
    let engine = state.current_engine();
    if let Ok(mut cfg) = engine.config.lock() {
        cfg.process_routing_rules.clear();
    }
    state.persist_config(&engine);
    let _ = clear_all_persisted_endpoints();
    let _ = engine.scan_and_sync_processes();
    Ok(())
}

#[tauri::command]
pub fn set_audio_devices(
    state: State<'_, AppState>,
    master_output_device_id: Option<String>,
    dummy_sink_device_id: Option<String>,
    physical_mic_device_id: Option<String>,
    virtual_mic_output_device_id: Option<String>,
    stream_output_device_id: Option<Option<String>>,
) -> Result<RouterConfig, String> {
    let updated_cfg = {
        let mut engine_guard = state
            .engine
            .lock()
            .map_err(|_| "Engine lock poisoned".to_string())?;
        let mut cfg = engine_guard
            .config
            .lock()
            .map_err(|_| "Config lock poisoned".to_string())?
            .clone();
        let previous_sink = cfg.dummy_sink_device_id.clone();

        cfg.master_output_device_id = master_output_device_id.filter(|s| !s.trim().is_empty());
        cfg.dummy_sink_device_id = dummy_sink_device_id.filter(|s| !s.trim().is_empty());
        cfg.physical_mic_device_id = physical_mic_device_id.filter(|s| !s.trim().is_empty());
        cfg.virtual_mic_output_device_id =
            virtual_mic_output_device_id.filter(|s| !s.trim().is_empty());
        if let Some(stream_id_opt) = stream_output_device_id {
            cfg.stream.output_device_id = stream_id_opt.filter(|s| !s.trim().is_empty());
        }

        validate_stream_destination(&cfg, &cfg.stream)?;

        let _ = apply_system_virtual_cable_names(&cfg);
        if let Ok(endpoints) = enumerate_audio_endpoints() {
            log_actions(silent_sink::sync_mute(previous_sink.as_deref(), &cfg, &endpoints));
        }
        let _ = cfg.save_to_path(&state.config_path);
        engine_guard.shutdown();
        *engine_guard = AudioRouterEngine::start(cfg.clone());
        let _ = engine_guard.scan_and_sync_processes();
        cfg
    };
    Ok(updated_cfg)
}

#[tauri::command]
pub fn get_nvidia_recording_status() -> crate::interop::nvidia::NvidiaRecordingStatus {
    crate::interop::nvidia::get_nvidia_recording_status()
}

/// `device_id` = None / empty restores the Windows default microphone for NVIDIA recording.
#[tauri::command]
pub fn set_nvidia_recording_mic(
    device_id: Option<String>,
) -> Result<crate::interop::nvidia::NvidiaRecordingStatus, String> {
    match device_id.filter(|s| !s.trim().is_empty()) {
        Some(id) => {
            let endpoints = enumerate_audio_endpoints()?;
            let ep = endpoints
                .iter()
                .find(|e| e.id == id && e.flow == "Capture")
                .ok_or_else(|| "Selected microphone is no longer available".to_string())?;
            crate::interop::nvidia::set_nvidia_recording_mic(Some((&ep.id, &ep.name)))?;
        }
        None => crate::interop::nvidia::set_nvidia_recording_mic(None)?,
    }
    Ok(crate::interop::nvidia::get_nvidia_recording_status())
}

#[tauri::command]
pub fn get_virtual_cable_status(
    state: State<'_, AppState>,
) -> Result<VirtualCableStatusPayload, String> {
    let engine = state.current_engine();
    let endpoints = enumerate_audio_endpoints()?;
    let cfg = engine
        .config
        .lock()
        .map_err(|_| "Config lock poisoned".to_string())?
        .clone();
    Ok(inspect_virtual_cables(&cfg, &endpoints))
}

// `async`: runs off the UI thread, so the window keeps responding while the engine restarts.
#[tauri::command(async)]
pub fn auto_route_virtual_cables(
    state: State<'_, AppState>,
    overwrite_existing: Option<bool>,
) -> Result<VirtualCableSetupResult, String> {
    let initial_endpoints = enumerate_audio_endpoints()?;
    let mut engine_guard = state
        .engine
        .lock()
        .map_err(|_| "Engine lock poisoned".to_string())?;
    let mut cfg = engine_guard
        .config
        .lock()
        .map_err(|_| "Config lock poisoned".to_string())?
        .clone();

    let previous_sink = cfg.dummy_sink_device_id.clone();
    let mut actions_taken = auto_assign_virtual_routes(
        &mut cfg,
        &initial_endpoints,
        overwrite_existing.unwrap_or(true),
    );
    let rename_actions = apply_system_virtual_cable_names(&cfg);
    actions_taken.extend(rename_actions);
    actions_taken.extend(silent_sink::sync_mute(previous_sink.as_deref(), &cfg, &initial_endpoints));

    let endpoints = enumerate_audio_endpoints().unwrap_or(initial_endpoints);
    let _ = cfg.save_to_path(&state.config_path);
    engine_guard.shutdown();
    *engine_guard = AudioRouterEngine::start(cfg.clone());
    let _ = engine_guard.scan_and_sync_processes();

    let cable_status = inspect_virtual_cables(&cfg, &endpoints);
    Ok(VirtualCableSetupResult {
        config: cfg,
        endpoints,
        cable_status,
        actions_taken,
        reboot_recommended: false,
    })
}

// `async`: the download and driver install take a while; on the UI thread the window would freeze.
#[tauri::command(async)]
pub fn install_or_repair_virtual_cables(
    state: State<'_, AppState>,
    install_hifi_cable: Option<bool>,
    force_repair: Option<bool>,
) -> Result<VirtualCableSetupResult, String> {
    // 1. Snapshot physical default endpoints BEFORE driver installation so Windows cannot hijack headphones/mic
    let pre_endpoints = enumerate_audio_endpoints().unwrap_or_default();
    let pre_phys_render = pre_endpoints
        .iter()
        .find(|e| e.flow == "Render" && e.is_default && !is_virtual_audio_endpoint_name(&e.name))
        .or_else(|| {
            pre_endpoints
                .iter()
                .find(|e| e.flow == "Render" && !is_virtual_audio_endpoint_name(&e.name))
        })
        .map(|e| e.id.clone());
    let pre_phys_mic = pre_endpoints
        .iter()
        .find(|e| e.flow == "Capture" && e.is_default && !is_virtual_audio_endpoint_name(&e.name))
        .or_else(|| {
            pre_endpoints
                .iter()
                .find(|e| e.flow == "Capture" && !is_virtual_audio_endpoint_name(&e.name))
        })
        .map(|e| e.id.clone());

    // 2. Run elevated VB-CABLE + Hi-Fi Cable installation / repair
    let (mut actions_taken, reboot_recommended) = run_elevated_vbcable_installer(
        install_hifi_cable.unwrap_or(true),
        force_repair.unwrap_or(false),
    )?;

    // 3. Re-enumerate endpoints and auto-assign Silent Sink, Virtual Mic, and Stream Mix
    let initial_endpoints = enumerate_audio_endpoints()?;
    let mut engine_guard = state
        .engine
        .lock()
        .map_err(|_| "Engine lock poisoned".to_string())?;
    let mut cfg = engine_guard
        .config
        .lock()
        .map_err(|_| "Config lock poisoned".to_string())?
        .clone();

    if cfg.master_output_device_id.is_none() {
        cfg.master_output_device_id = pre_phys_render;
    }
    if cfg.physical_mic_device_id.is_none() {
        cfg.physical_mic_device_id = pre_phys_mic;
    }

    let route_actions = auto_assign_virtual_routes(&mut cfg, &initial_endpoints, false);
    actions_taken.extend(route_actions);
    let rename_actions = apply_system_virtual_cable_names(&cfg);
    actions_taken.extend(rename_actions);

    let endpoints = enumerate_audio_endpoints().unwrap_or(initial_endpoints);
    let _ = cfg.save_to_path(&state.config_path);
    engine_guard.shutdown();
    *engine_guard = AudioRouterEngine::start(cfg.clone());
    let _ = engine_guard.scan_and_sync_processes();

    let cable_status = inspect_virtual_cables(&cfg, &endpoints);
    Ok(VirtualCableSetupResult {
        config: cfg,
        endpoints,
        cable_status,
        actions_taken,
        reboot_recommended,
    })
}

/// Streams real-time 60Hz VU peak + RMS meters over a Tauri v2 `Channel`.
/// Automatically terminates when the channel disconnects (e.g., when WebView2 is destroyed).
#[tauri::command]
pub fn subscribe_peak_meters(
    state: State<'_, AppState>,
    on_meter: Channel<AllBusMetersPayload>,
) -> Result<(), String> {
    let cancel_flag = Arc::new(AtomicBool::new(false));
    if let Ok(mut prev) = state.meter_stream_cancel.lock() {
        if let Some(old_flag) = prev.replace(Arc::clone(&cancel_flag)) {
            old_flag.store(true, Ordering::Release);
        }
    }

    let engine_state = Arc::clone(&state.engine);
    let pacing = Arc::clone(&state.meter_pacing);
    std::thread::Builder::new()
        .name("sonar-60hz-meter-channel".to_string())
        .spawn(move || {
            // The last snapshot the UI has. Identical snapshots (silence) are not re-sent, and it is
            // cleared while paused so the first snapshot after resuming always goes out.
            let mut last_sent: Option<AllBusMetersPayload> = None;
            while !cancel_flag.load(Ordering::Acquire) {
                let hz = pacing.effective_hz();
                if hz == 0 {
                    last_sent = None;
                    std::thread::sleep(PAUSED_POLL);
                    continue;
                }
                let engine = match engine_state.lock() {
                    Ok(engine) => Arc::clone(&engine),
                    Err(_) => break,
                };
                let payload = engine.meters_snapshot();
                if last_sent.as_ref() != Some(&payload) {
                    if on_meter.send(payload.clone()).is_err() {
                        break;
                    }
                    last_sent = Some(payload);
                }
                std::thread::sleep(meter_pacing::interval_for(hz));
            }
        })
        .map_err(|e| format!("Failed to spawn meter stream thread: {e}"))?;

    Ok(())
}




/// How often the UI wants meter snapshots (60 focused, lower when unfocused). Never touches audio.
#[tauri::command]
pub fn set_meter_rate(state: State<'_, AppState>, hz: u32) {
    // Never 0 from the UI: a stale "hidden" signal there must not freeze a visible window. The
    // window-hidden flag on the Rust side is what stops the stream entirely.
    state.meter_pacing.set_requested_hz(hz.max(1));
}

#[tauri::command]
pub fn window_minimize(window: tauri::Window) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn window_toggle_maximize(window: tauri::Window) -> Result<bool, String> {
    let is_max = window.is_maximized().map_err(|e| e.to_string())?;
    if is_max {
        window.unmaximize().map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        window.maximize().map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[tauri::command]
pub fn window_close(window: tauri::Window) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn window_is_maximized(window: tauri::Window) -> Result<bool, String> {
    window.is_maximized().map_err(|e| e.to_string())
}

/// Returns persisted app settings. `launchOnStartup` always reflects the real registry state so
/// it stays correct if the user removed the entry through Windows Settings / Task Manager.
#[tauri::command]
pub fn get_app_settings(state: State<'_, AppState>) -> AppSettings {
    let mut settings = state.app_settings();
    settings.launch_on_startup = autostart::is_enabled();
    settings
}

/// Persists new app settings and applies the ones that have an immediate system side effect.
#[tauri::command]
pub fn set_app_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    apply_app_settings(&app, &state, settings)
}

fn apply_app_settings(
    app: &tauri::AppHandle,
    state: &AppState,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    use tauri::Manager;

    if settings.launch_on_startup != autostart::is_enabled() {
        autostart::set_enabled(settings.launch_on_startup)?;
    }
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_always_on_top(settings.always_on_top)
            .map_err(|e| e.to_string())?;
    }
    settings.save_to_path(&state.settings_path)?;
    state.meter_pacing.set_saving_enabled(settings.save_resources_in_background);
    if let Ok(mut current) = state.settings.lock() {
        *current = settings.clone();
    }
    Ok(settings)
}

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub version: String,
    #[serde(rename = "dataDir")]
    pub data_dir: String,
    /// True when the previous session did not end cleanly; routing was reset at startup.
    #[serde(rename = "recoveredFromCrash")]
    pub recovered_from_crash: bool,
}

#[tauri::command]
pub fn get_app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        recovered_from_crash: state.recovered_from_crash.load(Ordering::Relaxed),
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: state
            .config_path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    }
}

/// Opens the folder that holds config.json / app_settings.json in Explorer.
#[tauri::command]
pub fn open_data_folder(state: State<'_, AppState>) -> Result<(), String> {
    let dir = state
        .config_path
        .parent()
        .ok_or_else(|| "Config folder is unknown".to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    spawn_detached("explorer.exe", &[dir.as_os_str().to_string_lossy().as_ref()])
}

/// Opens an https:// or mailto: link in the user's default handler. Anything else is rejected.
#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    let allowed = url.starts_with("https://") || url.starts_with("mailto:");
    if !allowed || url.chars().any(|c| c.is_control() || c == ' ') {
        return Err("Only https:// and mailto: links can be opened".to_string());
    }
    spawn_detached("rundll32.exe", &["url.dll,FileProtocolHandler", &url])
}

#[tauri::command]
pub fn quit_application(app: tauri::AppHandle) {
    crate::quit_application(&app);
}

// ---------------------------------------------------------------------------------------------
// Diagnostics, logs and backups
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceHealth {
    pub slot: String,
    pub device_id: Option<String>,
    pub name: Option<String>,
    /// False when a device is configured but no longer present in Windows.
    pub present: bool,
    pub mix_rate_hz: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepFilterStatus {
    pub bus: BusId,
    pub state: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport {
    pub app_version: String,
    pub engine_rate_hz: u32,
    pub uptime_secs: u64,
    pub master_running: bool,
    pub mic_running: bool,
    pub callback_count: u64,
    pub underrun_count: u64,
    /// Average and worst-block CPU load of the mixing thread, in percent of the block time.
    pub cpu_load_percent: f32,
    pub worst_block_percent: f32,
    pub deep_filter: Vec<DeepFilterStatus>,
    pub devices: Vec<DeviceHealth>,
    pub data_dir: String,
    pub log_path: Option<String>,
    /// Estimated delay of the monitor path in milliseconds (0 while the engine is stopped).
    pub latency_ms: f32,
    pub setup_checks: Vec<HealthCheck>,
}

#[tauri::command]
pub fn get_diagnostics(state: State<'_, AppState>) -> Result<DiagnosticsReport, String> {
    build_diagnostics(&state)
}

pub fn build_diagnostics(state: &AppState) -> Result<DiagnosticsReport, String> {
    let engine = state.current_engine();
    let cfg = engine
        .config
        .lock()
        .map_err(|_| "Config lock poisoned".to_string())?
        .clone();
    let endpoints = enumerate_audio_endpoints().unwrap_or_default();

    let slot = |slot: &str, id: &Option<String>, flow: &str, fallback_default: bool| -> Option<DeviceHealth> {
        match id {
            Some(id) => {
                let found = endpoints.iter().find(|e| e.id == *id);
                Some(DeviceHealth {
                    slot: slot.to_string(),
                    device_id: Some(id.clone()),
                    name: found.map(|e| e.name.clone()),
                    present: found.is_some(),
                    mix_rate_hz: found.and_then(|e| device_mix_rate(&e.id)),
                })
            }
            None if fallback_default => {
                let default = endpoints.iter().find(|e| e.flow == flow && e.is_default);
                Some(DeviceHealth {
                    slot: slot.to_string(),
                    device_id: None,
                    name: default.map(|e| format!("{} (system default)", e.name)),
                    present: default.is_some(),
                    mix_rate_hz: default.and_then(|e| device_mix_rate(&e.id)),
                })
            }
            None => None,
        }
    };

    let devices = [
        slot("monitor", &cfg.master_output_device_id, "Render", true),
        slot("microphone", &cfg.physical_mic_device_id, "Capture", true),
        slot("silentSink", &cfg.dummy_sink_device_id, "Render", false),
        slot("virtualMic", &cfg.virtual_mic_output_device_id, "Render", false),
        slot("stream", &cfg.stream.output_device_id, "Render", false),
    ]
    .into_iter()
    .flatten()
    .collect();

    let load = engine.diagnostics.take_load_report();
    let deep_filter_states = engine.deep_filter_states();
    let buffer_frames = engine.diagnostics.render_buffer_frames.load(Ordering::Relaxed);
    let latency_ms = if buffer_frames == 0 {
        0.0
    } else {
        estimate_latency_ms(
            buffer_frames,
            DEFAULT_SAMPLE_RATE as u32,
            deep_filter_states.iter().any(|(_, s)| *s == "ready"),
        )
    };
    Ok(DiagnosticsReport {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        engine_rate_hz: DEFAULT_SAMPLE_RATE as u32,
        uptime_secs: state.started_at.elapsed().as_secs(),
        master_running: engine.diagnostics.master_running.load(Ordering::Acquire),
        mic_running: engine.diagnostics.mic_running.load(Ordering::Acquire),
        callback_count: engine.diagnostics.callback_count.load(Ordering::Relaxed),
        underrun_count: engine.diagnostics.underrun_count.load(Ordering::Relaxed),
        cpu_load_percent: load.0,
        worst_block_percent: load.1,
        deep_filter: deep_filter_states
            .into_iter()
            .map(|(bus, state)| DeepFilterStatus { bus, state })
            .collect(),
        devices,
        data_dir: state.data_dir().display().to_string(),
        log_path: logging::log_file_path().map(|p| p.display().to_string()),
        latency_ms,
        setup_checks: state.setup_checks(),
    })
}

/// Setup checks alone (cables, routes, Windows defaults), for the first-run wizard.
#[tauri::command]
pub fn get_setup_checks(state: State<'_, AppState>) -> Vec<HealthCheck> {
    state.setup_checks()
}

/// Stops Stellars' routing and restores Windows audio to its state before Stellars.
#[tauri::command]
pub fn restore_windows_audio(app: tauri::AppHandle, state: State<'_, AppState>) -> Vec<String> {
    let log = state.restore_windows_audio();
    let _ = tauri::Emitter::emit(&app, "audio-restored", ());
    log
}

/// Starts routing again after [`restore_windows_audio`].
#[tauri::command]
pub fn resume_audio_engine(state: State<'_, AppState>) -> Result<(), String> {
    state.resume_audio_engine()
}

/// Writes the support file and shows it in Explorer. Returns its path.
#[tauri::command]
pub fn export_support_file(state: State<'_, AppState>) -> Result<String, String> {
    let path = state.write_support_file()?;
    let _ = spawn_detached("explorer.exe", &[&format!("/select,{}", path.display())]);
    Ok(path.display().to_string())
}

/// All presets, scenes and output profiles as a shareable JSON text.
#[tauri::command]
pub fn export_presets(state: State<'_, AppState>) -> Result<String, String> {
    serde_json::to_string_pretty(&state.presets_snapshot().to_share_file()).map_err(|e| e.to_string())
}

/// Writes the shareable JSON into the `exports` folder and shows it in Explorer.
#[tauri::command]
pub fn export_presets_file(state: State<'_, AppState>) -> Result<String, String> {
    let json = export_presets(state.clone())?;
    let dir = state.data_dir().join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create the exports folder: {e}"))?;
    let path = dir.join(format!("stellars-presets-{}.json", logging::unix_now_secs()));
    std::fs::write(&path, json).map_err(|e| format!("Could not write the file: {e}"))?;
    let _ = spawn_detached("explorer.exe", &[&format!("/select,{}", path.display())]);
    Ok(path.display().to_string())
}

/// Adds the presets, scenes and output profiles of a shared file to this PC's.
#[tauri::command]
pub fn import_presets(
    state: State<'_, AppState>,
    json: String,
) -> Result<(PresetStore, presets::ImportSummary), String> {
    let file = presets::parse_share_file(&json)?;
    let (store, summary) = state.update_presets(|store| store.merge_share_file(file))?;
    Ok((store, summary))
}

#[tauri::command]
pub fn get_log_tail(lines: Option<usize>) -> Vec<String> {
    logging::tail(lines.unwrap_or(200).clamp(1, 1000))
}

#[tauri::command]
pub fn open_logs_folder() -> Result<(), String> {
    let dir = logging::log_dir().ok_or_else(|| "Logging is not initialised".to_string())?;
    spawn_detached("explorer.exe", &[dir.as_os_str().to_string_lossy().as_ref()])
}

#[tauri::command]
pub fn create_backup(state: State<'_, AppState>) -> Result<BackupInfo, String> {
    let engine = state.current_engine();
    let cfg = engine
        .config
        .lock()
        .map_err(|_| "Config lock poisoned".to_string())?
        .clone();
    let info = backup::create(
        &backup::backups_dir(&state.data_dir()),
        BackupKind::Manual,
        &cfg,
        &state.app_settings(),
        &state.presets_snapshot(),
    )?;
    logging::info(&format!("Backup created: {}", info.name));
    Ok(info)
}

#[tauri::command]
pub fn list_backups(state: State<'_, AppState>) -> Vec<BackupInfo> {
    backup::list(&backup::backups_dir(&state.data_dir()))
}

#[tauri::command]
pub fn delete_backup(state: State<'_, AppState>, name: String) -> Result<(), String> {
    backup::delete(&backup::backups_dir(&state.data_dir()), &name)
}

#[tauri::command]
pub fn open_backups_folder(state: State<'_, AppState>) -> Result<(), String> {
    let dir = backup::backups_dir(&state.data_dir());
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    spawn_detached("explorer.exe", &[dir.as_os_str().to_string_lossy().as_ref()])
}

/// Restores a backup. The current state is backed up first so a restore can itself be undone.
#[tauri::command]
pub fn restore_backup(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<RouterConfig, String> {
    let dir = backup::backups_dir(&state.data_dir());
    let bundle = backup::load(&dir, &name)?;

    let current = state
        .current_engine()
        .config
        .lock()
        .map_err(|_| "Config lock poisoned".to_string())?
        .clone();
    backup::create(&dir, BackupKind::PreRestore, &current, &state.app_settings(), &state.presets_snapshot())?;
    backup::prune_kind(&dir, "prerestore", crate::PRE_RESTORE_BACKUPS_KEPT);

    let cfg = bundle.config.normalized();
    let _ = apply_system_virtual_cable_names(&cfg);
    cfg.save_to_path(&state.config_path)?;
    state.restart_engine_with(cfg.clone())?;

    let restored_presets = bundle.presets;
    state.update_presets(|store| {
        *store = restored_presets;
        Ok(())
    })?;
    let mut settings = bundle.app_settings;
    settings.onboarding_completed = true;
    apply_app_settings(&app, &state, settings)?;
    logging::info(&format!("Restored backup {name}"));
    Ok(cfg)
}

// ---------------------------------------------------------------------------------------------
// Mixing extras, ducking, per-app volume, scenes and output profiles
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub fn set_bus_extras(
    state: State<'_, AppState>,
    bus: BusId,
    limiter: LimiterConfig,
    de_esser: DeEsserConfig,
    noise_suppression: NoiseSuppressionConfig,
) -> Result<(), String> {
    let engine = state.current_engine();
    engine.update_bus_extras(bus, limiter, de_esser, noise_suppression);
    state.persist_config(&engine);
    Ok(())
}

#[tauri::command]
pub fn set_ducking(state: State<'_, AppState>, ducking: DuckingConfig) -> Result<DuckingConfig, String> {
    let engine = state.current_engine();
    engine.update_ducking(ducking.clone());
    state.persist_config(&engine);
    Ok(ducking.normalized())
}

#[tauri::command]
pub fn set_process_volume(
    state: State<'_, AppState>,
    exe_name: String,
    volume: f32,
) -> Result<(), String> {
    let engine = state.current_engine();
    engine.set_process_volume(&exe_name, volume);
    state.persist_config(&engine);
    Ok(())
}

#[tauri::command]
pub fn get_presets(state: State<'_, AppState>) -> PresetStore {
    state.presets_snapshot()
}

/// Saves (or replaces by name) a sound preset: an EQ curve, one Fine-tune section or a recipe.
#[tauri::command]
pub fn save_user_preset(
    state: State<'_, AppState>,
    kind: String,
    bus: BusId,
    name: String,
    data: serde_json::Value,
) -> Result<PresetStore, String> {
    let (store, _) = state.update_presets(|store| store.upsert_user_preset(&kind, bus, &name, data))?;
    Ok(store)
}

#[tauri::command]
pub fn delete_user_preset(state: State<'_, AppState>, id: String) -> Result<PresetStore, String> {
    let (store, _) = state.update_presets(|store| {
        store.user_presets.retain(|p| p.id != id);
        Ok(())
    })?;
    Ok(store)
}

/// Saves the current mixer as a new scene and marks it active.
#[tauri::command]
pub fn save_scene(
    state: State<'_, AppState>,
    name: String,
    auto_apps: Option<Vec<String>>,
) -> Result<PresetStore, String> {
    let name = presets::clean_name(&name)?;
    let cfg = state
        .current_engine()
        .config
        .lock()
        .map_err(|_| "Config lock poisoned".to_string())?
        .clone();
    let (store, _) = state.update_presets(|store| {
        let id = presets::new_id("s");
        store.scenes.push(Scene {
            id: id.clone(),
            name,
            auto_apps: presets::clean_apps(auto_apps.unwrap_or_default()),
            snapshot: SceneSnapshot::capture(&cfg),
        });
        store.active_scene_id = Some(id);
        Ok(())
    })?;
    Ok(store)
}

/// Renames a scene, changes its auto-apply apps, and/or overwrites it with the current mixer.
#[tauri::command]
pub fn update_scene(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    auto_apps: Option<Vec<String>>,
    recapture: Option<bool>,
) -> Result<PresetStore, String> {
    let name = name.map(|n| presets::clean_name(&n)).transpose()?;
    let snapshot = if recapture.unwrap_or(false) {
        let cfg = state
            .current_engine()
            .config
            .lock()
            .map_err(|_| "Config lock poisoned".to_string())?
            .clone();
        Some(SceneSnapshot::capture(&cfg))
    } else {
        None
    };
    let (store, _) = state.update_presets(|store| {
        let scene = store
            .scenes
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| "That scene no longer exists".to_string())?;
        if let Some(name) = name {
            scene.name = name;
        }
        if let Some(apps) = auto_apps {
            scene.auto_apps = presets::clean_apps(apps);
        }
        if let Some(snapshot) = snapshot {
            scene.snapshot = snapshot;
        }
        Ok(())
    })?;
    Ok(store)
}

#[tauri::command]
pub fn delete_scene(state: State<'_, AppState>, id: String) -> Result<PresetStore, String> {
    let (store, _) = state.update_presets(|store| {
        store.scenes.retain(|s| s.id != id);
        if store.active_scene_id.as_deref() == Some(id.as_str()) {
            store.active_scene_id = None;
        }
        Ok(())
    })?;
    Ok(store)
}

#[tauri::command]
pub fn apply_scene(state: State<'_, AppState>, id: String) -> Result<RouterConfig, String> {
    state.apply_scene_by_id(&id)
}

/// Saves the current monitor device, master volume and per-bus EQ as an output profile.
#[tauri::command]
pub fn save_output_profile(state: State<'_, AppState>, name: String) -> Result<PresetStore, String> {
    let name = presets::clean_name(&name)?;
    let cfg = state
        .current_engine()
        .config
        .lock()
        .map_err(|_| "Config lock poisoned".to_string())?
        .clone();
    let (store, _) = state.update_presets(|store| {
        let id = presets::new_id("p");
        store.output_profiles.push(OutputProfile::capture(id.clone(), name, &cfg));
        store.active_profile_id = Some(id);
        Ok(())
    })?;
    Ok(store)
}

#[tauri::command]
pub fn delete_output_profile(state: State<'_, AppState>, id: String) -> Result<PresetStore, String> {
    let (store, _) = state.update_presets(|store| {
        store.output_profiles.retain(|p| p.id != id);
        if store.active_profile_id.as_deref() == Some(id.as_str()) {
            store.active_profile_id = None;
        }
        Ok(())
    })?;
    Ok(store)
}

/// Switches the monitor device, master volume and EQ. Changing the device restarts the engine,
/// so audio is interrupted for a moment.
#[tauri::command]
pub fn apply_output_profile(state: State<'_, AppState>, id: String) -> Result<RouterConfig, String> {
    let profile = state
        .presets_snapshot()
        .output_profiles
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| "That output profile no longer exists".to_string())?;
    let mut cfg = state
        .current_engine()
        .config
        .lock()
        .map_err(|_| "Config lock poisoned".to_string())?
        .clone();
    profile.apply_to(&mut cfg);
    let cfg = cfg.normalized();
    validate_stream_destination(&cfg, &cfg.stream)?;
    let _ = apply_system_virtual_cable_names(&cfg);
    cfg.save_to_path(&state.config_path)?;
    state.restart_engine_with(cfg.clone())?;
    state.update_presets(|store| {
        store.active_profile_id = Some(profile.id.clone());
        Ok(())
    })?;
    logging::info(&format!("Output profile applied: {}", profile.name));
    Ok(cfg)
}

fn spawn_detached(program: &str, args: &[&str]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not launch {program}: {e}"))
}

#[tauri::command]
pub fn set_mic_to_monitor(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let engine = state.current_engine();
    engine.set_mic_to_monitor(enabled);
    state.persist_config(&engine);
    Ok(())
}

