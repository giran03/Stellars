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

    /// Everything worth sharing: sound presets, scenes and output profiles.
    pub fn to_share_file(&self) -> ShareFile {
        ShareFile {
            format: SHARE_FORMAT.to_string(),
            version: SHARE_VERSION,
            user_presets: self.user_presets.clone(),
            scenes: self.scenes.clone(),
            output_profiles: self.output_profiles.clone(),
        }
    }

    /// Adds the contents of a shared file. Imported scenes and profiles always get new ids and
    /// unique names; devices from another PC are dropped, and values are brought into range.
    pub fn merge_share_file(&mut self, file: ShareFile) -> Result<ImportSummary, String> {
        let total = file.user_presets.len() + file.scenes.len() + file.output_profiles.len();
        if total == 0 {
            return Err("That file has no presets, scenes or output profiles".to_string());
        }
        if total > MAX_IMPORTED_ITEMS {
            return Err(format!("That file has too many items (the limit is {MAX_IMPORTED_ITEMS})"));
        }
        let mut summary = ImportSummary::default();
        for preset in file.user_presets {
            if self.upsert_user_preset(&preset.kind, preset.bus, &preset.name, preset.data).is_ok() {
                summary.user_presets += 1;
            }
        }
        for scene in file.scenes {
            let Ok(name) = clean_name(&scene.name) else { continue };
            let mut cfg = RouterConfig::default();
            scene.snapshot.apply_to(&mut cfg);
            let cfg = cfg.normalized();
            let name = unique_name(&name, self.scenes.iter().map(|s| s.name.as_str()));
            self.scenes.push(Scene {
                id: new_id("s"),
                name,
                auto_apps: clean_apps(scene.auto_apps),
                snapshot: SceneSnapshot::capture(&cfg),
            });
            summary.scenes += 1;
        }
        for profile in file.output_profiles {
            let Ok(name) = clean_name(&profile.name) else { continue };
            let name = unique_name(&name, self.output_profiles.iter().map(|p| p.name.as_str()));
            let bus_eq = profile
                .bus_eq
                .into_iter()
                .map(|(bus, bands)| {
                    let bands = bands.into_iter().take(MAX_IMPORTED_EQ_BANDS).map(|b| b.clamped()).collect();
                    (bus, bands)
                })
                .collect();
            self.output_profiles.push(OutputProfile {
                id: new_id("p"),
                name,
                master_output_device_id: None,
                master_volume: profile.master_volume.clamp(0.0, 1.0),
                bus_eq,
            });
            summary.output_profiles += 1;
        }
        Ok(summary)
    }

    /// First scene (in list order) that lists `exe` (lower-case) as an auto-apply app.
    pub fn scene_for_app(&self, exe: &str) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.auto_apps.iter().any(|a| a == exe))
    }
}

pub const SHARE_FORMAT: &str = "stellars-presets";
const SHARE_VERSION: u32 = 1;
pub const MAX_IMPORT_BYTES: usize = 2 * 1024 * 1024;
const MAX_IMPORTED_ITEMS: usize = 100;
const MAX_IMPORTED_EQ_BANDS: usize = 32;

/// The file written by "Export" and read by "Import".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareFile {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub user_presets: Vec<UserPreset>,
    #[serde(default)]
    pub scenes: Vec<Scene>,
    #[serde(default)]
    pub output_profiles: Vec<OutputProfile>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub user_presets: usize,
    pub scenes: usize,
    pub output_profiles: usize,
}

/// `name`, or `name (imported)` / `name (imported 2)` … when `existing` already has it.
fn unique_name<'a>(name: &str, existing: impl Iterator<Item = &'a str> + Clone) -> String {
    let taken = |candidate: &str| existing.clone().any(|e| e.eq_ignore_ascii_case(candidate));
    if !taken(name) {
        return name.to_string();
    }
    let mut n = 1;
    loop {
        let suffix = if n == 1 { " (imported)".to_string() } else { format!(" (imported {n})") };
        let base: String = name.chars().take(40usize.saturating_sub(suffix.chars().count())).collect();
        let candidate = format!("{base}{suffix}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Parses a shared file, rejecting anything that is not a Stellars export or is too large.
pub fn parse_share_file(json: &str) -> Result<ShareFile, String> {
    if json.len() > MAX_IMPORT_BYTES {
        return Err("That file is too large to import".to_string());
    }
    let file: ShareFile = serde_json::from_str(json).map_err(|_| "That is not a Stellars presets file".to_string())?;
    if file.format != SHARE_FORMAT {
        return Err("That is not a Stellars presets file".to_string());
    }
    if file.version > SHARE_VERSION {
        return Err("That file was made by a newer Stellars. Update the app to import it".to_string());
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_roundtrip_gives_new_ids_unique_names_and_drops_devices() {
        let cfg = RouterConfig::default();
        let mut source = PresetStore::default();
        source.scenes.push(Scene {
            id: "s-old".into(),
            name: "Gaming".into(),
            auto_apps: vec!["CS2.exe".into()],
            snapshot: SceneSnapshot::capture(&cfg),
        });
        let mut profile_cfg = RouterConfig::default();
        profile_cfg.master_output_device_id = Some("pc-a-headphones".into());
        source.output_profiles.push(OutputProfile::capture("p-old".into(), "Headphones".into(), &profile_cfg));
        source
            .upsert_user_preset("gate", BusId::Mic, "Quiet room", serde_json::json!({"thresholdDb": -50}))
            .unwrap();

        let json = serde_json::to_string(&source.to_share_file()).unwrap();
        let mut target = PresetStore::default();
        target.scenes.push(Scene {
            id: "s-mine".into(),
            name: "gaming".into(),
            auto_apps: vec![],
            snapshot: SceneSnapshot::capture(&cfg),
        });
        let summary = target.merge_share_file(parse_share_file(&json).unwrap()).unwrap();

        assert_eq!(summary, ImportSummary { user_presets: 1, scenes: 1, output_profiles: 1 });
        assert_eq!(target.scenes.len(), 2);
        assert_eq!(target.scenes[1].name, "Gaming (imported)");
        assert_ne!(target.scenes[1].id, "s-old");
        assert_eq!(target.scenes[1].auto_apps, vec!["cs2.exe".to_string()]);
        assert_eq!(target.output_profiles[0].master_output_device_id, None);
        assert_eq!(target.user_presets.len(), 1);
    }

    #[test]
    fn import_rejects_foreign_empty_oversized_and_newer_files() {
        assert!(parse_share_file("not json").is_err());
        assert!(parse_share_file(r#"{"format":"other","version":1}"#).is_err());
        assert!(parse_share_file(r#"{"format":"stellars-presets","version":99}"#).is_err());
        assert!(parse_share_file(&" ".repeat(MAX_IMPORT_BYTES + 1)).is_err());
        let empty = parse_share_file(r#"{"format":"stellars-presets","version":1}"#).unwrap();
        assert!(PresetStore::default().merge_share_file(empty).is_err());
    }

    #[test]
    fn imported_names_stay_unique_and_short() {
        let existing = ["Mix", "Mix (imported)"];
        assert_eq!(unique_name("Mix", existing.iter().copied()), "Mix (imported 2)");
        let long = "x".repeat(40);
        let taken = [long.as_str()];
        assert!(unique_name(&long, taken.iter().copied()).chars().count() <= 40);
    }

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
