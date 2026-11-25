//! System tray: left click opens the main window, right click opens a native Win32 layered menu
//! anchored next to the tray icon. It is drawn in software with the app's own theme tokens and
//! needs no WebView2, so it opens instantly and costs no memory while closed.

use crate::commands::AppState;
use crate::state::app_settings::ThemeMode;
use crate::state::logging;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, GdiFlush, SelectObject,
    SetBkMode, SetTextColor, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_LEFT, DT_NOPREFIX,
    DT_RIGHT, DT_SINGLELINE, DT_VCENTER, FF_SWISS, HBITMAP, HDC, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS, TRANSPARENT,
};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_DOWN, VK_ESCAPE, VK_LBUTTON, VK_RBUTTON,
    VK_RETURN, VK_SPACE, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos, GetForegroundWindow, GetMessageW,
    GetWindowLongPtrW, GetWindowRect, KillTimer, LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassExW,
    SetCursor, SetForegroundWindow, SetTimer, SetWindowLongPtrW, ShowWindow, SystemParametersInfoW, TranslateMessage,
    UpdateLayeredWindow, GWLP_USERDATA, HMENU, HTCLIENT, IDC_ARROW, MSG, SPI_GETCLIENTAREAANIMATION, SW_SHOW,
    ULW_ALPHA, WA_INACTIVE, WM_ACTIVATE, WM_APP, WM_CLOSE, WM_DESTROY, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCHITTEST, WM_SETCURSOR, WM_SETTINGCHANGE, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

/// Space kept between the menu and the tray icon / screen edge, in logical pixels.
const ANCHOR_GAP: f64 = 8.0;
const EDGE_MARGIN: f64 = 8.0;

/// A tray click arriving this soon after the menu closed itself is the click that closed it.
const REOPEN_SUPPRESS_MS: u64 = 300;
const WM_APP_REFRESH: u32 = WM_APP + 1;
/// Not exported without `Win32_UI_Controls`; fixed value from `WinUser.h`.
const WM_MOUSELEAVE: u32 = 0x02A3;
const TIMER_ID: usize = 1;

static LAST_MENU_CLOSE: AtomicU64 = AtomicU64::new(0);
static MENU_HWND: AtomicIsize = AtomicIsize::new(0);
static MENU_OPEN: AtomicBool = AtomicBool::new(false);

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

const TRAY_ID: &str = "main";
const TOOLTIP_ACTIVE: &str = "Stellars — Multi-Bus Audio Engine (Active)";

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(TOOLTIP_ACTIVE)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            let TrayIconEvent::Click { button, button_state: MouseButtonState::Up, rect, position, .. } = event else {
                return;
            };
            let app = tray.app_handle();
            match button {
                MouseButton::Left => crate::show_or_recreate_main_window(app),
                MouseButton::Right => open_menu(app, rect, position),
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
    spawn_state_watch(app.clone());
    Ok(())
}

/// The tray icon with a small badge in its corner: red when everything is muted, amber when only
/// the microphone is muted.
fn badged_icon(base: &tauri::image::Image<'_>, all_muted: bool) -> tauri::image::Image<'static> {
    let (w, h) = (base.width() as i32, base.height() as i32);
    let mut rgba = base.rgba().to_vec();
    let color: [u8; 3] = if all_muted { [229, 72, 77] } else { [245, 166, 35] };
    let radius = (w.min(h) as f32) * 0.26;
    let (cx, cy) = (w as f32 - radius - 0.5, h as f32 - radius - 0.5);
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy)) - radius;
            let cov = (0.5 - d).clamp(0.0, 1.0);
            if cov > 0.0 {
                let i = ((y * w + x) as usize) * 4;
                for c in 0..3 {
                    rgba[i + c] = (f32::from(rgba[i + c]) * (1.0 - cov) + f32::from(color[c]) * cov).round() as u8;
                }
                rgba[i + 3] = rgba[i + 3].max((cov * 255.0).round() as u8);
            }
        }
    }
    tauri::image::Image::new_owned(rgba, w as u32, h as u32)
}

/// Keeps the tray icon and tooltip in step with the mute state, whoever changed it.
fn spawn_state_watch(app: AppHandle) {
    let base = app
        .default_window_icon()
        .map(|i| tauri::image::Image::new_owned(i.rgba().to_vec(), i.width(), i.height()))
        .or_else(|| tauri::image::Image::from_app_icon_resource(32).ok());
    let Some(base) = base else {
        return;
    };
    let _ = std::thread::Builder::new().name("tray-state".into()).spawn(move || {
        let mut shown: Option<(bool, bool)> = None;
        loop {
            std::thread::sleep(Duration::from_millis(700));
            let Some(state) = app.try_state::<AppState>() else {
                continue;
            };
            let now = (state.is_all_muted(), state.is_mic_muted());
            if shown == Some(now) {
                continue;
            }
            let Some(tray) = app.tray_by_id(TRAY_ID) else {
                return; // the tray is gone: the app is quitting
            };
            let (icon, tip) = match now {
                (true, _) => (Some(badged_icon(&base, true)), "Stellars — all channels muted"),
                (false, true) => (Some(badged_icon(&base, false)), "Stellars — microphone muted"),
                (false, false) => (Some(base.clone()), TOOLTIP_ACTIVE),
            };
            let _ = tray.set_icon(icon);
            let _ = tray.set_tooltip(Some(tip));
            shown = Some(now);
        }
    });
}

/// Asks an open tray menu to re-read the theme and app state and redraw (no-op when closed).
pub fn notify_menu_changed() {
    let raw = MENU_HWND.load(Ordering::Acquire);
    if raw != 0 {
        // SAFETY: `raw` is a live HWND; it is cleared in `WM_DESTROY` before the window goes away.
        unsafe {
            let _ = PostMessageW(HWND(raw as *mut c_void), WM_APP_REFRESH, WPARAM(0), LPARAM(0));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Menu content
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuAction {
    Open,
    MuteAll,
    MuteMic,
    NextScene,
    ResetEndpoints,
    RestoreAudio,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MenuItem {
    label: &'static str,
    hint: Option<&'static str>,
    action: MenuAction,
    danger: bool,
    disabled: bool,
}

/// What the menu shows; compared against a fresh snapshot to know when to redraw.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MenuModel {
    all_muted: bool,
    mic_muted: bool,
    items: [MenuItem; ITEM_COUNT],
}

const ITEM_COUNT: usize = 7;
/// Number of items in each group; groups are separated by a divider line.
const GROUPS: [usize; 4] = [1, 3, 2, 1];

fn build_model(all_muted: bool, mic_muted: bool, has_scenes: bool) -> MenuModel {
    let item = |label, hint, action| MenuItem { label, hint, action, danger: false, disabled: false };
    MenuModel {
        all_muted,
        mic_muted,
        items: [
            item("Open Stellars", Some("Left click"), MenuAction::Open),
            item(if all_muted { "Unmute all channels" } else { "Mute all channels" }, None, MenuAction::MuteAll),
            item(if mic_muted { "Unmute microphone" } else { "Mute microphone" }, None, MenuAction::MuteMic),
            MenuItem {
                disabled: !has_scenes,
                hint: if has_scenes { None } else { Some("None saved") },
                ..item("Next scene", None, MenuAction::NextScene)
            },
            item("Reset app audio endpoints", None, MenuAction::ResetEndpoints),
            item("Restore Windows audio", None, MenuAction::RestoreAudio),
            MenuItem { danger: true, ..item("Quit Stellars", None, MenuAction::Quit) },
        ],
    }
}

fn status_text(model: &MenuModel) -> Option<&'static str> {
    match (model.all_muted, model.mic_muted) {
        (true, true) => Some("All + mic muted"),
        (true, false) => Some("All muted"),
        (false, true) => Some("Mic muted"),
        (false, false) => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Theme: the same tokens as `.studio-shell` / `[data-theme="light"]` in src/studio.css
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
struct Palette {
    surface: [u8; 3],
    line: [u8; 3],
    line_strong: [u8; 3],
    control: [u8; 3],
    control_hover: [u8; 3],
    text: [u8; 3],
    text_secondary: [u8; 3],
    text_muted: [u8; 3],
    accent: [u8; 3],
    accent_soft_alpha: f32,
    danger: [u8; 3],
    danger_soft: [u8; 3],
}

fn palette_for(light: bool) -> Palette {
    if light {
        Palette {
            surface: [247, 246, 251],
            line: [230, 229, 235],
            line_strong: [213, 211, 220],
            control: [238, 234, 245],
            control_hover: [226, 221, 236],
            text: [25, 23, 36],
            text_secondary: [74, 69, 90],
            text_muted: [110, 105, 128],
            accent: [124, 77, 255],
            accent_soft_alpha: 0.16,
            danger: [196, 43, 49],
            danger_soft: [243, 218, 221],
        }
    } else {
        Palette {
            surface: [36, 35, 40],
            line: [54, 53, 58],
            line_strong: [71, 70, 74],
            control: [41, 40, 46],
            control_hover: [57, 53, 63],
            text: [244, 243, 247],
            text_secondary: [199, 197, 206],
            text_muted: [162, 161, 172],
            accent: [184, 161, 250],
            accent_soft_alpha: 0.2,
            danger: [229, 72, 77],
            danger_soft: [68, 40, 46],
        }
    }
}

fn system_prefers_light() -> bool {
    let mut val: u32 = 0;
    let mut size: u32 = std::mem::size_of::<u32>() as u32;
    // SAFETY: reads a DWORD from HKCU; the output pointers refer to stack variables of that size.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut val as *mut u32).cast::<c_void>()),
            Some(&mut size),
        )
    };
    err.0 == 0 && val != 0
}

fn resolve_palette(theme: ThemeMode) -> Palette {
    palette_for(match theme {
        ThemeMode::Light => true,
        ThemeMode::Dark => false,
        ThemeMode::System => system_prefers_light(),
    })
}

/// Reads the live state the menu depends on.
struct Snapshot {
    model: MenuModel,
    palette: Palette,
}

fn take_snapshot(app: &AppHandle) -> Snapshot {
    match app.try_state::<AppState>() {
        Some(state) => Snapshot {
            model: build_model(state.is_all_muted(), state.is_mic_muted(), !state.presets_snapshot().scenes.is_empty()),
            palette: resolve_palette(state.app_settings().theme),
        },
        None => Snapshot { model: build_model(false, false, false), palette: palette_for(false) },
    }
}

// ---------------------------------------------------------------------------------------------
// Layout (logical pixels, scaled per monitor)
// ---------------------------------------------------------------------------------------------

const LOGICAL_W: f32 = 252.0;
const LOGICAL_PAD: f32 = 6.0;
const LOGICAL_HEAD_H: f32 = 40.0;
const LOGICAL_SEP_H: f32 = 11.0;
const LOGICAL_ITEM_H: f32 = 34.0;
const LOGICAL_CARD_R: f32 = 14.0;
const LOGICAL_ITEM_R: f32 = 9.0;

#[derive(Debug, Clone, Copy)]
struct MenuLayout {
    w: i32,
    h: i32,
    scale: f32,
    dot_cx: f32,
    dot_cy: f32,
    head_rect: RECT,
    sep_rects: [RECT; GROUPS.len()],
    item_rects: [RECT; ITEM_COUNT],
}

fn compute_layout(scale: f32) -> MenuLayout {
    let px = |logical: f32| -> i32 { (logical * scale).round() as i32 };
    let total_h = LOGICAL_PAD * 2.0
        + LOGICAL_HEAD_H
        + GROUPS.len() as f32 * LOGICAL_SEP_H
        + ITEM_COUNT as f32 * LOGICAL_ITEM_H;
    let w = px(LOGICAL_W);
    let h = px(total_h);
    let head_top = LOGICAL_PAD;
    let head_rect = RECT {
        left: px(LOGICAL_PAD + 10.0 + 8.0 + 9.0),
        top: px(head_top),
        right: px(LOGICAL_W - LOGICAL_PAD - 10.0),
        bottom: px(head_top + LOGICAL_HEAD_H),
    };
    let dot_cx = (LOGICAL_PAD + 10.0 + 4.0) * scale;
    let dot_cy = (head_top + LOGICAL_HEAD_H * 0.5) * scale;

    let sep_thickness = (scale.round() as i32).max(1);
    let mut sep_rects = [RECT::default(); GROUPS.len()];
    let mut item_rects = [RECT::default(); ITEM_COUNT];
    let mut y = head_top + LOGICAL_HEAD_H;
    let mut idx = 0usize;
    for (g, count) in GROUPS.iter().enumerate() {
        let sep_top = px(y + (LOGICAL_SEP_H - 1.0) * 0.5);
        sep_rects[g] = RECT {
            left: px(LOGICAL_PAD + 8.0),
            top: sep_top,
            right: w - px(LOGICAL_PAD + 8.0),
            bottom: sep_top + sep_thickness,
        };
        y += LOGICAL_SEP_H;
        for _ in 0..*count {
            item_rects[idx] = RECT {
                left: px(LOGICAL_PAD),
                top: px(y),
                right: w - px(LOGICAL_PAD),
                bottom: px(y + LOGICAL_ITEM_H),
            };
            y += LOGICAL_ITEM_H;
            idx += 1;
        }
    }
    MenuLayout { w, h, scale, dot_cx, dot_cy, head_rect, sep_rects, item_rects }
}

fn hit_item(layout: &MenuLayout, x: i32, y: i32) -> Option<usize> {
    layout.item_rects.iter().position(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
}

/// Next enabled item in direction `dir`, wrapping; `-1` when none is enabled.
fn step_index(items: &[MenuItem], current: isize, dir: isize) -> isize {
    let n = items.len() as isize;
    let mut i = if current < 0 && dir < 0 { 0 } else { current };
    for _ in 0..n {
        i = (i + dir).rem_euclid(n);
        if !items[i as usize].disabled {
            return i;
        }
    }
    -1
}

// ---------------------------------------------------------------------------------------------
// Software rendering into a premultiplied BGRA DIB
// ---------------------------------------------------------------------------------------------

const fn colorref(rgb: [u8; 3]) -> COLORREF {
    COLORREF((rgb[0] as u32) | ((rgb[1] as u32) << 8) | ((rgb[2] as u32) << 16))
}

fn blend_pixel(dst: &mut [u8], col: [u8; 3], cov: f32) {
    let a = cov.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let inv = 1.0 - a;
    dst[0] = (f32::from(dst[0]) * inv + f32::from(col[2]) * a).round() as u8;
    dst[1] = (f32::from(dst[1]) * inv + f32::from(col[1]) * a).round() as u8;
    dst[2] = (f32::from(dst[2]) * inv + f32::from(col[0]) * a).round() as u8;
    dst[3] = 255;
}

fn rounded_box_sd(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (px - cx).abs() - (hw - r);
    let qy = (py - cy).abs() - (hh - r);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.min(0.0).max(qy.min(0.0)) - r
}

fn fill_rounded_rect(bits: &mut [u8], w: i32, h: i32, r: RECT, radius: f32, col: [u8; 3]) {
    let (x0, y0, x1, y1) = (r.left.max(0), r.top.max(0), r.right.min(w), r.bottom.min(h));
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let cx = (r.left + r.right) as f32 * 0.5;
    let cy = (r.top + r.bottom) as f32 * 0.5;
    let hw = (r.right - r.left) as f32 * 0.5;
    let hh = (r.bottom - r.top) as f32 * 0.5;
    let rad = radius.min(hw).min(hh);
    for y in y0..y1 {
        let py = y as f32 + 0.5;
        for x in x0..x1 {
            let d = rounded_box_sd(x as f32 + 0.5, py, cx, cy, hw, hh, rad);
            let cov = (0.5 - d).clamp(0.0, 1.0);
            if cov > 0.0 {
                let idx = ((y * w + x) as usize) * 4;
                blend_pixel(&mut bits[idx..idx + 4], col, cov);
            }
        }
    }
}

fn fill_circle(bits: &mut [u8], w: i32, h: i32, center: (f32, f32), radius: f32, col: [u8; 3], opacity: f32) {
    if opacity <= 0.0 || radius <= 0.0 {
        return;
    }
    let (cx, cy) = center;
    let x0 = (cx - radius - 1.0).floor().max(0.0) as i32;
    let y0 = (cy - radius - 1.0).floor().max(0.0) as i32;
    let x1 = (cx + radius + 1.0).ceil().min(w as f32) as i32;
    let y1 = (cy + radius + 1.0).ceil().min(h as f32) as i32;
    for y in y0..y1 {
        let dy = (y as f32 + 0.5) - cy;
        for x in x0..x1 {
            let dx = (x as f32 + 0.5) - cx;
            let cov = (0.5 - (dx.hypot(dy) - radius)).clamp(0.0, 1.0) * opacity;
            if cov > 0.0 {
                let idx = ((y * w + x) as usize) * 4;
                blend_pixel(&mut bits[idx..idx + 4], col, cov);
            }
        }
    }
}

/// Rounds the outer corners and draws the 1px border. The interior stays fully opaque so
/// ClearType text keeps its sub-pixel colours.
fn apply_card_mask_and_border(bits: &mut [u8], w: i32, h: i32, radius: f32, bw: f32, border_col: [u8; 3]) {
    let hw = w as f32 * 0.5;
    let hh = h as f32 * 0.5;
    let rad = radius.min(hw).min(hh);
    let edge_threshold = -bw - 0.5;
    for y in 0..h {
        let py = y as f32 + 0.5;
        for x in 0..w {
            let d = rounded_box_sd(x as f32 + 0.5, py, hw, hh, hw, hh, rad);
            let idx = ((y * w + x) as usize) * 4;
            if d <= edge_threshold {
                bits[idx + 3] = 255;
            } else {
                let cov_outer = (0.5 - d).clamp(0.0, 1.0);
                if cov_outer <= 0.0 {
                    bits[idx..idx + 4].fill(0);
                } else {
                    let cov_inner = (d + bw + 0.5).clamp(0.0, 1.0);
                    let inv_b = 1.0 - cov_inner;
                    let b = f32::from(bits[idx]) * inv_b + f32::from(border_col[2]) * cov_inner;
                    let g = f32::from(bits[idx + 1]) * inv_b + f32::from(border_col[1]) * cov_inner;
                    let r = f32::from(bits[idx + 2]) * inv_b + f32::from(border_col[0]) * cov_inner;
                    bits[idx] = (b * cov_outer).round() as u8;
                    bits[idx + 1] = (g * cov_outer).round() as u8;
                    bits[idx + 2] = (r * cov_outer).round() as u8;
                    bits[idx + 3] = (cov_outer * 255.0).round() as u8;
                }
            }
        }
    }
}

struct LayeredCanvas {
    w: i32,
    h: i32,
    mem_dc: HDC,
    bmp: HBITMAP,
    old_obj: HGDIOBJ,
    bits: *mut u8,
    font_head: HFONT,
    font_item: HFONT,
    font_hint: HFONT,
}

fn create_ui_font(logical_px: f32, scale: f32, weight: i32) -> HFONT {
    // SAFETY: static face name literal and valid GDI font attributes.
    unsafe {
        CreateFontW(
            -((logical_px * scale).round() as i32),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            (DEFAULT_PITCH.0 | FF_SWISS.0) as u32,
            w!("Segoe UI Variable Text"),
        )
    }
}

impl LayeredCanvas {
    fn new(w: i32, h: i32, scale: f32) -> Option<Self> {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: creates an off-screen memory DC and 32-bit DIB section owned by `Self`.
        unsafe {
            let mem_dc = CreateCompatibleDC(HDC::default());
            if mem_dc.is_invalid() {
                return None;
            }
            let bmp = match CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, HANDLE::default(), 0) {
                Ok(b) if !bits.is_null() => b,
                _ => {
                    let _ = DeleteDC(mem_dc);
                    return None;
                }
            };
            let old_obj = SelectObject(mem_dc, HGDIOBJ(bmp.0));
            SetBkMode(mem_dc, TRANSPARENT);
            Some(Self {
                w,
                h,
                mem_dc,
                bmp,
                old_obj,
                bits: bits.cast::<u8>(),
                font_head: create_ui_font(13.0, scale, 700),
                font_item: create_ui_font(12.5, scale, 500),
                font_hint: create_ui_font(11.5, scale, 400),
            })
        }
    }

    fn bits_mut(&mut self) -> &mut [u8] {
        let len = (self.w as usize) * (self.h as usize) * 4;
        // SAFETY: `self.bits` points to `w * h * 4` bytes that live as long as `self.bmp`.
        unsafe { std::slice::from_raw_parts_mut(self.bits, len) }
    }

    fn draw_text(&self, font: HFONT, col: [u8; 3], text: &str, rect: RECT, right_align: bool) {
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        if wide.is_empty() {
            return;
        }
        let mut r = rect;
        let align = if right_align { DT_RIGHT } else { DT_LEFT };
        // SAFETY: `mem_dc` has a valid selected DIB; `wide` and `r` outlive the call.
        unsafe {
            let prev = SelectObject(self.mem_dc, HGDIOBJ(font.0));
            SetTextColor(self.mem_dc, colorref(col));
            DrawTextW(self.mem_dc, &mut wide, &mut r, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | align);
            SelectObject(self.mem_dc, prev);
        }
    }
}

impl Drop for LayeredCanvas {
    fn drop(&mut self) {
        // SAFETY: releases the GDI handles allocated in `new`.
        unsafe {
            SelectObject(self.mem_dc, self.old_obj);
            let _ = DeleteObject(HGDIOBJ(self.font_head.0));
            let _ = DeleteObject(HGDIOBJ(self.font_item.0));
            let _ = DeleteObject(HGDIOBJ(self.font_hint.0));
            let _ = DeleteObject(HGDIOBJ(self.bmp.0));
            let _ = DeleteDC(self.mem_dc);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The menu window
// ---------------------------------------------------------------------------------------------

struct MenuWindowState {
    app: AppHandle,
    hwnd: HWND,
    pos: (i32, i32),
    layout: MenuLayout,
    canvas: LayeredCanvas,
    palette: Palette,
    model: MenuModel,
    active: isize,
    pressed: bool,
    mouse_tracked: bool,
    saw_active: bool,
    was_l_down: bool,
    was_r_down: bool,
    reduced_motion: bool,
    open_at: Instant,
    open_done: bool,
    ticks: u32,
    chosen: Option<MenuAction>,
}

fn reduced_motion_enabled() -> bool {
    let mut anim: i32 = 1;
    // SAFETY: queries `SPI_GETCLIENTAREAANIMATION` into an `i32` (a `BOOL`).
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut anim as *mut i32).cast::<c_void>()),
            Default::default(),
        )
    };
    ok.is_ok() && anim == 0
}

/// Re-reads theme and app state; returns true when anything visible changed.
fn refresh_snapshot(st: &mut MenuWindowState) -> bool {
    let snap = take_snapshot(&st.app);
    let changed = snap.model != st.model || snap.palette != st.palette;
    st.model = snap.model;
    st.palette = snap.palette;
    if st.active >= 0 && st.model.items[st.active as usize].disabled {
        st.active = -1;
        st.pressed = false;
    }
    changed
}

fn render_and_present(st: &mut MenuWindowState) {
    let p = st.palette;
    let layout = st.layout;
    let scale = layout.scale;
    let (w, h) = (layout.w, layout.h);

    let open_alpha: u8 = if st.reduced_motion || st.open_done {
        255
    } else {
        let t = (st.open_at.elapsed().as_secs_f32() / 0.14).clamp(0.0, 1.0);
        if t >= 1.0 {
            st.open_done = true;
            255
        } else {
            ((1.0 - (1.0 - t).powi(3)) * 255.0).round() as u8
        }
    };

    let bg_px = [p.surface[2], p.surface[1], p.surface[0], 255u8];
    let status = status_text(&st.model);
    {
        let bits = st.canvas.bits_mut();
        for px in bits.chunks_exact_mut(4) {
            px.copy_from_slice(&bg_px);
        }

        // Status dot with a slow pulse while the engine is live and nothing is muted.
        let dot_r = 4.0 * scale;
        let dot_center = (layout.dot_cx, layout.dot_cy);
        if status.is_none() {
            if !st.reduced_motion {
                let phase = (st.open_at.elapsed().as_secs_f32() % 2.4) / 2.4;
                if phase < 0.7 {
                    let t = phase / 0.7;
                    let eased = 1.0 - (1.0 - t).powi(2);
                    fill_circle(bits, w, h, dot_center, dot_r + 6.0 * scale * eased, p.accent, p.accent_soft_alpha * (1.0 - eased));
                }
            }
            fill_circle(bits, w, h, dot_center, dot_r, p.accent, 1.0);
        } else {
            fill_circle(bits, w, h, dot_center, dot_r, p.text_muted, 1.0);
        }

        // Group dividers.
        for sep in &layout.sep_rects {
            for y in sep.top.max(0)..sep.bottom.min(h) {
                for x in sep.left.max(0)..sep.right.min(w) {
                    let idx = ((y * w + x) as usize) * 4;
                    blend_pixel(&mut bits[idx..idx + 4], p.line, 1.0);
                }
            }
        }

        // Hover / pressed pill.
        if st.active >= 0 {
            let idx = st.active as usize;
            let item = &st.model.items[idx];
            if !item.disabled {
                let mut r = layout.item_rects[idx];
                if st.pressed {
                    let inset = (1.5 * scale).round() as i32;
                    r.left += inset;
                    r.right -= inset;
                    r.top += (0.5 * scale).round() as i32;
                    r.bottom -= (0.5 * scale).round() as i32;
                }
                let col = if item.danger {
                    p.danger_soft
                } else if st.pressed {
                    p.control_hover
                } else {
                    p.control
                };
                fill_rounded_rect(bits, w, h, r, LOGICAL_ITEM_R * scale, col);
            }
        }
    }

    // Text pass (GDI ClearType).
    let pad_x = (10.0 * scale).round() as i32;
    st.canvas.draw_text(st.canvas.font_head, p.text, "Stellars", layout.head_rect, false);
    if let Some(text) = status {
        st.canvas.draw_text(st.canvas.font_hint, p.accent, text, layout.head_rect, true);
    }
    for (i, item) in st.model.items.iter().enumerate() {
        let base = layout.item_rects[i];
        let r = RECT { left: base.left + pad_x, top: base.top, right: base.right - pad_x, bottom: base.bottom };
        let active = st.active == i as isize && !item.disabled;
        let label_col = if item.disabled {
            p.text_muted
        } else if item.danger && active {
            p.danger
        } else if active {
            p.text
        } else {
            p.text_secondary
        };
        st.canvas.draw_text(st.canvas.font_item, label_col, item.label, r, false);
        if let Some(hint) = item.hint {
            st.canvas.draw_text(st.canvas.font_hint, p.text_muted, hint, r, true);
        }
    }

    // SAFETY: flushes GDI text writes before the pixel pass below.
    unsafe {
        let _ = GdiFlush();
    }
    apply_card_mask_and_border(st.canvas.bits_mut(), w, h, LOGICAL_CARD_R * scale, scale.max(1.0), p.line_strong);

    let pt_dst = POINT { x: st.pos.0, y: st.pos.1 };
    let sz = SIZE { cx: w, cy: h };
    let pt_src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: open_alpha,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    // SAFETY: `hwnd` and the memory DC are valid on this owning thread.
    unsafe {
        let _ = UpdateLayeredWindow(
            st.hwnd,
            HDC::default(),
            Some(&pt_dst),
            Some(&sz),
            st.canvas.mem_dc,
            Some(&pt_src),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
    }
}

fn dismiss_on_blur(hwnd: HWND) {
    LAST_MENU_CLOSE.store(now_ms(), Ordering::SeqCst);
    // SAFETY: `hwnd` is owned by the calling menu thread.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

fn register_menu_class() -> bool {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    *REGISTERED.get_or_init(|| {
        // SAFETY: registers a process-local window class once.
        unsafe {
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(menu_wnd_proc),
                hCursor: LoadCursorW(HINSTANCE::default(), IDC_ARROW).unwrap_or_default(),
                lpszClassName: w!("StellarsTrayMenu"),
                ..Default::default()
            };
            RegisterClassExW(&wc) != 0
        }
    })
}

fn lparam_xy(lparam: LPARAM) -> (i32, i32) {
    ((lparam.0 & 0xFFFF) as i16 as i32, ((lparam.0 >> 16) & 0xFFFF) as i16 as i32)
}

unsafe extern "system" fn menu_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_NCHITTEST {
        return LRESULT(HTCLIENT as isize);
    }
    if msg == WM_SETCURSOR {
        // SAFETY: loads the shared system arrow cursor.
        unsafe {
            if let Ok(c) = LoadCursorW(HINSTANCE::default(), IDC_ARROW) {
                SetCursor(c);
            }
        }
        return LRESULT(1);
    }

    // SAFETY: the pointer is a heap `MenuWindowState` valid until `WM_DESTROY` clears it.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut MenuWindowState;
    if ptr.is_null() {
        // SAFETY: default handling before the state is attached or after it is cleared.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    // SAFETY: only touched from this window's own message loop thread, one message at a time.
    let st = unsafe { &mut *ptr };

    match msg {
        WM_MOUSEMOVE => {
            if !st.mouse_tracked {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                // SAFETY: `tme` describes `hwnd`.
                st.mouse_tracked = unsafe { TrackMouseEvent(&mut tme).is_ok() };
            }
            let (x, y) = lparam_xy(lparam);
            let next = hit_item(&st.layout, x, y)
                .filter(|&i| !st.model.items[i].disabled)
                .map_or(-1, |i| i as isize);
            if next != st.active {
                st.active = next;
                render_and_present(st);
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            st.mouse_tracked = false;
            if st.active != -1 || st.pressed {
                st.active = -1;
                st.pressed = false;
                render_and_present(st);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = lparam_xy(lparam);
            if let Some(i) = hit_item(&st.layout, x, y).filter(|&i| !st.model.items[i].disabled) {
                st.active = i as isize;
                st.pressed = true;
                render_and_present(st);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = lparam_xy(lparam);
            let was_pressed = st.pressed;
            st.pressed = false;
            if was_pressed
                && hit_item(&st.layout, x, y).is_some_and(|i| i as isize == st.active && !st.model.items[i].disabled)
            {
                st.chosen = Some(st.model.items[st.active as usize].action);
                // SAFETY: closes the menu after a choice.
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            } else if was_pressed {
                render_and_present(st);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let key = wparam.0 as u16;
            if key == VK_ESCAPE.0 {
                // SAFETY: closes the menu on Escape.
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            } else if key == VK_DOWN.0 {
                st.active = step_index(&st.model.items, st.active, 1);
                render_and_present(st);
            } else if key == VK_UP.0 {
                st.active = step_index(&st.model.items, st.active, -1);
                render_and_present(st);
            } else if (key == VK_RETURN.0 || key == VK_SPACE.0) && st.active >= 0 {
                let item = &st.model.items[st.active as usize];
                if !item.disabled {
                    st.chosen = Some(item.action);
                    // SAFETY: closes the menu after activating an item.
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wparam.0 & 0xFFFF) as u32 != WA_INACTIVE {
                st.saw_active = true;
            } else if st.saw_active || st.open_at.elapsed() >= Duration::from_millis(80) {
                dismiss_on_blur(hwnd);
            }
            LRESULT(0)
        }
        WM_KILLFOCUS => {
            if st.saw_active || st.open_at.elapsed() >= Duration::from_millis(80) {
                dismiss_on_blur(hwnd);
            }
            LRESULT(0)
        }
        WM_SETTINGCHANGE | WM_APP_REFRESH => {
            refresh_snapshot(st);
            render_and_present(st);
            LRESULT(0)
        }
        WM_TIMER => {
            // Outside clicks and lost foreground also close the menu (covers the Windows 11 tray
            // overflow flyout, which does not always deactivate this window).
            if st.open_at.elapsed() >= Duration::from_millis(60) {
                // SAFETY: reads global mouse state, cursor position and this window's rectangle.
                let (l_down, r_down, pt, rc) = unsafe {
                    let l = (GetAsyncKeyState(i32::from(VK_LBUTTON.0)) as u16 & 0x8000) != 0;
                    let r = (GetAsyncKeyState(i32::from(VK_RBUTTON.0)) as u16 & 0x8000) != 0;
                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);
                    let mut rc = RECT::default();
                    let _ = GetWindowRect(hwnd, &mut rc);
                    (l, r, pt, rc)
                };
                let clicked = (l_down && !st.was_l_down) || (r_down && !st.was_r_down);
                st.was_l_down = l_down;
                st.was_r_down = r_down;
                let inside = pt.x >= rc.left && pt.x < rc.right && pt.y >= rc.top && pt.y < rc.bottom;
                if clicked && !inside {
                    dismiss_on_blur(hwnd);
                    return LRESULT(0);
                }
            }
            // SAFETY: reads the current foreground window.
            let fg = unsafe { GetForegroundWindow() };
            if (st.saw_active || st.open_at.elapsed() >= Duration::from_millis(180)) && fg != hwnd {
                dismiss_on_blur(hwnd);
                return LRESULT(0);
            }
            st.ticks = st.ticks.wrapping_add(1);
            // Pick up changes made elsewhere (the window, another tray action) about twice a second.
            let changed = st.ticks % 20 == 0 && refresh_snapshot(st);
            let animating = !st.reduced_motion
                && (!st.open_done || status_text(&st.model).is_none());
            if changed || animating {
                render_and_present(st);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // SAFETY: closes the menu when the tray icon is clicked again.
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            MENU_HWND.store(0, Ordering::Release);
            // SAFETY: detaches the state pointer, stops the timer and ends the message loop.
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let _ = KillTimer(hwnd, TIMER_ID);
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

// ---------------------------------------------------------------------------------------------
// Opening the menu and running the chosen action
// ---------------------------------------------------------------------------------------------

fn execute_action(app: &AppHandle, action: MenuAction) {
    match action {
        MenuAction::Open => crate::show_or_recreate_main_window(app),
        MenuAction::ResetEndpoints => {
            let _ = crate::interop::policy::clear_all_persisted_endpoints();
        }
        MenuAction::MuteAll => {
            if let Some(state) = app.try_state::<AppState>() {
                let muted = state.toggle_mute_all();
                logging::info(if muted { "Tray: all channels muted" } else { "Tray: channel mutes restored" });
                let _ = app.emit("config-changed", ());
            }
        }
        MenuAction::MuteMic => {
            if let Some(state) = app.try_state::<AppState>() {
                let muted = state.toggle_mic_mute();
                logging::info(if muted { "Tray: microphone muted" } else { "Tray: microphone unmuted" });
                let _ = app.emit("config-changed", ());
            }
        }
        MenuAction::NextScene => {
            if let Some(state) = app.try_state::<AppState>() {
                match state.next_scene() {
                    Some(name) => {
                        logging::info(&format!("Tray: scene switched to {name}"));
                        let _ = app.emit("scene-applied", serde_json::json!({ "id": "", "name": name }));
                    }
                    None => logging::info("Tray: no scene to switch to"),
                }
            }
        }
        MenuAction::RestoreAudio => {
            if let Some(state) = app.try_state::<AppState>() {
                state.restore_windows_audio();
                let _ = app.emit("audio-restored", ());
            }
        }
        MenuAction::Quit => crate::quit_application(app),
    }
}

struct MenuOpenGuard;
impl Drop for MenuOpenGuard {
    fn drop(&mut self) {
        MENU_HWND.store(0, Ordering::Release);
        MENU_OPEN.store(false, Ordering::Release);
    }
}

fn open_menu(app: &AppHandle, icon_rect: tauri::Rect, cursor: PhysicalPosition<f64>) {
    let raw = MENU_HWND.load(Ordering::Acquire);
    if raw != 0 {
        // Clicking the icon again closes the open menu.
        // SAFETY: `raw` is a live HWND cleared in `WM_DESTROY`.
        unsafe {
            let _ = PostMessageW(HWND(raw as *mut c_void), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        return;
    }
    // The click that dismissed the menu by blur must not reopen it.
    if now_ms().saturating_sub(LAST_MENU_CLOSE.load(Ordering::SeqCst)) < REOPEN_SUPPRESS_MS {
        return;
    }
    if MENU_OPEN.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
        return;
    }
    let pos = icon_rect.position.to_physical::<f64>(1.0);
    let size = icon_rect.size.to_physical::<f64>(1.0);
    let icon = if size.width > 0.0 && size.height > 0.0 {
        Area { x: pos.x, y: pos.y, w: size.width, h: size.height }
    } else {
        Area { x: cursor.x, y: cursor.y, w: 1.0, h: 1.0 }
    };
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("tray-menu".into()).spawn({
        let app = app.clone();
        move || {
            let guard = MenuOpenGuard;
            let chosen = run_native_menu(&app, icon, (cursor.x, cursor.y));
            drop(guard);
            if let Some(action) = chosen {
                execute_action(&app, action);
            }
        }
    });
    if spawned.is_err() {
        MENU_OPEN.store(false, Ordering::Release);
        logging::warn("Tray menu thread could not be started");
    }
}

fn run_native_menu(app: &AppHandle, icon: Area, cursor: (f64, f64)) -> Option<MenuAction> {
    if !register_menu_class() {
        logging::warn("Tray menu window class could not be registered");
        return None;
    }
    let monitor = app
        .monitor_from_point(cursor.0, cursor.1)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
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

    let layout = compute_layout(scale as f32);
    let (x, y) = place_popup(
        icon,
        monitor_area,
        work_area,
        (layout.w as f64, layout.h as f64),
        ANCHOR_GAP * scale,
        EDGE_MARGIN * scale,
    );
    let pos = (x.round() as i32, y.round() as i32);
    let canvas = LayeredCanvas::new(layout.w, layout.h, scale as f32)?;
    let snap = take_snapshot(app);

    let mut boxed = Box::new(MenuWindowState {
        app: app.clone(),
        hwnd: HWND::default(),
        pos,
        layout,
        canvas,
        palette: snap.palette,
        model: snap.model,
        active: -1,
        pressed: false,
        mouse_tracked: false,
        saw_active: false,
        was_l_down: false,
        was_r_down: false,
        reduced_motion: reduced_motion_enabled(),
        open_at: Instant::now(),
        open_done: false,
        ticks: 0,
        chosen: None,
    });

    // SAFETY: creates a layered popup window owned by this thread.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            w!("StellarsTrayMenu"),
            w!("Stellars tray menu"),
            WS_POPUP,
            pos.0,
            pos.1,
            layout.w,
            layout.h,
            HWND::default(),
            HMENU::default(),
            HINSTANCE::default(),
            None,
        )
    }
    .ok()?;

    boxed.hwnd = hwnd;
    MENU_HWND.store(hwnd.0 as isize, Ordering::Release);
    let state_ptr = Box::into_raw(boxed);
    // SAFETY: attaches the state to `hwnd` before the window is shown or the loop runs; the
    // first frame is drawn before `ShowWindow` so there is no wrong-theme flash.
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        render_and_present(&mut *state_ptr);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        SetTimer(hwnd, TIMER_ID, 25, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        Box::from_raw(state_ptr).chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: Area = Area { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 };
    const WORK_BOTTOM_TASKBAR: Area = Area { x: 0.0, y: 0.0, w: 1920.0, h: 1040.0 };
    const SIZE: (f64, f64) = (240.0, 211.0);

    #[test]
    fn keyboard_steps_skip_disabled_items_and_wrap() {
        let model = build_model(false, false, false);
        let scene = model.items.iter().position(|i| i.action == MenuAction::NextScene).unwrap() as isize;
        assert!(model.items[scene as usize].disabled);
        assert_eq!(step_index(&model.items, scene - 1, 1), scene + 1);
        assert_eq!(step_index(&model.items, ITEM_COUNT as isize - 1, 1), 0);
        assert_eq!(step_index(&model.items, 0, -1), ITEM_COUNT as isize - 1);
    }

    #[test]
    fn layout_has_one_row_per_item_inside_the_card() {
        for scale in [1.0, 1.5, 2.0] {
            let l = compute_layout(scale);
            assert_eq!(GROUPS.iter().sum::<usize>(), ITEM_COUNT);
            for r in &l.item_rects {
                assert!(r.left >= 0 && r.right <= l.w && r.top >= 0 && r.bottom <= l.h);
            }
        }
    }

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
