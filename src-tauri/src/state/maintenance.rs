//! Repair and support actions on the running app: restoring Windows audio, resuming the engine,
//! the tray's quick actions, setup checks and the support file.

use crate::commands::{build_diagnostics, AppState};
use crate::interop::cables::inspect_virtual_cables;
use crate::interop::sessions::enumerate_audio_endpoints;
use crate::interop::silent_sink;
use crate::interop::uninstall::{load_state, reset_routing_and_mutes, restore_saved_defaults};
use crate::state::config::BusId;
use crate::state::health::{redact_user_names, setup_checks, HealthCheck};
use crate::state::logging;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

const SUPPORT_FILES_KEPT: usize = 5;
const SUPPORT_LOG_LINES: usize = 400;

impl AppState {
    /// Stops Stellars' routing and puts Windows audio back as it was: every app plays to its
    /// normal device again, a muted silent sink is unmuted and the saved default devices return.
    /// Your assignments are kept; use [`AppState::resume_audio_engine`] to start routing again.
    pub fn restore_windows_audio(&self) -> Vec<String> {
        let engine = self.current_engine();
        let sink = engine.config.lock().ok().and_then(|c| c.dummy_sink_device_id.clone());
        engine.shutdown();
        let mut log = vec!["Stopped Stellars' routing".to_string()];
        log.extend(reset_routing_and_mutes(sink.as_deref()));
        log.extend(restore_saved_defaults());
        for line in &log {
            logging::info(&format!("Restore Windows audio: {line}"));
        }
        log
    }

    /// Starts the audio engine again after [`AppState::restore_windows_audio`].
    pub fn resume_audio_engine(&self) -> Result<(), String> {
        self.restart_engine()?;
        let engine = self.current_engine();
        let cfg = engine.config.lock().map_err(|_| "Config lock poisoned".to_string())?.clone();
        if let Ok(endpoints) = enumerate_audio_endpoints() {
            for line in silent_sink::sync_mute(None, &cfg, &endpoints) {
                logging::info(&line);
            }
        }
        Ok(())
    }

    /// Mutes or unmutes the Mic channel. Returns true when it is now muted.
    pub fn toggle_mic_mute(&self) -> bool {
        let engine = self.current_engine();
        let Some((volume, muted, pre_gain)) = engine
            .config
            .lock()
            .ok()
            .and_then(|cfg| cfg.buses.get(&BusId::Mic).map(|b| (b.volume, b.muted, b.pre_gain_db)))
        else {
            return false;
        };
        engine.update_bus_volume_mute(BusId::Mic, volume, !muted, pre_gain);
        self.persist_config(&engine);
        !muted
    }

    /// True while the Mic channel is muted.
    pub fn is_mic_muted(&self) -> bool {
        self.current_engine()
            .config
            .lock()
            .map(|cfg| cfg.buses.get(&BusId::Mic).is_some_and(|b| b.muted))
            .unwrap_or(false)
    }

    /// Applies the scene after the active one (wrapping around). Returns its name.
    pub fn next_scene(&self) -> Option<String> {
        let store = self.presets_snapshot();
        let next = {
            let active = store.active_scene_id.as_deref();
            let index = store.scenes.iter().position(|s| Some(s.id.as_str()) == active);
            store.scenes.get(index.map_or(0, |i| (i + 1) % store.scenes.len().max(1)))?
        };
        self.apply_scene_by_id(&next.id).ok().map(|_| next.name.clone())
    }

    pub fn setup_checks(&self) -> Vec<HealthCheck> {
        let engine = self.current_engine();
        let endpoints = enumerate_audio_endpoints().unwrap_or_default();
        let cfg = engine.config.lock().map(|c| c.clone()).unwrap_or_default();
        let cables = inspect_virtual_cables(&cfg, &endpoints);
        setup_checks(engine.diagnostics.master_running.load(Ordering::Acquire), &cables, &endpoints)
    }

    /// Writes one text file with the diagnostics, setup checks, settings and recent log, with
    /// Windows user names removed, so it can be attached to a bug report. Returns its path.
    pub fn write_support_file(&self) -> Result<PathBuf, String> {
        let engine = self.current_engine();
        let report = build_diagnostics(self)?;
        let cfg = engine.config.lock().map(|c| c.clone()).map_err(|_| "Config lock poisoned".to_string())?;

        let mut text = String::new();
        let mut section = |title: &str, body: String| {
            let _ = writeln!(text, "===== {title} =====\n{body}\n");
        };
        section("Stellars support file", format!("Version {}", report.app_version));
        section("Diagnostics", serde_json::to_string_pretty(&report).unwrap_or_default());
        section("Setup checks", serde_json::to_string_pretty(&self.setup_checks()).unwrap_or_default());
        section("Settings", serde_json::to_string_pretty(&self.app_settings()).unwrap_or_default());
        section("Mixer configuration", serde_json::to_string_pretty(&cfg).unwrap_or_default());
        section(
            "Install record",
            load_state().map_or("none".to_string(), |s| serde_json::to_string_pretty(&s).unwrap_or_default()),
        );
        section("Recent log", logging::tail(SUPPORT_LOG_LINES).join("\n"));

        let dir = self.data_dir().join("support");
        fs::create_dir_all(&dir).map_err(|e| format!("Could not create the support folder: {e}"))?;
        let path = dir.join(format!("stellars-support-{}.txt", logging::unix_now_secs()));
        fs::write(&path, redact_user_names(&text)).map_err(|e| format!("Could not write the support file: {e}"))?;
        prune_support_files(&dir);
        Ok(path)
    }
}

/// Keeps only the newest few support files.
fn prune_support_files(dir: &std::path::Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    files.sort();
    let excess = files.len().saturating_sub(SUPPORT_FILES_KEPT);
    for old in files.into_iter().take(excess) {
        let _ = fs::remove_file(old);
    }
}
