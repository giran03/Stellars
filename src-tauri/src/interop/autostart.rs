//! "Launch Stellars when Windows starts", implemented with the per-user `Run` registry key
//! (no administrator rights required).

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SAM_FLAGS, REG_SZ,
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: PCWSTR = w!("Stellars");

struct RunKey(HKEY);

impl Drop for RunKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open(access: REG_SAM_FLAGS) -> Result<RunKey, String> {
    unsafe {
        let mut key = HKEY::default();
        let status = RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, access, &mut key);
        if status == ERROR_SUCCESS {
            Ok(RunKey(key))
        } else {
            Err(format!("Could not open the Windows startup registry key ({status:?})"))
        }
    }
}

pub fn is_enabled() -> bool {
    let Ok(key) = open(KEY_READ) else {
        return false;
    };
    unsafe { RegQueryValueExW(key.0, VALUE_NAME, None, None, None, None) == ERROR_SUCCESS }
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let key = open(KEY_SET_VALUE)?;
    unsafe {
        if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let command = format!("\"{}\"", exe.display());
            let wide: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            let status = RegSetValueExW(key.0, VALUE_NAME, 0, REG_SZ, Some(bytes));
            if status != ERROR_SUCCESS {
                return Err(format!("Could not enable launch on startup ({status:?})"));
            }
        } else {
            let status = RegDeleteValueW(key.0, VALUE_NAME);
            if status != ERROR_SUCCESS && status != ERROR_FILE_NOT_FOUND {
                return Err(format!("Could not disable launch on startup ({status:?})"));
            }
        }
    }
    Ok(())
}
