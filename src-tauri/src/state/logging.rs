//! Minimal rotating file logger (`logs/stellars.log`, `.1.log`, `.2.log`) plus a panic hook,
//! so users can attach a log when reporting a problem. No external dependencies.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: u64 = 512 * 1024;
const KEPT_ROTATIONS: usize = 2;

static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Days since 1970-01-01 -> (year, month, day) in the proleptic Gregorian calendar (UTC).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn broken_down(unix_secs: u64) -> (i64, u32, u32, u64, u64, u64) {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    (y, m, d, rem / 3_600, (rem % 3_600) / 60, rem % 60)
}

pub fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `2026-11-09 12:00:00` (UTC)
pub fn timestamp(unix_secs: u64) -> String {
    let (y, mo, d, h, mi, s) = broken_down(unix_secs);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

/// `20261109-120000` (UTC), safe for file names.
pub fn compact_timestamp(unix_secs: u64) -> String {
    let (y, mo, d, h, mi, s) = broken_down(unix_secs);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

pub fn init(data_dir: &Path) {
    let dir = data_dir.join("logs");
    let _ = fs::create_dir_all(&dir);
    let _ = LOG_DIR.set(dir);
}

pub fn log_dir() -> Option<&'static Path> {
    LOG_DIR.get().map(|p| p.as_path())
}

pub fn log_file_path() -> Option<PathBuf> {
    log_dir().map(|d| d.join("stellars.log"))
}

fn rotated_path(dir: &Path, index: usize) -> PathBuf {
    dir.join(format!("stellars.{index}.log"))
}

fn rotate_if_needed(dir: &Path, current: &Path) {
    let too_big = fs::metadata(current)
        .map(|m| m.len() >= MAX_LOG_BYTES)
        .unwrap_or(false);
    if !too_big {
        return;
    }
    let _ = fs::remove_file(rotated_path(dir, KEPT_ROTATIONS));
    for i in (1..KEPT_ROTATIONS).rev() {
        let _ = fs::rename(rotated_path(dir, i), rotated_path(dir, i + 1));
    }
    let _ = fs::rename(current, rotated_path(dir, 1));
}

pub fn log(level: &str, message: &str) {
    let Some(dir) = log_dir() else {
        return;
    };
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let path = dir.join("stellars.log");
    rotate_if_needed(dir, &path);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let line = format!(
            "{} {:<5} {}\n",
            timestamp(unix_now_secs()),
            level,
            message.replace('\n', " | ")
        );
        let _ = file.write_all(line.as_bytes());
    }
}

pub fn info(message: &str) {
    log("INFO", message);
}

pub fn warn(message: &str) {
    log("WARN", message);
}

pub fn error(message: &str) {
    log("ERROR", message);
}

/// Returns the last `max_lines` log lines (newest last).
pub fn tail(max_lines: usize) -> Vec<String> {
    let Some(path) = log_file_path() else {
        return Vec::new();
    };
    let raw = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = raw.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].iter().map(|l| l.to_string()).collect()
}

/// Writes panics (with location) to the log before the default handler runs.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".into());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".into());
        error(&format!("PANIC at {location}: {payload}"));
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_timestamps() {
        assert_eq!(timestamp(0), "1970-01-01 00:00:00");
        assert_eq!(timestamp(1_794_227_696), "2026-11-09 12:34:56");
        assert_eq!(compact_timestamp(1_794_227_696), "20261109-123456");
    }

    #[test]
    fn handles_leap_day() {
        assert_eq!(timestamp(1_709_164_800), "2024-02-29 00:00:00");
    }
}
