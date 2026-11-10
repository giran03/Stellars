//! System tray: left click opens the main window, right click opens a small styled popup
//! (a frameless webview window) anchored next to the tray icon, like Steam or Discord.

use crate::commands::AppState;
use crate::state::logging;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

pub const LABEL: &str = "tray";

/// Visible card size in logical pixels. Must match `.card` in `src/TrayMenu.svelte`.
const CARD_W: f64 = 240.0;
const CARD_H: f64 = 211.0;
/// The window is exactly the card: no blurred shadow or margin around it.
const SHADOW_PAD: f64 = 0.0;
/// Space kept between the card and the tray icon / screen edge.
const ANCHOR_GAP: f64 = 8.0;
const EDGE_MARGIN: f64 = 8.0;

/// The popup is destroyed after this long hidden so its WebView2 process does not linger.
const IDLE_DESTROY_AFTER: Duration = Duration::from_secs(30);
/// A tray click arriving this soon after the popup closed itself is the click that closed it.
const REOPEN_SUPPRESS_MS: u64 = 300;
const FOCUS_GRACE: Duration = Duration::from_millis(600);
const FOCUS_POLL: Duration = Duration::from_millis(150);

static LAST_HIDE_MS: AtomicU64 = AtomicU64::new(0);
static HIDE_GENERATION: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------
// Placement
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Area {
    fn right(&self) -> f64 {
        self.x + self.w
    }
    fn bottom(&self) -> f64 {
        self.y + self.h
    }
    fn center_x(&self) -> f64 {
        self.x + self.w / 2.0
    }
    fn center_y(&self) -> f64 {
        self.y + self.h / 2.0
    }
    pub fn intersects(&self, other: &Area) -> bool {
        self.x < other.right() && other.x < self.right() && self.y < other.bottom() && other.y < self.bottom()
    }
}

fn clamp_range(value: f64, min: f64, max: f64) -> f64 {
    if max < min {
        min
    } else {
        value.clamp(min, max)
    }
}

/// Returns the top-left of a popup of `size` that sits beside the tray icon `anchor`, on the side
/// facing the middle of the screen (above a bottom taskbar, below a top one, and so on), kept
/// inside the `work` area and never overlapping the icon.
pub fn place_popup(anchor: Area, monitor: Area, work: Area, size: (f64, f64), gap: f64, margin: f64) -> (f64, f64) {
    let (w, h) = size;
    let to_bottom = monitor.bottom() - anchor.center_y();
    let to_top = anchor.center_y() - monitor.y;
    let to_left = anchor.center_x() - monitor.x;
    let to_right = monitor.right() - anchor.center_x();
    let nearest = to_bottom.min(to_top).min(to_left).min(to_right);

    let x_across = |cx: f64| clamp_range(cx - w / 2.0, work.x + margin, work.right() - margin - w);
    let y_across = |cy: f64| clamp_range(cy - h / 2.0, work.y + margin, work.bottom() - margin - h);

    if nearest == to_bottom {
        let y = (anchor.y - gap - h).min(work.bottom() - margin - h);
        (x_across(anchor.center_x()), y.max(work.y + margin))
    } else if nearest == to_top {
        let y = (anchor.bottom() + gap).max(work.y + margin);
        (x_across(anchor.center_x()), y.min(work.bottom() - margin - h))
    } else if nearest == to_right {
        let x = (anchor.x - gap - w).min(work.right() - margin - w);
        (x.max(work.x + margin), y_across(anchor.center_y()))
    } else {
        let x = (anchor.right() + gap).max(work.x + margin);
        (x.min(work.right() - margin - w), y_across(anchor.center_y()))
    }
}

// ---------------------------------------------------------------------------------------------
// Tray icon
// ---------------------------------------------------------------------------------------------

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = TrayIconBuilder::new()
        .tooltip("Stellars — Multi-Bus Audio Engine (Active)")
        .on_tray_icon_event(|tray, event| {
            let TrayIconEvent::Click { button, button_state: MouseButtonState::Up, rect, .. } = event else {
                return;
            };
            let app = tray.app_handle();
            match button {
                MouseButton::Left => {
                    hide_popup(app);
                    crate::show_or_recreate_main_window(app);
                }
                MouseButton::Right => toggle_popup(app, rect),
                _ => {}
            }
        });

    let icon = app
        .default_window_icon()
        .cloned()
        .or_else(|| tauri::image::Image::from_app_icon_resource(32).ok());
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

fn toggle_popup(app: &AppHandle, icon_rect: tauri::Rect) {
    if let Some(window) = app.get_webview_window(LABEL) {
        if window.is_visible().unwrap_or(false) {
            hide_popup(app);
            return;
        }
    }
    // The popup closes itself when it loses focus, which happens on the mouse-down of this very
    // click. Do not immediately reopen it.
    if now_ms().saturating_sub(LAST_HIDE_MS.load(Ordering::SeqCst)) < REOPEN_SUPPRESS_MS {
        return;
    }
    if let Err(err) = show_popup(app, icon_rect) {
        logging::warn(&format!("Tray menu could not be shown: {err}"));
    }
}

fn show_popup(app: &AppHandle, icon_rect: tauri::Rect) -> tauri::Result<()> {
    HIDE_GENERATION.fetch_add(1, Ordering::SeqCst);

    let anchor_pos = icon_rect.position.to_physical::<f64>(1.0);
    let anchor_size = icon_rect.size.to_physical::<f64>(1.0);
    let anchor = Area { x: anchor_pos.x, y: anchor_pos.y, w: anchor_size.width, h: anchor_size.height };

    let monitor = app
        .monitor_from_point(anchor.center_x(), anchor.center_y())?
        .or(app.primary_monitor()?);
    let (scale, monitor_area, work_area) = match &monitor {
        Some(m) => {
            let (p, s, wa) = (m.position(), m.size(), m.work_area());
            (
                m.scale_factor(),
                Area { x: p.x as f64, y: p.y as f64, w: s.width as f64, h: s.height as f64 },
                Area {
                    x: wa.position.x as f64,
                    y: wa.position.y as f64,
                    w: wa.size.width as f64,
                    h: wa.size.height as f64,
                },
            )
        }
        None => {
            let a = Area { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 };
            (1.0, a, a)
        }
    };

    let card = (CARD_W * scale, CARD_H * scale);
    let pad = SHADOW_PAD * scale;
    let (card_x, card_y) = place_popup(anchor, monitor_area, work_area, card, ANCHOR_GAP * scale, EDGE_MARGIN * scale);
    let position = PhysicalPosition::new((card_x - pad).round() as i32, (card_y - pad).round() as i32);
    let size = PhysicalSize::new((card.0 + pad * 2.0).round() as u32, (card.1 + pad * 2.0).round() as u32);

    let window = match app.get_webview_window(LABEL) {
        Some(window) => window,
        None => create_popup(app, position.x as f64 / scale, position.y as f64 / scale)?,
    };
    // Position, size, position again: moving between monitors with different scaling can
    // otherwise resize the window using the previous monitor's scale factor.
    window.set_position(position)?;
    window.set_size(size)?;
    window.set_position(position)?;
    window.show()?;
    window.set_position(position)?;
    let _ = window.set_focus();
    keep_position(window.clone(), position);
    let _ = app.emit_to(LABEL, "tray-menu-shown", ());
    spawn_focus_watch(app.clone());
    Ok(())
}

/// Some shell tweaks (for example "open new windows in the middle of the screen") move a window
/// right after it appears. Re-apply the intended position for a short moment to win that race.
fn keep_position(window: WebviewWindow, position: PhysicalPosition<i32>) {
    let _ = std::thread::Builder::new().name("tray-place".into()).spawn(move || {
        for delay_ms in [15u64, 30, 50, 80, 120, 200, 300] {
            std::thread::sleep(Duration::from_millis(delay_ms));
            if !window.is_visible().unwrap_or(false) {
                return;
            }
            if window.outer_position().map(|p| p != position).unwrap_or(false) {
                let _ = window.set_position(position);
            }
        }
    });
}

fn create_popup(app: &AppHandle, x: f64, y: f64) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?view=tray".into()))
        .title("Stellars tray menu")
        .inner_size(CARD_W + SHADOW_PAD * 2.0, CARD_H + SHADOW_PAD * 2.0)
        .position(x, y)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()
}

/// Hides the popup and, if it stays hidden, destroys it after a while to release its webview.
pub fn hide_popup(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        LAST_HIDE_MS.store(now_ms(), Ordering::SeqCst);
    }
    let generation = HIDE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    let _ = std::thread::Builder::new().name("tray-idle".into()).spawn(move || {
        std::thread::sleep(IDLE_DESTROY_AFTER);
        if HIDE_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        if let Some(window) = app.get_webview_window(LABEL) {
            if !window.is_visible().unwrap_or(false) && HIDE_GENERATION.load(Ordering::SeqCst) == generation {
                let _ = window.destroy();
            }
        }
    });
}

/// Backstop for the focus-lost event: if the popup is visible but never gets (or loses) focus,
/// close it anyway so it can never get stuck on screen.
fn spawn_focus_watch(app: AppHandle) {
    let generation = HIDE_GENERATION.load(Ordering::SeqCst);
    let _ = std::thread::Builder::new().name("tray-focus".into()).spawn(move || {
        let started = std::time::Instant::now();
        loop {
            std::thread::sleep(FOCUS_POLL);
            if HIDE_GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
            let Some(window) = app.get_webview_window(LABEL) else {
                return;
            };
            if !window.is_visible().unwrap_or(false) {
                return;
            }
            if started.elapsed() > FOCUS_GRACE && !window.is_focused().unwrap_or(true) {
                hide_popup(&app);
                return;
            }
        }
    });
}

/// Called from the global window-event handler for the popup window.
pub fn handle_window_event(window: &tauri::Window, event: &WindowEvent) {
    if let WindowEvent::Focused(false) = event {
        if window.is_visible().unwrap_or(false) {
            hide_popup(window.app_handle());
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Commands used by the popup
// ---------------------------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayMenuState {
    all_muted: bool,
}

#[tauri::command]
pub fn tray_menu_state(state: tauri::State<'_, AppState>) -> TrayMenuState {
    TrayMenuState { all_muted: state.is_all_muted() }
}

#[tauri::command]
pub fn tray_menu_hide(app: AppHandle) {
    hide_popup(&app);
}

/// Async on purpose: "open" may have to create the main window, and creating a window from a sync
/// command can deadlock on Windows.
#[tauri::command]
pub async fn tray_menu_action(app: AppHandle, action: String) {
    hide_popup(&app);
    match action.as_str() {
        "open" => crate::show_or_recreate_main_window(&app),
        "reset_endpoints" => {
            let _ = crate::interop::policy::clear_all_persisted_endpoints();
        }
        "toggle_mute_all" => {
            if let Some(state) = app.try_state::<AppState>() {
                let muted = state.toggle_mute_all();
                logging::info(if muted { "Tray: all channels muted" } else { "Tray: channel mutes restored" });
                let _ = app.emit("config-changed", ());
            }
        }
        "quit" => crate::quit_application(&app),
        other => logging::warn(&format!("Unknown tray menu action: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: Area = Area { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 };
    const WORK_BOTTOM_TASKBAR: Area = Area { x: 0.0, y: 0.0, w: 1920.0, h: 1040.0 };
    const SIZE: (f64, f64) = (240.0, 211.0);

    #[test]
    fn opens_above_a_bottom_taskbar_without_covering_the_icon() {
        let icon = Area { x: 1700.0, y: 1048.0, w: 24.0, h: 24.0 };
        let (x, y) = place_popup(icon, MONITOR, WORK_BOTTOM_TASKBAR, SIZE, 8.0, 8.0);
        let popup = Area { x, y, w: SIZE.0, h: SIZE.1 };
        assert!(!popup.intersects(&icon));
        assert!(popup.bottom() <= icon.y - 8.0 + 0.001);
        assert!(popup.bottom() <= WORK_BOTTOM_TASKBAR.bottom());
    }

    #[test]
    fn is_kept_inside_the_work_area_near_the_right_edge() {
        let icon = Area { x: 1890.0, y: 1048.0, w: 24.0, h: 24.0 };
        let (x, _) = place_popup(icon, MONITOR, WORK_BOTTOM_TASKBAR, SIZE, 8.0, 8.0);
        assert!(x + SIZE.0 <= MONITOR.w - 8.0 + 0.001);
    }

    #[test]
    fn opens_below_a_top_taskbar() {
        let work = Area { x: 0.0, y: 40.0, w: 1920.0, h: 1040.0 };
        let icon = Area { x: 1700.0, y: 8.0, w: 24.0, h: 24.0 };
        let (_, y) = place_popup(icon, MONITOR, work, SIZE, 8.0, 8.0);
        assert!(y >= icon.bottom() + 8.0 - 0.001);
    }

    #[test]
    fn opens_beside_a_left_taskbar() {
        let work = Area { x: 48.0, y: 0.0, w: 1872.0, h: 1080.0 };
        let icon = Area { x: 12.0, y: 1000.0, w: 24.0, h: 24.0 };
        let (x, y) = place_popup(icon, MONITOR, work, SIZE, 8.0, 8.0);
        let popup = Area { x, y, w: SIZE.0, h: SIZE.1 };
        assert!(!popup.intersects(&icon));
        assert!(x >= work.x);
    }

    #[test]
    fn opens_beside_a_right_taskbar() {
        let work = Area { x: 0.0, y: 0.0, w: 1872.0, h: 1080.0 };
        let icon = Area { x: 1884.0, y: 1000.0, w: 24.0, h: 24.0 };
        let (x, y) = place_popup(icon, MONITOR, work, SIZE, 8.0, 8.0);
        let popup = Area { x, y, w: SIZE.0, h: SIZE.1 };
        assert!(!popup.intersects(&icon));
        assert!(popup.right() <= work.right());
    }

    #[test]
    fn works_on_a_monitor_that_does_not_start_at_the_origin() {
        let monitor = Area { x: -1920.0, y: 0.0, w: 1920.0, h: 1080.0 };
        let work = Area { x: -1920.0, y: 0.0, w: 1920.0, h: 1040.0 };
        let icon = Area { x: -40.0, y: 1048.0, w: 24.0, h: 24.0 };
        let (x, y) = place_popup(icon, monitor, work, SIZE, 8.0, 8.0);
        assert!(x >= work.x && x + SIZE.0 <= work.right());
        assert!(y + SIZE.1 <= icon.y);
    }
}
