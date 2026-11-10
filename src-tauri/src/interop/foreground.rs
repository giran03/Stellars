//! Name of the executable that owns the foreground window. Uses only
//! `PROCESS_QUERY_LIMITED_INFORMATION` (via `query_process_image_info`), like the rest of the app.

use crate::interop::sessions::query_process_image_info;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// Lower-cased exe name (e.g. `cs2.exe`) of the foreground window's process, if any.
pub fn foreground_exe_name() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        query_process_image_info(pid).map(|(name, _)| name.to_ascii_lowercase())
    }
}
