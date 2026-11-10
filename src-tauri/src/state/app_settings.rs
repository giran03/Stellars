use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Behaviour of the window close (X) button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseBehavior {
    /// Close the window but keep the audio engine running in the system tray.
    Tray,
    /// Quit Stellars completely (engine included).
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    Stable,
    Beta,
}

/// How much of the interface is shown. The mode only hides or reveals controls; it never
/// changes audio values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InterfaceMode {
    Simple,
    Advanced,
}

/// Settings files written before the modes existed belong to people already using the full
/// interface, so a missing field means "advanced" (new installs use `Default`, i.e. simple).
fn existing_install_mode() -> InterfaceMode {
    InterfaceMode::Advanced
}

fn existing_install_chosen() -> bool {
    true
}

/// Application-level (non-audio) preferences, persisted next to `config.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub launch_on_startup: bool,
    pub start_minimized: bool,
    pub close_behavior: CloseBehavior,
    pub minimize_to_tray: bool,
    pub always_on_top: bool,
    pub confirm_before_reset: bool,
    pub show_notifications: bool,
    pub show_tooltips: bool,
    pub reduce_motion: bool,
    pub compact_mode: bool,
    pub auto_reconnect_devices: bool,
    /// Slow the live meters when the window is unfocused and stop them while it is hidden.
    pub save_resources_in_background: bool,
    pub onboarding_completed: bool,
    pub auto_check_updates: bool,
    pub update_channel: UpdateChannel,
    pub last_update_check: Option<String>,
    #[serde(default = "existing_install_mode")]
    pub interface_mode: InterfaceMode,
    /// False until the user picks a mode (wizard or the side prompt); existing installs count as chosen.
    #[serde(default = "existing_install_chosen")]
    pub interface_mode_chosen: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            launch_on_startup: false,
            start_minimized: false,
            close_behavior: CloseBehavior::Tray,
            minimize_to_tray: false,
            always_on_top: false,
            confirm_before_reset: true,
            show_notifications: true,
            show_tooltips: true,
            reduce_motion: false,
            compact_mode: false,
            auto_reconnect_devices: true,
            save_resources_in_background: true,
            onboarding_completed: false,
            auto_check_updates: true,
            update_channel: UpdateChannel::Stable,
            last_update_check: None,
            interface_mode: InterfaceMode::Simple,
            interface_mode_chosen: false,
        }
    }
}

impl AppSettings {
    pub fn default_path() -> PathBuf {
        match std::env::var("APPDATA") {
            Ok(appdata) => PathBuf::from(appdata)
                .join("StellarsSonar")
                .join("app_settings.json"),
            Err(_) => PathBuf::from("app_settings.json"),
        }
    }

    pub fn load_from_path(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<AppSettings>(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let parsed: AppSettings = serde_json::from_str(r#"{"startMinimized":true}"#).unwrap();
        assert!(parsed.start_minimized);
        assert_eq!(parsed.close_behavior, CloseBehavior::Tray);
        assert!(parsed.show_tooltips);
    }

    #[test]
    fn existing_settings_files_keep_the_advanced_interface() {
        let parsed: AppSettings = serde_json::from_str(r#"{"onboardingCompleted":true}"#).unwrap();
        assert_eq!(parsed.interface_mode, InterfaceMode::Advanced);
        assert!(parsed.interface_mode_chosen);
    }

    #[test]
    fn new_installs_start_simple_and_unchosen() {
        let fresh = AppSettings::default();
        assert_eq!(fresh.interface_mode, InterfaceMode::Simple);
        assert!(!fresh.interface_mode_chosen);
    }

    #[test]
    fn roundtrip_preserves_values() {
        let mut s = AppSettings::default();
        s.close_behavior = CloseBehavior::Quit;
        s.update_channel = UpdateChannel::Beta;
        let restored: AppSettings =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(restored, s);
    }
}
