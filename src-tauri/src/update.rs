//! Updates from GitHub Releases, modelled on SuiNotch. The latest release must carry the
//! installer `Stellars_<version>_x64-setup.exe` and a `SHA256SUMS.txt` that lists it (both are
//! produced when a release is published). The installer is checked against the sum, checked
//! again right before it runs, then started silently with `/S /UPDATE`; it closes this copy
//! and starts the new one.
//!
//! The installer installs for all users, so Windows asks for permission (UAC) when it starts.

use crate::commands::AppState;
use crate::state::app_settings::UpdateMode;
use crate::state::{logging, session_guard};
use serde::Serialize;
use serde_json::Value;
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use windows::core::{Result, HSTRING};
use windows::Foundation::Uri;
use windows::Security::Cryptography::Core::{HashAlgorithmNames, HashAlgorithmProvider};
use windows::Security::Cryptography::CryptographicBuffer;
use windows::Storage::Streams::{DataReader, IBuffer};
use windows::Web::Http::{HttpClient, HttpStatusCode};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

pub const REPO: &str = "giran03/Stellars";
const USER_AGENT: &str = concat!("Stellars/", env!("CARGO_PKG_VERSION"), " (update check)");
const SUMS_NAME: &str = "SHA256SUMS.txt";
const FIRST_CHECK: Duration = Duration::from_secs(90);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
/// Automatic installs wait until the PC has been left alone this long.
const IDLE_BEFORE_INSTALL_S: u32 = 10 * 60;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_NOTES_CHARS: usize = 400;
const EVENT: &str = "update-status";

static BUSY: AtomicBool = AtomicBool::new(false);
static APP: OnceLock<AppHandle> = OnceLock::new();
static STATUS: Mutex<Status> = Mutex::new(Status::Idle);
static LATEST: Mutex<Option<Release>> = Mutex::new(None);
static PREPARED: Mutex<Option<Download>> = Mutex::new(None);

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    pub setup_name: String,
    pub notes: Option<String>,
    pub page_url: String,
    #[serde(skip)]
    pub setup_url: String,
    #[serde(skip)]
    pub sums_url: String,
}

/// An installer whose checksum was verified, checked again immediately before launch.
#[derive(Clone, Debug, PartialEq)]
pub struct Download {
    pub release: Release,
    pub path: std::path::PathBuf,
    pub sha256: String,
}

/// What the interface shows. Sent to the window as the `update-status` event.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Status {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available { release: Release },
    Downloading { version: String },
    Ready { version: String },
    Installing { version: String },
    Installed { version: String },
    InstallFailed { version: String, message: String },
    Failed { message: String },
}

impl Status {
    /// States where the update needs the user's attention or is in progress.
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Available { .. } | Self::Downloading { .. } | Self::Ready { .. } | Self::Installing { .. } | Self::InstallFailed { .. }
        )
    }

    #[cfg(test)]
    fn version(&self) -> Option<&str> {
        match self {
            Self::Available { release } => Some(&release.version),
            Self::Downloading { version }
            | Self::Ready { version }
            | Self::Installing { version }
            | Self::Installed { version }
            | Self::InstallFailed { version, .. } => Some(version),
            _ => None,
        }
    }

    /// A routine recheck must not flicker the notice or discard an already verified download.
    fn receive(&mut self, incoming: Status) {
        if (self.is_active() || matches!(self, Self::Installed { .. })) && matches!(&incoming, Self::Checking | Self::Failed { .. }) {
            return;
        }
        if let (Self::Ready { version: old }, Self::Available { release }) = (&*self, &incoming) {
            if *old == release.version {
                return;
            }
        }
        if let (Self::Installed { version: old }, Self::Available { release }) = (&*self, &incoming) {
            if !is_newer(&release.version, old) {
                return;
            }
        }
        *self = incoming;
    }
}

fn post(status: Status) {
    let mut guard = STATUS.lock().unwrap_or_else(|e| e.into_inner());
    guard.receive(status);
    let current = guard.clone();
    drop(guard);
    if let Some(app) = APP.get() {
        let _ = app.emit(EVENT, current);
    }
}

pub fn current_status() -> Status {
    STATUS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn mode(app: &AppHandle) -> UpdateMode {
    app.try_state::<AppState>().map_or(UpdateMode::Notify, |s| s.app_settings().update_mode)
}

/// Background checks: shortly after launch, then every few hours while the setting allows it.
pub fn start(app: AppHandle) {
    let _ = APP.set(app.clone());
    let _ = std::thread::Builder::new().name("update-check".into()).spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            let current = mode(&app);
            if current != UpdateMode::Off {
                if let Some(release) = run_check() {
                    if current == UpdateMode::Automatic && wait_until_idle(&app) {
                        run_install(&release);
                    }
                }
            }
            std::thread::sleep(CHECK_EVERY);
        }
    });
}

/// "Check for updates" in Settings.
pub fn check_now() {
    std::thread::spawn(|| {
        run_check();
    });
}

/// Downloads and verifies the update, leaving the restart decision to the user.
pub fn download() {
    if let Some(release) = LATEST.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        std::thread::spawn(move || run_download(&release, false));
    }
}

/// Installs the downloaded update (downloading it first when needed) and restarts.
pub fn install() {
    if let Some(release) = LATEST.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        std::thread::spawn(move || run_install(&release));
    }
}

fn idle_seconds() -> u32 {
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe {
        if GetLastInputInfo(&mut info).as_bool() {
            return GetTickCount().wrapping_sub(info.dwTime) / 1000;
        }
    }
    0
}

fn wait_until_idle(app: &AppHandle) -> bool {
    while idle_seconds() < IDLE_BEFORE_INSTALL_S {
        if mode(app) != UpdateMode::Automatic {
            return false;
        }
        std::thread::sleep(Duration::from_secs(60));
    }
    mode(app) == UpdateMode::Automatic
}

struct BusyGuard;
impl BusyGuard {
    fn acquire() -> Option<Self> {
        // Only build the guard once the lock is held: a guard that is dropped without owning
        // the lock would release another operation's lock.
        BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok().then(|| Self)
    }
}
impl Drop for BusyGuard {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}

fn run_check() -> Option<Release> {
    let _busy = BusyGuard::acquire()?;
    post(Status::Checking);
    let (status, found) = match latest() {
        Ok(Some(r)) if is_newer(&r.version, env!("CARGO_PKG_VERSION")) => (Status::Available { release: r.clone() }, Some(r)),
        Ok(_) => (Status::UpToDate, None),
        Err(message) => {
            logging::warn(&format!("Update check failed: {message}"));
            (Status::Failed { message }, None)
        }
    };
    if let Some(release) = &found {
        *LATEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(release.clone());
    }
    post(status);
    stamp_last_check();
    found
}

fn stamp_last_check() {
    let Some(state) = APP.get().and_then(|a| a.try_state::<AppState>()) else { return };
    let mut settings = state.app_settings();
    settings.last_update_check = Some(format!("{}Z", logging::timestamp(logging::unix_now_secs()).replace(' ', "T")));
    if settings.save_to_path(&state.settings_path).is_ok() {
        if let Ok(mut current) = state.settings.lock() {
            *current = settings;
        }
    }
}

fn run_install(release: &Release) {
    let prepared = PREPARED.lock().unwrap_or_else(|e| e.into_inner()).clone();
    match prepared.filter(|d| d.release == *release && d.path.is_file()) {
        Some(download) => {
            let Some(_busy) = BusyGuard::acquire() else { return };
            launch(&download);
        }
        None => run_download(release, true),
    }
}

fn run_download(release: &Release, install_after: bool) {
    let Some(_busy) = BusyGuard::acquire() else { return };
    post(Status::Downloading { version: release.version.clone() });
    match download_verified(release) {
        Ok(download) => {
            *PREPARED.lock().unwrap_or_else(|e| e.into_inner()) = Some(download.clone());
            if install_after {
                launch(&download);
            } else {
                post(Status::Ready { version: release.version.clone() });
            }
        }
        Err(message) => {
            logging::warn(&format!("Update download failed: {message}"));
            post(Status::InstallFailed { version: release.version.clone(), message });
        }
    }
}

fn verify_download(download: &Download) -> std::result::Result<(), String> {
    let data = std::fs::read(&download.path).map_err(|_| "The downloaded installer is missing. Download the update again.")?;
    let buffer = CryptographicBuffer::CreateFromByteArray(&data).map_err(|e| e.message())?;
    if sha256_hex(&buffer).map_err(|e| e.message())? != download.sha256 {
        return Err("The installer changed after download. Download the update again.".into());
    }
    Ok(())
}

/// Runs the installer with administrator rights and waits for it. Returns its exit code.
fn run_installer_elevated(path: &std::path::Path) -> std::result::Result<i32, String> {
    let quoted = path.display().to_string().replace('\'', "''");
    let script = format!(
        "$p = Start-Process -FilePath '{quoted}' -ArgumentList '/S','/UPDATE' -Verb RunAs -Wait -PassThru; exit $p.ExitCode"
    );
    let output = std::process::Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-Command", &script])
        .output()
        .map_err(|_| "Couldn't start the installer".to_string())?;
    if output.status.success() {
        return Ok(0);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    if stderr.contains("canceled by the user") || stderr.contains("cancelled") {
        return Err("Windows permission was declined, so the update was not installed.".into());
    }
    Ok(output.status.code().unwrap_or(1))
}

fn launch(download: &Download) {
    let version = download.release.version.clone();
    let data_dir = APP.get().and_then(|a| a.try_state::<AppState>()).map(|s| {
        s.flush_config();
        s.data_dir()
    });
    let result = verify_download(download).and_then(|()| {
        logging::info(&format!("Installing update {version}"));
        post(Status::Installing { version: version.clone() });
        // The installer closes this app on purpose, so that is not a crash.
        if let Some(dir) = &data_dir {
            session_guard::end(dir);
        }
        match run_installer_elevated(&download.path)? {
            0 => Ok(()),
            _ => Err("Installation did not complete. Please try again.".to_string()),
        }
    });
    match result {
        Ok(()) => post(Status::Installed { version }),
        Err(message) => {
            logging::warn(&format!("Update install failed: {message}"));
            if let Some(dir) = &data_dir {
                session_guard::begin(dir);
            }
            *PREPARED.lock().unwrap_or_else(|e| e.into_inner()) = None;
            post(Status::InstallFailed { version, message });
        }
    }
}

// ------------------------------------------------------------------ network

fn client() -> Result<HttpClient> {
    let client = HttpClient::new()?;
    let headers = client.DefaultRequestHeaders()?;
    let _ = headers.UserAgent()?.TryParseAdd(&HSTRING::from(USER_AGENT));
    let _ = headers.TryAppendWithoutValidation(&HSTRING::from("Accept"), &HSTRING::from("application/vnd.github+json"));
    Ok(client)
}

fn get_buffer(client: &HttpClient, url: &str) -> std::result::Result<IBuffer, String> {
    if !url.starts_with("https://") {
        return Err("Refused a download that is not over https".into());
    }
    let uri = Uri::CreateUri(&HSTRING::from(url)).map_err(|e| e.message())?;
    let resp = client.GetAsync(&uri).and_then(|op| op.get()).map_err(|_| "No internet connection".to_string())?;
    let code = resp.StatusCode().map_err(|e| e.message())?;
    if code == HttpStatusCode::NotFound {
        return Err("No public release found".into());
    }
    if !resp.IsSuccessStatusCode().unwrap_or(false) {
        return Err(format!("Update server replied {}", code.0));
    }
    resp.Content().and_then(|c| c.ReadAsBufferAsync()).and_then(|op| op.get()).map_err(|e| e.message())
}

fn bytes(buf: &IBuffer) -> std::result::Result<Vec<u8>, String> {
    let len = buf.Length().map_err(|e| e.message())? as usize;
    let mut out = vec![0u8; len];
    let reader = DataReader::FromBuffer(buf).map_err(|e| e.message())?;
    reader.ReadBytes(&mut out).map_err(|e| e.message())?;
    Ok(out)
}

fn latest() -> std::result::Result<Option<Release>, String> {
    let client = client().map_err(|e| e.message())?;
    let body = bytes(&get_buffer(&client, &format!("https://api.github.com/repos/{REPO}/releases/latest"))?)?;
    let json: Value = serde_json::from_slice(&body).map_err(|_| "Unreadable release data".to_string())?;
    Ok(parse_release(&json))
}

fn download_verified(release: &Release) -> std::result::Result<Download, String> {
    let client = client().map_err(|e| e.message())?;
    let sums = String::from_utf8(bytes(&get_buffer(&client, &release.sums_url)?)?).map_err(|_| "Unreadable checksum file".to_string())?;
    let expected = expected_sum(&sums, &release.setup_name).ok_or("The release has no checksum for its installer")?;
    let setup = get_buffer(&client, &release.setup_url)?;
    if sha256_hex(&setup).map_err(|e| e.message())? != expected {
        return Err("The download was damaged, please try again".into());
    }
    let dir = std::env::temp_dir().join("Stellars-update");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(&release.setup_name);
    std::fs::write(&path, bytes(&setup)?).map_err(|e| e.to_string())?;
    Ok(Download { release: release.clone(), path, sha256: expected })
}

fn sha256_hex(buf: &IBuffer) -> Result<String> {
    let provider = HashAlgorithmProvider::OpenAlgorithm(&HashAlgorithmNames::Sha256()?)?;
    let hash = provider.HashData(buf)?;
    Ok(CryptographicBuffer::EncodeToHexString(&hash)?.to_string().to_ascii_lowercase())
}

// ------------------------------------------------------------------ parsing

/// Plain, short version of the release notes for the update notice.
fn summarize_notes(body: &str) -> Option<String> {
    let text = body.replace(['#', '*', '_', '`', '>', '[', ']'], "").split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    Some(if text.chars().count() > MAX_NOTES_CHARS {
        format!("{}…", text.chars().take(MAX_NOTES_CHARS - 1).collect::<String>())
    } else {
        text
    })
}

/// Reads a GitHub "latest release" response; None when it lacks the installer or its sums.
pub fn parse_release(v: &Value) -> Option<Release> {
    let version = v.get("tag_name")?.as_str()?.trim_start_matches(['v', 'V']).to_string();
    if !version.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+')) {
        return None;
    }
    parse_version(&version)?;
    let assets = v.get("assets")?.as_array()?;
    let asset = |pred: &dyn Fn(&str) -> bool| {
        assets.iter().find_map(|a| {
            let name = a.get("name")?.as_str()?;
            let url = a.get("browser_download_url")?.as_str()?;
            pred(name).then(|| (name.to_string(), url.to_string()))
        })
    };
    let expected_name = format!("Stellars_{version}_x64-setup.exe");
    let (setup_name, setup_url) = asset(&|n| n.eq_ignore_ascii_case(&expected_name))?;
    let (_, sums_url) = asset(&|n| n.eq_ignore_ascii_case(SUMS_NAME))?;
    let page_url = format!("https://github.com/{REPO}/releases/tag/v{version}");
    let notes = v.get("body").and_then(Value::as_str).and_then(summarize_notes);
    Some(Release { version, setup_name, notes, page_url, setup_url, sums_url })
}

/// Finds `name` in a `sha256sum`-style file ("<hex>  <name>" per line).
pub fn expected_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.trim().split_once(char::is_whitespace)?;
        let file = file.trim().trim_start_matches('*');
        (file.eq_ignore_ascii_case(name) && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())).then(|| hash.to_ascii_lowercase())
    })
}

pub fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let core = v.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u32>().ok());
    let major = parts.next()??;
    let minor = parts.next().unwrap_or(Some(0))?;
    let patch = parts.next().unwrap_or(Some(0))?;
    Some((major, minor, patch))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn release(version: &str) -> Release {
        Release {
            version: version.into(),
            setup_name: format!("Stellars_{version}_x64-setup.exe"),
            notes: None,
            page_url: "https://example.invalid/page".into(),
            setup_url: "https://example.invalid/setup".into(),
            sums_url: "https://example.invalid/sums".into(),
        }
    }

    fn prepared() -> Download {
        Download { release: release("99.0.0"), path: std::path::PathBuf::from("missing-test-installer.exe"), sha256: "0".repeat(64) }
    }

    #[test]
    fn notice_shows_for_actionable_states_and_hides_on_completion() {
        for status in [Status::Idle, Status::Checking, Status::UpToDate, Status::Failed { message: "offline".into() }, Status::Installed { version: "99.0.0".into() }] {
            assert!(!status.is_active());
        }
        for status in [
            Status::Available { release: release("99.0.0") },
            Status::Downloading { version: "99.0.0".into() },
            Status::Ready { version: "99.0.0".into() },
            Status::Installing { version: "99.0.0".into() },
            Status::InstallFailed { version: "99.0.0".into(), message: "x".into() },
        ] {
            assert!(status.is_active());
        }
        let mut status = Status::Idle;
        status.receive(Status::Available { release: release("99.0.0") });
        status.receive(Status::Ready { version: "99.0.0".into() });
        status.receive(Status::Installed { version: "99.0.0".into() });
        assert!(!status.is_active());
        assert_eq!(status.version(), Some("99.0.0"));
    }

    #[test]
    fn rechecks_do_not_discard_a_download_or_resurrect_an_installed_update() {
        let mut status = Status::Ready { version: "99.0.0".into() };
        for incoming in [Status::Checking, Status::Failed { message: "offline".into() }, Status::Available { release: release("99.0.0") }] {
            status.receive(incoming);
            assert!(matches!(status, Status::Ready { .. }));
        }
        status.receive(Status::Installed { version: "99.0.0".into() });
        status.receive(Status::Checking);
        status.receive(Status::Available { release: release("99.0.0") });
        assert!(!status.is_active());
        status.receive(Status::Available { release: release("99.0.1") });
        assert!(status.is_active());
    }

    #[test]
    fn a_second_operation_cannot_unlock_a_running_one() {
        let first = BusyGuard::acquire().unwrap();
        assert!(BusyGuard::acquire().is_none());
        assert!(BUSY.load(Ordering::Acquire));
        drop(first);
        assert!(!BUSY.load(Ordering::Acquire));
    }

    #[test]
    fn a_changed_or_missing_installer_is_never_launched() {
        let mut download = prepared();
        assert!(verify_download(&download).is_err());
        let dir = std::env::temp_dir().join(format!("stellars-verify-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        download.path = dir.join("fixture.bin");
        std::fs::write(&download.path, b"abc").unwrap();
        download.sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into();
        assert!(verify_download(&download).is_ok());
        std::fs::write(&download.path, b"changed").unwrap();
        assert!(verify_download(&download).is_err());
        std::fs::remove_file(&download.path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn only_the_expected_installer_name_is_accepted() {
        for name in ["../Stellars_1.2.0_x64-setup.exe", "Other-Setup.exe", "Stellars_1.1.0_x64-setup.exe"] {
            let value = json!({"tag_name":"v1.2.0", "assets":[{"name":name,"browser_download_url":"https://example.invalid/s"},{"name":"SHA256SUMS.txt","browser_download_url":"https://example.invalid/h"}]});
            assert!(parse_release(&value).is_none(), "{name}");
        }
    }

    #[test]
    fn reads_installer_sums_and_notes_from_a_release() {
        let v = json!({
            "tag_name": "v1.2.0",
            "body": "## Added\n\n- **New thing** works",
            "assets": [
                { "name": "Stellars_1.2.0_x64-setup.exe", "browser_download_url": "https://x/s" },
                { "name": "SHA256SUMS.txt", "browser_download_url": "https://x/h" }
            ]
        });
        let r = parse_release(&v).unwrap();
        assert_eq!(r.version, "1.2.0");
        assert_eq!(r.setup_name, "Stellars_1.2.0_x64-setup.exe");
        assert_eq!(r.setup_url, "https://x/s");
        assert_eq!(r.sums_url, "https://x/h");
        assert_eq!(r.page_url, "https://github.com/giran03/Stellars/releases/tag/v1.2.0");
        assert_eq!(r.notes.as_deref(), Some("Added - New thing works"));
    }

    #[test]
    fn a_release_without_checksums_is_ignored() {
        let v = json!({ "tag_name": "v1.2.0", "assets": [{ "name": "Stellars_1.2.0_x64-setup.exe", "browser_download_url": "https://x/s" }] });
        assert_eq!(parse_release(&v), None);
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("1.0.10", "1.0.9"));
        assert!(is_newer("v2.0.0", "1.9.9"));
        assert!(is_newer("1.1", "1.0.5"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.0", "1.0.0"));
        assert!(!is_newer("nightly", "1.0.0"));
    }

    #[test]
    fn finds_the_sum_for_a_file() {
        let h = "956b4afaf1d2bbcc8c80789a4b4f3dae4a3f0cd285be6452a965bc96ca15e234";
        let sums = format!("{h}  Stellars_1.0.2_x64-setup.exe\r\n{}  other.txt\n", "0".repeat(64));
        assert_eq!(expected_sum(&sums, "Stellars_1.0.2_x64-setup.exe").as_deref(), Some(h));
        assert_eq!(expected_sum(&format!("{} *Stellars_1.0.2_x64-setup.exe", h.to_uppercase()), "Stellars_1.0.2_x64-setup.exe").as_deref(), Some(h));
        assert_eq!(expected_sum("abc  Stellars_1.0.2_x64-setup.exe", "Stellars_1.0.2_x64-setup.exe"), None);
        assert_eq!(expected_sum(&sums, "missing.exe"), None);
    }

    #[test]
    fn hashes_with_sha256() {
        let buf = CryptographicBuffer::ConvertStringToBinary(&HSTRING::from("abc"), windows::Security::Cryptography::BinaryStringEncoding::Utf8).unwrap();
        assert_eq!(sha256_hex(&buf).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    /// Talks to GitHub: `cargo test -- --ignored live_latest_release`.
    #[test]
    #[ignore]
    fn live_latest_release() {
        println!("{:?}", latest());
    }
}
