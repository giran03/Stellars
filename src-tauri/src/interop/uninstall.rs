//! Uninstall-time cleanup. At install time Stellars records which virtual cables it installed
//! itself and what the Windows default devices were; the uninstaller (`--uninstall-cables`) then
//! removes only those cables and puts the user's audio back the way it was before Stellars.

use crate::interop::cables::inspect_virtual_cables;
use crate::interop::default_device::{restore_original_defaults, DefaultDevices};
use crate::interop::policy::clear_all_persisted_endpoints;
use crate::interop::sessions::enumerate_audio_endpoints;
use crate::interop::silent_sink::sync_mute;
use crate::state::config::RouterConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

/// Command-line flag the uninstaller uses to undo the audio changes without opening the app.
pub const UNINSTALL_CABLES_FLAG: &str = "--uninstall-cables";

/// Exit code returned when Windows must restart to finish removing a driver.
pub const UNINSTALL_REBOOT_EXIT_CODE: i32 = 3010;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Lines the elevated setup script logs when it really installed a cable (not when it was there).
const LOG_INSTALLED_VB: &str = "Installed VB-Audio Virtual Cable";
const LOG_INSTALLED_HIFI: &str = "Installed VB-Audio Hi-Fi Cable";

/// What the installer did to the PC, saved so the uninstaller can undo exactly that.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InstallState {
    pub installed_vb_cable: bool,
    pub installed_hifi_cable: bool,
    pub original_render_default: Option<String>,
    pub original_capture_default: Option<String>,
}

impl InstallState {
    fn original_defaults(&self) -> DefaultDevices {
        DefaultDevices {
            render: self.original_render_default.clone(),
            capture: self.original_capture_default.clone(),
        }
    }
}

pub fn is_uninstall_invocation<I: IntoIterator<Item = String>>(args: I) -> bool {
    args.into_iter().skip(1).any(|a| a == UNINSTALL_CABLES_FLAG)
}

fn state_path() -> PathBuf {
    RouterConfig::default_config_path().with_file_name("install_state.json")
}

fn staging_dir() -> PathBuf {
    RouterConfig::default_config_path()
        .parent()
        .map(|p| p.join("drivers"))
        .unwrap_or_else(|| PathBuf::from("drivers"))
}

pub fn load_state() -> Option<InstallState> {
    serde_json::from_str(&fs::read_to_string(state_path()).ok()?).ok()
}

/// Combines what an earlier install recorded with what this run did. Cables an earlier run
/// installed stay ours (they are "already present" now), and the first-seen defaults are kept.
pub fn merge_install_state(
    existing: Option<InstallState>,
    install_log: &[String],
    defaults_before: &DefaultDevices,
) -> InstallState {
    let installed_now = |prefix: &str| install_log.iter().any(|l| l.starts_with(prefix));
    let existing = existing.unwrap_or_default();
    InstallState {
        installed_vb_cable: existing.installed_vb_cable || installed_now(LOG_INSTALLED_VB),
        installed_hifi_cable: existing.installed_hifi_cable || installed_now(LOG_INSTALLED_HIFI),
        original_render_default: existing.original_render_default.or_else(|| defaults_before.render.clone()),
        original_capture_default: existing.original_capture_default.or_else(|| defaults_before.capture.clone()),
    }
}

/// Saves what the installer just did. Returns a log line.
pub fn record_install(install_log: &[String], defaults_before: &DefaultDevices) -> String {
    let state = merge_install_state(load_state(), install_log, defaults_before);
    let path = state_path();
    let saved = path
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|_| serde_json::to_string_pretty(&state).map_err(std::io::Error::other))
        .and_then(|json| fs::write(&path, json));
    match saved {
        Ok(()) => "Saved the install record used to undo the audio changes on uninstall".to_string(),
        Err(err) => format!("Could not save the install record: {err}"),
    }
}

/// PowerShell that removes the cables Stellars installed. Runs from the (already elevated)
/// uninstaller. It uses the vendor's own signed uninstaller when it is still staged, then sweeps
/// any leftover device and its driver package by hardware ID, so other VB-Audio products stay.
fn build_removal_script(remove_vb: bool, remove_hifi: bool, staging: &str) -> String {
    let staging = staging.replace('\'', "''");
    let flag = |b: bool| if b { "$true" } else { "$false" };
    format!(
        r#"$ErrorActionPreference = 'Continue'
$staging = '{staging}'
$script:reboot = $false

function Test-VbAudioSigned([string]$path) {{
    $sig = Get-AuthenticodeSignature -LiteralPath $path
    return ($sig.Status -eq 'Valid' -and $sig.SignerCertificate.Subject -match 'Vincent Burel|BUREL VINCENT')
}}

function Remove-Cable([string]$hwTag, [string]$setupExe) {{
    if ((Test-Path -LiteralPath $setupExe) -and (Test-VbAudioSigned $setupExe)) {{
        Start-Process -FilePath $setupExe -ArgumentList '-u','-h' -WorkingDirectory (Split-Path $setupExe) -Wait -WindowStyle Hidden
    }}
    $infs = New-Object System.Collections.Generic.HashSet[string]
    $candidates = Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object {{
        $_.InstanceId -match $hwTag -or $_.Manufacturer -match 'VB-Audio|Vincent' -or $_.FriendlyName -match 'CABLE|Hi-Fi|Stellars'
    }}
    foreach ($d in $candidates) {{
        $hw = @((Get-PnpDeviceProperty -InstanceId $d.InstanceId -KeyName 'DEVPKEY_Device_HardwareIds' -ErrorAction SilentlyContinue).Data)
        if (-not ($d.InstanceId -match $hwTag -or ($hw | Where-Object {{ $_ -match $hwTag }}))) {{ continue }}
        $inf = (Get-PnpDeviceProperty -InstanceId $d.InstanceId -KeyName 'DEVPKEY_Device_DriverInfPath' -ErrorAction SilentlyContinue).Data
        if ($inf) {{ [void]$infs.Add([string]$inf) }}
        pnputil.exe /remove-device $d.InstanceId | Out-Null
        if ($LASTEXITCODE -eq 3010) {{ $script:reboot = $true }}
    }}
    foreach ($inf in $infs) {{
        pnputil.exe /delete-driver $inf /uninstall | Out-Null
        if ($LASTEXITCODE -eq 3010) {{ $script:reboot = $true }}
    }}
}}

try {{
    if ({remove_vb}) {{ Remove-Cable 'VBAudioVACWDM' (Join-Path $staging 'vbcable\VBCABLE_Setup_x64.exe') }}
    if ({remove_hifi}) {{ Remove-Cable 'VBAudioHFVAIO' (Join-Path $staging 'hificable\HiFiCableAsioBridgeSetup.exe') }}
    pnputil.exe /scan-devices | Out-Null
}} catch {{
    Write-Output "Cable removal warning: $($_.Exception.Message)"
}}
if ($script:reboot) {{ exit 3010 }}
exit 0
"#,
        remove_vb = flag(remove_vb),
        remove_hifi = flag(remove_hifi),
    )
}

/// Runs the removal script; returns a log line and whether Windows needs a restart.
fn remove_installed_cables(state: &InstallState) -> (Vec<String>, bool) {
    let staging = staging_dir();
    let script = build_removal_script(
        state.installed_vb_cable,
        state.installed_hifi_cable,
        &staging.display().to_string(),
    );
    let script_path = std::env::temp_dir().join("stellars_remove_cables.ps1");
    if let Err(err) = fs::write(&script_path, script) {
        return (vec![format!("Could not write the cable removal script: {err}")], false);
    }
    let output = Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"])
        .arg(&script_path)
        .output();
    let _ = fs::remove_file(&script_path);
    let mut log = Vec::new();
    let mut reboot = false;
    match output {
        Ok(out) => {
            let code = out.status.code();
            reboot = code == Some(UNINSTALL_REBOOT_EXIT_CODE);
            if code == Some(0) || reboot {
                log.push("Removed the virtual cables Stellars installed".to_string());
            } else {
                log.push(format!("Cable removal finished with exit code {code:?}"));
            }
        }
        Err(err) => log.push(format!("Could not run the cable removal: {err}")),
    }
    // Wait for Windows to drop the cable endpoints before the defaults are put back.
    for _ in 0..10 {
        if let Ok(eps) = enumerate_audio_endpoints() {
            let status = inspect_virtual_cables(&RouterConfig::default(), &eps);
            let vb_gone = !state.installed_vb_cable || !status.vb_cable_installed;
            let hifi_gone = !state.installed_hifi_cable || !status.hifi_cable_installed;
            if vb_gone && hifi_gone {
                break;
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    (log, reboot)
}

/// Sends every app's output back to the Windows default and unmutes a physical output that
/// Stellars kept muted as the silent sink. Returns a log line per change.
pub fn reset_routing_and_mutes(silent_sink_id: Option<&str>) -> Vec<String> {
    let mut log = Vec::new();
    if let Ok(endpoints) = enumerate_audio_endpoints() {
        log.extend(sync_mute(silent_sink_id, &RouterConfig::default(), &endpoints));
    }
    match clear_all_persisted_endpoints() {
        Ok(()) => log.push("Reset every app's output to the Windows default".to_string()),
        Err(err) => log.push(format!("Could not reset per-app outputs: {err}")),
    }
    log
}

/// Puts the default speakers and microphone back to what they were before Stellars was installed,
/// using the install record. Returns a log; says so when there is no record.
pub fn restore_saved_defaults() -> Vec<String> {
    match load_state() {
        Some(state) => restore_original_defaults(&state.original_defaults()),
        None => vec!["No install record: the Windows default devices were left as they are".to_string()],
    }
}

/// Undoes what Stellars changed in Windows audio: routing, mute, cables and default devices.
/// Never panics, so the uninstall always finishes. Returns a log and whether a restart is needed.
pub fn run_uninstaller_cleanup() -> (Vec<String>, bool) {
    let state = load_state();
    let cfg = RouterConfig::load_from_path(&RouterConfig::default_config_path());
    let mut log = reset_routing_and_mutes(cfg.dummy_sink_device_id.as_deref());

    let mut reboot = false;
    if let Some(state) = &state {
        if state.installed_vb_cable || state.installed_hifi_cable {
            let (lines, needs_restart) = remove_installed_cables(state);
            log.extend(lines);
            reboot = needs_restart;
            let _ = fs::remove_dir_all(staging_dir());
        }
        log.extend(restore_original_defaults(&state.original_defaults()));
    } else {
        log.push("No install record found: left the virtual cables and default devices as they are".to_string());
    }
    let _ = fs::remove_file(state_path());
    (log, reboot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults(render: &str, capture: &str) -> DefaultDevices {
        DefaultDevices { render: Some(render.into()), capture: Some(capture.into()) }
    }

    fn log(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn records_only_the_cables_this_install_added() {
        let state = merge_install_state(
            None,
            &log(&["Installed VB-Audio Virtual Cable (CABLE Input / CABLE Output)", "Verified VB-Audio Hi-Fi Cable is installed and active"]),
            &defaults("spk", "mic"),
        );
        assert!(state.installed_vb_cable);
        assert!(!state.installed_hifi_cable, "a cable that was already there is not ours to remove");
        assert_eq!(state.original_render_default.as_deref(), Some("spk"));
    }

    #[test]
    fn an_update_keeps_the_first_record() {
        let first = merge_install_state(
            None,
            &log(&["Installed VB-Audio Virtual Cable (CABLE Input / CABLE Output)"]),
            &defaults("spk", "mic"),
        );
        let update = merge_install_state(
            Some(first),
            &log(&["Verified VB-Audio Virtual Cable is installed and active"]),
            &defaults("cable-in", "cable-out"),
        );
        assert!(update.installed_vb_cable, "cables from the first install stay ours");
        assert_eq!(update.original_render_default.as_deref(), Some("spk"));
        assert_eq!(update.original_capture_default.as_deref(), Some("mic"));
    }

    #[test]
    fn removal_script_targets_only_the_requested_cables() {
        let script = build_removal_script(true, false, "C:\\it's\\drivers");
        assert!(script.contains("if ($true) { Remove-Cable 'VBAudioVACWDM'"));
        assert!(script.contains("if ($false) { Remove-Cable 'VBAudioHFVAIO'"));
        assert!(script.contains("C:\\it''s\\drivers"));
        assert!(script.contains("Test-VbAudioSigned $setupExe"));
        assert!(!script.contains("/force"));
        assert!(!script.contains("http://"));
    }

    #[test]
    fn uninstall_flag_is_detected_only_after_the_program_name() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(is_uninstall_invocation(args(&["stellars.exe", "--uninstall-cables"])));
        assert!(!is_uninstall_invocation(args(&["--uninstall-cables"])));
        assert!(!is_uninstall_invocation(args(&["stellars.exe"])));
    }
}
