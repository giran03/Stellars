//! Detects that Stellars did not shut down cleanly. A marker file exists while the app runs and is
//! removed on a normal quit; finding it at startup means the last session crashed or was killed,
//! so apps may still be routed to the silent sink in Windows.

use std::fs;
use std::path::{Path, PathBuf};

const MARKER_NAME: &str = "session.running";

fn marker_path(dir: &Path) -> PathBuf {
    dir.join(MARKER_NAME)
}

/// Marks this session as running. Returns true when the previous session did not end cleanly.
pub fn begin(dir: &Path) -> bool {
    let path = marker_path(dir);
    let crashed = path.exists();
    let _ = fs::create_dir_all(dir);
    let _ = fs::write(&path, std::process::id().to_string());
    crashed
}

/// Marks this session as ended cleanly.
pub fn end(dir: &Path) {
    let _ = fs::remove_file(marker_path(dir));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stellars-guard-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_clean_quit_leaves_no_crash_behind() {
        let dir = temp_dir("clean");
        assert!(!begin(&dir), "first start is not a crash");
        end(&dir);
        assert!(!begin(&dir), "start after a clean quit is not a crash");
        end(&dir);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_clean_quit_is_reported_once() {
        let dir = temp_dir("crash");
        assert!(!begin(&dir));
        assert!(begin(&dir), "marker left behind means the last session crashed");
        end(&dir);
        assert!(!begin(&dir));
        end(&dir);
        let _ = fs::remove_dir_all(&dir);
    }
}
