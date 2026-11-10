//! Configuration backups: one JSON bundle (router config + app settings) per file in
//! `<data dir>/backups`. File names are generated here and validated on every read/delete so the
//! frontend can never address a path outside that folder.

use super::app_settings::AppSettings;
use super::config::RouterConfig;
use super::logging::{compact_timestamp, unix_now_secs};
use super::presets::PresetStore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const FORMAT_VERSION: u32 = 1;
const NAME_PREFIX: &str = "stellars-";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupKind {
    Manual,
    Auto,
    PreRestore,
}

impl BackupKind {
    fn slug(self) -> &'static str {
        match self {
            BackupKind::Manual => "manual",
            BackupKind::Auto => "auto",
            BackupKind::PreRestore => "prerestore",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupBundle {
    pub format_version: u32,
    pub created_at: String,
    pub app_version: String,
    pub config: RouterConfig,
    pub app_settings: AppSettings,
    /// Scenes and output profiles; absent in backups made before presets existed.
    #[serde(default)]
    pub presets: PresetStore,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub name: String,
    pub kind: String,
    pub created_at: String,
    pub size_bytes: u64,
}

pub fn backups_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("backups")
}

/// Accepts only names this module produces: `stellars-YYYYMMDD-HHMMSS-<kind>[-n].json`.
fn validate_name(name: &str) -> Result<(), String> {
    let ok = name.starts_with(NAME_PREFIX)
        && name.ends_with(".json")
        && name.len() < 80
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    if ok && !name.contains("..") {
        Ok(())
    } else {
        Err("Invalid backup name".to_string())
    }
}

fn kind_of(name: &str) -> &'static str {
    if name.contains("-prerestore") {
        "prerestore"
    } else if name.contains("-auto") {
        "auto"
    } else {
        "manual"
    }
}

/// `stellars-20261109-123456-manual.json` -> `2026-11-09 12:34:56`
fn created_at_of(name: &str) -> String {
    let stamp = name.trim_start_matches(NAME_PREFIX);
    let digits: Vec<char> = stamp.chars().take(15).collect();
    if digits.len() == 15 && digits[8] == '-' {
        let s: String = digits.iter().collect();
        format!(
            "{}-{}-{} {}:{}:{}",
            &s[0..4],
            &s[4..6],
            &s[6..8],
            &s[9..11],
            &s[11..13],
            &s[13..15]
        )
    } else {
        String::new()
    }
}

fn info_for(path: &Path, name: &str) -> BackupInfo {
    BackupInfo {
        name: name.to_string(),
        kind: kind_of(name).to_string(),
        created_at: created_at_of(name),
        size_bytes: fs::metadata(path).map(|m| m.len()).unwrap_or(0),
    }
}

pub fn create(
    dir: &Path,
    kind: BackupKind,
    config: &RouterConfig,
    settings: &AppSettings,
    presets: &PresetStore,
) -> Result<BackupInfo, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let now = unix_now_secs();
    let bundle = BackupBundle {
        format_version: FORMAT_VERSION,
        created_at: super::logging::timestamp(now),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        config: config.clone(),
        app_settings: settings.clone(),
        presets: presets.clone(),
    };
    let json = serde_json::to_string_pretty(&bundle).map_err(|e| e.to_string())?;

    let stem = format!("{NAME_PREFIX}{}-{}", compact_timestamp(now), kind.slug());
    let mut name = format!("{stem}.json");
    let mut counter = 2;
    while dir.join(&name).exists() {
        name = format!("{stem}-{counter}.json");
        counter += 1;
    }
    let path = dir.join(&name);
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(info_for(&path, &name))
}

/// Newest first.
pub fn list(dir: &Path) -> Vec<BackupInfo> {
    let mut items: Vec<BackupInfo> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            validate_name(&name).ok()?;
            Some(info_for(&entry.path(), &name))
        })
        .collect();
    items.sort_by(|a, b| b.name.cmp(&a.name));
    items
}

pub fn load(dir: &Path, name: &str) -> Result<BackupBundle, String> {
    validate_name(name)?;
    let raw = fs::read_to_string(dir.join(name)).map_err(|e| format!("Could not read backup: {e}"))?;
    let bundle: BackupBundle =
        serde_json::from_str(&raw).map_err(|e| format!("Backup file is not valid: {e}"))?;
    if bundle.format_version > FORMAT_VERSION {
        return Err("This backup was made by a newer version of Stellars".to_string());
    }
    Ok(bundle)
}

pub fn delete(dir: &Path, name: &str) -> Result<(), String> {
    validate_name(name)?;
    fs::remove_file(dir.join(name)).map_err(|e| e.to_string())
}

/// Keeps only the newest `keep` backups of `kind` ("auto" or "prerestore"). Manual backups are
/// never pruned.
pub fn prune_kind(dir: &Path, kind: &str, keep: usize) {
    if kind == "manual" {
        return;
    }
    for stale in list(dir).into_iter().filter(|b| b.kind == kind).skip(keep) {
        let _ = delete(dir, &stale.name);
    }
}

/// True when there is no automatic backup newer than `min_age_secs`.
pub fn auto_backup_due(dir: &Path, min_age_secs: u64) -> bool {
    let newest = list(dir).into_iter().find(|b| b.kind == "auto");
    let Some(newest) = newest else {
        return true;
    };
    let modified = fs::metadata(dir.join(&newest.name))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    unix_now_secs().saturating_sub(modified) >= min_age_secs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stellars-backup-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn rejects_traversal_and_foreign_names() {
        for bad in ["../x.json", "stellars-..\\a.json", "config.json", "stellars-1/2.json", "stellars-a.txt"] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
        assert!(validate_name("stellars-20261109-123456-manual.json").is_ok());
    }

    #[test]
    fn create_list_load_roundtrip() {
        let dir = temp_dir("roundtrip");
        let info = create(
            &dir,
            BackupKind::Manual,
            &RouterConfig::default(),
            &AppSettings::default(),
            &PresetStore::default(),
        )
        .unwrap();
        assert_eq!(info.kind, "manual");
        assert_eq!(list(&dir).len(), 1);
        let bundle = load(&dir, &info.name).unwrap();
        assert_eq!(bundle.format_version, FORMAT_VERSION);
        delete(&dir, &info.name).unwrap();
        assert!(list(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_keeps_newest_auto_backups_only() {
        let dir = temp_dir("prune");
        fs::create_dir_all(&dir).unwrap();
        for i in 1..=4 {
            fs::write(dir.join(format!("stellars-2026110{i}-000000-auto.json")), "{}").unwrap();
        }
        fs::write(dir.join("stellars-20260101-000000-manual.json"), "{}").unwrap();
        prune_kind(&dir, "auto", 2);
        prune_kind(&dir, "manual", 0);
        let names: Vec<String> = list(&dir).into_iter().map(|b| b.name).collect();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"stellars-20260101-000000-manual.json".to_string()));
        assert!(!names.contains(&"stellars-20261101-000000-auto.json".to_string()));
        let _ = fs::remove_dir_all(&dir);
    }
}
