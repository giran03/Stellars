#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod audio;
pub mod commands;
pub mod dsp;
pub mod interop;
pub mod state;
pub mod tray;
pub mod update;

use commands::AppState;
use state::app_settings::CloseBehavior;
use state::config::RouterConfig;
use std::sync::atomic::{AtomicBool, Ordering};
use state::{backup, logging};
use std::collections::{BTreeMap, HashSet};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

const DEVICE_POLL_INTERVAL: Duration = Duration::from_secs(3);
const AUTO_BACKUP_MIN_AGE_SECS: u64 = 24 * 60 * 60;
pub(crate) const AUTO_BACKUPS_KEPT: usize = 5;
pub(crate) const PRE_RESTORE_BACKUPS_KEPT: usize = 5;

#[derive(Clone, serde::Serialize)]
struct DevicesChanged {
    added: Vec<String>,
    removed: Vec<String>,
    /// True when a configured device came back and the engine was restarted to re-bind it.
    reconnected: bool,
}

/// Polls the Windows endpoint list and tells the UI when devices appear or disappear. When a
/// device that is part of the saved routing returns, the engine is restarted so it binds again.
fn spawn_device_watcher(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("device-watch".into())
        .spawn(move || {
            let mut previous: Option<BTreeMap<String, String>> = None;
            let mut missing_configured: HashSet<String> = HashSet::new();
            loop {
                std::thread::sleep(DEVICE_POLL_INTERVAL);
                if QUIT_REQUESTED.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(list) = interop::sessions::enumerate_audio_endpoints() else {
                    continue;
                };
                let current: BTreeMap<String, String> =
                    list.into_iter().map(|e| (e.id, e.name)).collect();

                let mut added = Vec::new();
                let mut removed = Vec::new();
                if let Some(prev) = &previous {
                    added = current.iter().filter(|(id, _)| !prev.contains_key(*id)).map(|(_, n)| n.clone()).collect();
                    removed = prev.iter().filter(|(id, _)| !current.contains_key(*id)).map(|(_, n)| n.clone()).collect();
                }

                let mut reconnected = false;
                if let Some(state) = app.try_state::<AppState>() {
                    let configured: Vec<String> = match state.current_engine().config.lock() {
                        Ok(cfg) => [
                            cfg.master_output_device_id.clone(),
                            cfg.dummy_sink_device_id.clone(),
                            cfg.physical_mic_device_id.clone(),
                            cfg.virtual_mic_output_device_id.clone(),
                            cfg.stream.output_device_id.clone(),
                        ]
                        .into_iter()
                        .flatten()
                        .collect(),
                        Err(_) => Vec::new(),
                    };
                    let now_missing: HashSet<String> = configured
                        .into_iter()
                        .filter(|id| !current.contains_key(id))
                        .collect();
                    let came_back = previous.is_some()
                        && missing_configured.iter().any(|id| !now_missing.contains(id) && current.contains_key(id));
                    if came_back && state.app_settings().auto_reconnect_devices {
                        match state.restart_engine() {
                            Ok(()) => {
                                reconnected = true;
                                logging::info("Configured audio device reconnected; engine restarted");
                            }
                            Err(err) => logging::error(&format!("Engine restart after reconnect failed: {err}")),
                        }
                    }
                    missing_configured = now_missing;
                }

                if !added.is_empty() || !removed.is_empty() {
                    logging::info(&format!("Audio devices changed. added={added:?} removed={removed:?}"));
                    let _ = app.emit("devices-changed", DevicesChanged { added, removed, reconnected });
                }
                previous = Some(current);
            }
        });
}

#[derive(Clone, serde::Serialize)]
struct SceneApplied {
    id: String,
    name: String,
}

/// Applies a scene when one of its auto-apply apps gets focus. Only reacts when the foreground
/// app changes, so manual tweaks are never overwritten while you stay in the same app.
fn spawn_scene_watcher(app: AppHandle) {
    let own_exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()))
        .unwrap_or_default();
    let _ = std::thread::Builder::new()
        .name("scene-watch".into())
        .spawn(move || {
            let mut last_exe = String::new();
            loop {
                std::thread::sleep(Duration::from_millis(1500));
                if QUIT_REQUESTED.load(Ordering::SeqCst) {
                    break;
                }
                let Some(state) = app.try_state::<AppState>() else {
                    continue;
                };
                let store = state.presets_snapshot();
                if store.scenes.iter().all(|s| s.auto_apps.is_empty()) {
                    continue;
                }
                let Some(exe) = interop::foreground::foreground_exe_name() else {
                    continue;
                };
                if exe == own_exe || exe == last_exe {
                    continue;
                }
                last_exe = exe.clone();
                let Some(scene) = store.scene_for_app(&exe) else {
                    continue;
                };
                if store.active_scene_id.as_deref() == Some(scene.id.as_str()) {
                    continue;
                }
                match state.apply_scene_by_id(&scene.id) {
                    Ok(_) => {
                        logging::info(&format!("Scene '{}' applied automatically for {exe}", scene.name));
                        let _ = app.emit("scene-applied", SceneApplied { id: scene.id.clone(), name: scene.name.clone() });
                    }
                    Err(err) => logging::warn(&format!("Automatic scene switch failed: {err}")),
                }
            }
        });
}

/// One automatic backup per day, keeping the newest few.
fn run_auto_backup(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let dir = backup::backups_dir(&state.data_dir());
    if !backup::auto_backup_due(&dir, AUTO_BACKUP_MIN_AGE_SECS) {
        return;
    }
    let Ok(cfg) = state.current_engine().config.lock().map(|c| c.clone()) else {
        return;
    };
    match backup::create(&dir, backup::BackupKind::Auto, &cfg, &state.app_settings(), &state.presets_snapshot()) {
        Ok(info) => {
            logging::info(&format!("Automatic backup created: {}", info.name));
            backup::prune_kind(&dir, "auto", AUTO_BACKUPS_KEPT);
        }
        Err(err) => logging::warn(&format!("Automatic backup failed: {err}")),
    }
}

static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

fn show_or_recreate_main_window(app: &AppHandle) {
    set_main_window_hidden(app, false);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        // Resize events fired while restoring can briefly mark it hidden; it is visible now.
        set_main_window_hidden(app, false);
    } else {
        // Recreate the WebView2 window on demand after it was destroyed on close
        if let Ok(window) = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
            .title("Stellars — Multi-Bus Audio Router")
            .inner_size(1280.0, 820.0)
            .min_inner_size(1024.0, 700.0)
            .resizable(true)
            .decorations(false)
            .shadow(true)
            .build()
        {
            let _ = window.show();
            let _ = window.set_focus();
            set_main_window_hidden(app, false);
        }
    }
}

/// Tells the meter stream whether the main window can be seen, so it can stop while it cannot.
fn set_main_window_hidden(app: &AppHandle, hidden: bool) {
    if let Some(state) = app.try_state::<AppState>() {
        state.meter_pacing.set_window_hidden(hidden);
    }
}

/// Stops the meter stream and audio engine, then exits the whole process.
pub(crate) fn quit_application(app: &AppHandle) {
    QUIT_REQUESTED.store(true, Ordering::SeqCst);
    if let Some(state) = app.try_state::<AppState>() {
        if let Some(flag) = state
            .meter_stream_cancel
            .lock()
            .ok()
            .and_then(|mut g| g.take())
        {
            flag.store(true, Ordering::Release);
        }
        state.flush_config();
        state.current_engine().shutdown();
        state::session_guard::end(&state.data_dir());
    }
    logging::info("Stellars quitting");
    app.exit(0);
}

/// Applies "always on top" and "start minimized to tray" before the user sees the window.
fn apply_startup_window_settings(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let settings = state.app_settings();
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_always_on_top(settings.always_on_top);
        if settings.start_minimized {
            let _ = window.hide();
            state.meter_pacing.set_window_hidden(true);
        }
    }
}

/// Honours "close button quits" and "minimize to tray".
fn handle_close_and_minimize(window: &tauri::Window, event: &tauri::WindowEvent) {
    let Some(state) = window.app_handle().try_state::<AppState>() else {
        return;
    };
    let settings = state.app_settings();
    if let tauri::WindowEvent::Resized(_) = event {
        let hidden = window.is_minimized().unwrap_or(false) || !window.is_visible().unwrap_or(true);
        state.meter_pacing.set_window_hidden(hidden);
    }
    match event {
        // Only a visible window can be focused, so this always clears a stuck "hidden" state.
        tauri::WindowEvent::Focused(true) => state.meter_pacing.set_window_hidden(false),
        tauri::WindowEvent::CloseRequested { .. }
            if settings.close_behavior == CloseBehavior::Quit =>
        {
            quit_application(window.app_handle());
        }
        tauri::WindowEvent::Resized(_)
            if settings.minimize_to_tray && window.is_minimized().unwrap_or(false) =>
        {
            let _ = window.hide();
            state.meter_pacing.set_window_hidden(true);
        }
        _ => {}
    }
}

/// `stellars.exe --setup-cables`, run by the installer: sets up the virtual audio cables and exits
/// without opening a window or starting the audio engine.
fn run_cable_setup_and_exit() {
    if let Some(dir) = RouterConfig::default_config_path().parent() {
        logging::init(dir);
    }
    logging::info("Installer: setting up virtual audio cables");
    let (actions, reboot_recommended) = interop::cables::run_installer_setup();
    for line in actions {
        logging::info(&format!("Installer: {line}"));
    }
    if reboot_recommended {
        logging::info("Installer: Windows needs a restart before the virtual cables appear");
        std::process::exit(interop::cables::SETUP_REBOOT_EXIT_CODE);
    }
}

/// `stellars.exe --uninstall-cables`, run by the uninstaller: removes the cables Stellars installed
/// and restores the Windows audio routing, then exits.
fn run_cable_uninstall_and_exit() {
    if let Some(dir) = RouterConfig::default_config_path().parent() {
        logging::init(dir);
    }
    logging::info("Uninstaller: restoring Windows audio");
    let (actions, reboot_recommended) = interop::uninstall::run_uninstaller_cleanup();
    for line in actions {
        logging::info(&format!("Uninstaller: {line}"));
    }
    if reboot_recommended {
        std::process::exit(interop::uninstall::UNINSTALL_REBOOT_EXIT_CODE);
    }
}

fn main() {
    if interop::uninstall::is_uninstall_invocation(std::env::args()) {
        run_cable_uninstall_and_exit();
        return;
    }
    if interop::cables::is_setup_invocation(std::env::args()) {
        run_cable_setup_and_exit();
        return;
    }
    let app = tauri::Builder::default()
        .setup(|app| {
            let config_path = RouterConfig::default_config_path();
            if let Some(dir) = config_path.parent() {
                logging::init(dir);
            }
            logging::install_panic_hook();
            logging::info(&format!("Stellars {} starting", env!("CARGO_PKG_VERSION")));
            // A leftover marker means the last session crashed: apps may still be routed to the
            // silent sink in Windows, so send them back to their normal device first.
            let crashed = config_path.parent().is_some_and(state::session_guard::begin);
            if crashed {
                logging::warn("The previous session did not end cleanly; resetting per-app audio routing");
                let _ = interop::policy::clear_all_persisted_endpoints();
            }
            let app_state = AppState::new(config_path);
            app_state.recovered_from_crash.store(crashed, Ordering::Relaxed);
            app.manage(app_state);
            tray::setup(app.handle())?;
            apply_startup_window_settings(app.handle());
            run_auto_backup(app.handle());
            spawn_device_watcher(app.handle().clone());
            spawn_scene_watcher(app.handle().clone());
            update::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // The tray popup is a separate, short-lived window: it must not trigger the main
            // window's close, minimize or meter-stream handling.
            if window.label() == tray::LABEL {
                tray::handle_window_event(window, event);
                return;
            }
            handle_close_and_minimize(window, event);
            // Section 0 & Phase 4.3:
            // When the user closes the window, we stop the 60Hz meter channel and let Tauri
            // destroy the WebviewWindow (terminating msedgewebview2.exe so background RAM < 15 MB).
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    if let Ok(mut cancel_guard) = state.meter_stream_cancel.lock() {
                        if let Some(flag) = cancel_guard.take() {
                            flag.store(true, Ordering::Release);
                        }
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_router_state,
            commands::refresh_audio_processes,
            commands::set_chat_mix,
            commands::set_master_volume,
            commands::set_stream_config,
            commands::set_stream_channel,
            commands::set_bus_volume_mute,
            commands::set_bus_audition,
            commands::set_eq_band,
            commands::get_eq_curve,
            commands::set_bus_dynamics,
            commands::set_mic_to_monitor,
            commands::route_process_to_bus,
            commands::unroute_process,
            commands::reset_all_endpoints,
            commands::set_audio_devices,
            commands::get_virtual_cable_status,
            commands::get_nvidia_recording_status,
            commands::set_nvidia_recording_mic,
            commands::auto_route_virtual_cables,
            commands::install_or_repair_virtual_cables,
            commands::subscribe_peak_meters,
            commands::set_meter_rate,
            commands::window_minimize,
            commands::window_toggle_maximize,
            commands::window_close,
            commands::window_is_maximized,
            commands::get_app_settings,
            commands::set_app_settings,
            commands::get_app_info,
            commands::open_data_folder,
            commands::open_external,
            commands::quit_application,
            commands::get_diagnostics,
            commands::get_log_tail,
            commands::get_update_status,
            commands::check_for_updates,
            commands::download_update,
            commands::install_update,
            commands::get_setup_checks,
            commands::restore_windows_audio,
            commands::resume_audio_engine,
            commands::export_support_file,
            commands::export_presets,
            commands::export_presets_file,
            commands::import_presets,
            commands::open_logs_folder,
            commands::create_backup,
            commands::list_backups,
            commands::delete_backup,
            commands::open_backups_folder,
            commands::restore_backup,
            commands::set_bus_extras,
            commands::set_ducking,
            commands::set_process_volume,
            commands::get_presets,
            commands::save_user_preset,
            commands::delete_user_preset,
            commands::save_scene,
            commands::update_scene,
            commands::delete_scene,
            commands::apply_scene,
            commands::save_output_profile,
            commands::delete_output_profile,
            commands::apply_output_profile,
            tray::tray_menu_state,
            tray::tray_menu_action,
            tray::tray_menu_hide,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Stellars Sonar Tauri application");

    app.run(|_app_handle, event| {
        // Keep the Rust WASAPI engine alive in the System Tray when the WebView2 window is destroyed
        if let RunEvent::ExitRequested { api, .. } = event {
            if !QUIT_REQUESTED.load(Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
    });
}
