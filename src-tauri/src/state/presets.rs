//! Scenes (whole-mixer snapshots, optionally auto-applied when an app gets focus) and output
//! profiles (monitor device + volume + per-bus EQ, for switching headphones <-> speakers).
//! Stored in `presets.json` next to `config.json`.

use super::config::{
    BusConfig, BusId, DuckingConfig, EqBandConfig, RouterConfig, StreamConfig,
};
use super::logging::unix_now_secs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Everything a scene restores. Device selections are deliberately excluded: scenes change how
/// audio is mixed, never where it is played, so they can be applied live without a restart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneSnapshot {
    pub buses: BTreeMap<BusId, BusConfig>,
    pub stream: StreamConfig,
    pub chat_mix_balance: f32,
    pub master_volume: f32,
    pub mic_to_monitor: bool,
    pub process_routing_rules: BTreeMap<String, BusId>,
    #[serde(default)]
    pub process_volumes: BTreeMap<String, f32>,
    #[serde(default)]
    pub ducking: DuckingConfig,
}

impl SceneSnapshot {
    pub fn capture(cfg: &RouterConfig) -> Self {
        Self {
            buses: cfg.buses.clone(),
            stream: cfg.stream.clone(),
            chat_mix_balance: cfg.chat_mix_balance,
            master_volume: cfg.master_volume,
            mic_to_monitor: cfg.mic_to_monitor,
            process_routing_rules: cfg.process_routing_rules.clone(),
            process_volumes: cfg.process_volumes.clone(),
            ducking: cfg.ducking.clone(),
        }
    }

    /// Writes the snapshot into `cfg`, keeping every device selection (including the stream
    /// output) as it is.
    pub fn apply_to(&self, cfg: &mut RouterConfig) {
        cfg.buses = self.buses.clone();
        let stream_output = cfg.stream.output_device_id.clone();
        cfg.stream = self.stream.clone();
        cfg.stream.output_device_id = stream_output;
        if cfg.stream.output_device_id.is_none() {
            cfg.stream.enabled = false;
        }
        cfg.chat_mix_balance = self.chat_mix_balance;
        cfg.master_volume = self.master_volume;
        cfg.mic_to_monitor = self.mic_to_monitor;
        cfg.process_routing_rules = self.process_routing_rules.clone();
        cfg.process_volumes = self.process_volumes.clone();
        cfg.ducking = self.ducking.clone();
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub id: String,
    pub name: String,
    /// Executable names (e.g. `cs2.exe`); the scene is applied when one of them gets focus.
    #[serde(default)]
    pub auto_apps: Vec<String>,
    pub snapshot: SceneSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputProfile {
    pub id: String,
    pub name: String,
    pub master_output_device_id: Option<String>,
    pub master_volume: f32,
    pub bus_eq: BTreeMap<BusId, Vec<EqBandConfig>>,
}

impl OutputProfile {
    pub fn capture(id: String, name: String, cfg: &RouterConfig) -> Self {
        Self {
            id,
            name,
            master_output_device_id: cfg.master_output_device_id.clone(),
            master_volume: cfg.master_volume,
            bus_eq: cfg.buses.iter().map(|(b, c)| (*b, c.eq_bands.clone())).collect(),
        }
    }

    pub fn apply_to(&self, cfg: &mut RouterConfig) {
        cfg.master_output_device_id = self.master_output_device_id.clone();
        cfg.master_volume = self.master_volume;
        for (bus, bands) in &self.bus_eq {
            if let Some(bus_cfg) = cfg.buses.get_mut(bus) {
                bus_cfg.eq_bands = bands.clone();
            }
        }
    }
}

/// Kinds of sound preset a user can save (EQ curves, Fine-tune sections and whole-channel recipes).
pub const USER_PRESET_KINDS: [&str; 7] = ["eq", "noise", "gate", "comp", "deEsser", "limiter", "recipe"];
const MAX_USER_PRESETS_PER_GROUP: usize = 30;
const MAX_USER_PRESET_BYTES: usize = 16 * 1024;

/// A sound preset saved by the user. `data` is opaque to the engine: the interface decides its
/// shape per `kind` and re-applies it through the normal commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPreset {
    pub id: String,
    pub kind: String,
    pub bus: BusId,
    pub name: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PresetStore {
    pub user_presets: Vec<UserPreset>,
    pub scenes: Vec<Scene>,
    pub output_profiles: Vec<OutputProfile>,
    pub active_scene_id: Option<String>,
    pub active_profile_id: Option<String>,
}

pub fn presets_path(data_dir: &Path) -> PathBuf {
    data_dir.join("presets.json")
}

/// Unique, file-name-safe id such as `s-1794227696-3`.
pub fn new_id(prefix: &str) -> String {
    let n = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{}-{n}", unix_now_secs())
}

/// Trims, collapses whitespace and limits length so names stay tidy in menus.
pub fn clean_name(raw: &str) -> Result<String, String> {
    let name: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let name: String = name.chars().take(40).collect();
    if name.is_empty() {
        Err("Give it a name first".to_string())
    } else {
        Ok(name)
    }
}

/// Lower-cases and de-duplicates executable names, dropping blanks and path separators.
pub fn clean_apps(apps: Vec<String>) -> Vec<String> {
    let mut cleaned: Vec<String> = apps
        .into_iter()
        .map(|a| a.trim().to_ascii_lowercase())
        .filter(|a| !a.is_empty() && !a.contains(['\\', '/']))
        .collect();
    cleaned.sort();
    cleaned.dedup();
    cleaned
}

impl PresetStore {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Adds a preset (or replaces the same-named one for that kind and channel).
    pub fn upsert_user_preset(
        &mut self,
        kind: &str,
        bus: BusId,
        name: &str,
        data: serde_json::Value,
    ) -> Result<UserPreset, String> {
        if !USER_PRESET_KINDS.contains(&kind) {
            return Err("Unknown preset type".to_string());
        }
        let name = clean_name(name)?;
        let size = serde_json::to_string(&data).map(|s| s.len()).unwrap_or(usize::MAX);
        if size > MAX_USER_PRESET_BYTES {
            return Err("That preset is too large to store".to_string());
        }
        let existing = self.user_presets.iter().position(|p| {
            p.kind == kind && p.bus == bus && p.name.eq_ignore_ascii_case(&name)
        });
        if existing.is_none() {
            let in_group = self.user_presets.iter().filter(|p| p.kind == kind && p.bus == bus).count();
            if in_group >= MAX_USER_PRESETS_PER_GROUP {
                return Err(format!("You can keep up to {MAX_USER_PRESETS_PER_GROUP} saved presets here"));
            }
        }
        let preset = UserPreset {
            id: existing.map(|i| self.user_presets[i].id.clone()).unwrap_or_else(|| new_id("u")),
            kind: kind.to_string(),
            bus,
            name,
            data,
        };
        match existing {
            Some(i) => self.user_presets[i] = preset.clone(),
            None => self.user_presets.push(preset.clone()),
        }
        Ok(preset)
    }

    pub fn scene(&self, id: &str) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.id == id)
    }

    /// First scene (in list order) that lists `exe` (lower-case) as an auto-apply app.
    pub fn scene_for_app(&self, exe: &str) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.auto_apps.iter().any(|a| a == exe))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_roundtrip_keeps_devices_untouched() {
        let mut source = RouterConfig::default();
        source.master_volume = 0.5;
        source.stream.output_device_id = Some("scene-stream".into());
        source.buses.get_mut(&BusId::Game).unwrap().volume = 0.3;
        let snapshot = SceneSnapshot::capture(&source);

        let mut target = RouterConfig::default();
        target.master_output_device_id = Some("headphones".into());
        target.stream.output_device_id = Some("my-stream-cable".into());
        snapshot.apply_to(&mut target);

        assert_eq!(target.master_volume, 0.5);
        assert_eq!(target.buses[&BusId::Game].volume, 0.3);
        assert_eq!(target.master_output_device_id.as_deref(), Some("headphones"));
        assert_eq!(target.stream.output_device_id.as_deref(), Some("my-stream-cable"));
    }

    #[test]
    fn output_profile_restores_device_volume_and_eq() {
        let mut cfg = RouterConfig::default();
        cfg.master_output_device_id = Some("speakers".into());
        cfg.master_volume = 0.7;
        cfg.buses.get_mut(&BusId::Media).unwrap().eq_bands[3].gain_db = 4.0;
        let profile = OutputProfile::capture("p-1".into(), "Speakers".into(), &cfg);

        let mut other = RouterConfig::default();
        profile.apply_to(&mut other);
        assert_eq!(other.master_output_device_id.as_deref(), Some("speakers"));
        assert_eq!(other.master_volume, 0.7);
        assert_eq!(other.buses[&BusId::Media].eq_bands[3].gain_db, 4.0);
    }

    #[test]
    fn user_presets_upsert_validate_and_limit() {
        let mut store = PresetStore::default();
        let a = store
            .upsert_user_preset("gate", BusId::Mic, "  My   gate ", serde_json::json!({"thresholdDb": -40}))
            .unwrap();
        let again = store
            .upsert_user_preset("gate", BusId::Mic, "my gate", serde_json::json!({"thresholdDb": -35}))
            .unwrap();
        assert_eq!(a.id, again.id, "same name replaces instead of duplicating");
        assert_eq!(store.user_presets.len(), 1);
        assert_eq!(store.user_presets[0].data["thresholdDb"], -35);
        // Same name on another channel is a separate preset.
        store.upsert_user_preset("gate", BusId::Chat, "My gate", serde_json::json!({})).unwrap();
        assert_eq!(store.user_presets.len(), 2);
        assert!(store.upsert_user_preset("bogus", BusId::Mic, "x", serde_json::json!({})).is_err());
        assert!(store.upsert_user_preset("gate", BusId::Mic, "  ", serde_json::json!({})).is_err());
        let huge = serde_json::json!({ "blob": "x".repeat(20_000) });
        assert!(store.upsert_user_preset("recipe", BusId::Mic, "big", huge).is_err());
        for i in 0..40 {
            let _ = store.upsert_user_preset("comp", BusId::Game, &format!("p{i}"), serde_json::json!({}));
        }
        let comp_count = store.user_presets.iter().filter(|p| p.kind == "comp").count();
        assert_eq!(comp_count, MAX_USER_PRESETS_PER_GROUP);
    }

    #[test]
    fn old_preset_files_without_user_presets_still_load() {
        let store: PresetStore = serde_json::from_str(r#"{"scenes":[],"outputProfiles":[]}"#).unwrap();
        assert!(store.user_presets.is_empty());
    }

    #[test]
    fn names_and_apps_are_cleaned() {
        assert_eq!(clean_name("  Late   night  ").unwrap(), "Late night");
        assert!(clean_name("   ").is_err());
        assert_eq!(
            clean_apps(vec![" CS2.exe ".into(), "cs2.exe".into(), "".into(), "C:\\x.exe".into()]),
            vec!["cs2.exe".to_string()]
        );
    }

    #[test]
    fn ids_are_unique_and_scene_lookup_by_app_works() {
        assert_ne!(new_id("s"), new_id("s"));
        let cfg = RouterConfig::default();
        let store = PresetStore {
            scenes: vec![Scene {
                id: "s-1".into(),
                name: "Gaming".into(),
                auto_apps: vec!["cs2.exe".into()],
                snapshot: SceneSnapshot::capture(&cfg),
            }],
            ..Default::default()
        };
        assert_eq!(store.scene_for_app("cs2.exe").map(|s| s.id.as_str()), Some("s-1"));
        assert!(store.scene_for_app("notepad.exe").is_none());
    }
}
