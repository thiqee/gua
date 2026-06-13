// KeyHop — 数据结构、常量、工具函数

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config;

// ── constants ──────────────────────────────────────────────────

pub const WW: i32 = 420;
pub const PD: i32 = 6;
pub const GP: i32 = 2;
pub const MV: usize = 8;
pub const FW: f32 = 18.0;
pub const CONFIG_FILE: &str = "config.toml";

pub const HOTKEY_ID: i32 = 1;
pub const TRAY_MSG: u32 = WM_APP + 256;
pub const TRAY_ID: u32 = 1;
pub const IDM_TOGGLE: u16 = 100;
pub const IDM_OPEN_CONFIG: u16 = 101;
pub const IDM_EXIT: u16 = 102;
pub const MOD_ALT: u32 = 1;
pub const VK_SPACE: u32 = 0x20;
pub const VK_ESCAPE: u32 = 0x1B;
pub const VK_RETURN: u32 = 0x0D;

#[link(name = "user32")]
extern "system" {
    pub fn RegisterHotKey(hwnd: HWND, id: i32, fs_modifiers: u32, vk: u32) -> BOOL;
    pub fn SetFocus(hwnd: HWND) -> HWND;
}

#[repr(C)]
#[allow(non_snake_case)]
pub struct COMPOSITIONFORM {
    pub dwStyle: u32,
    pub ptCurrentPos: POINT,
    pub rcArea: RECT,
}

pub const CFS_FORCE_POSITION: u32 = 0x0020;
pub const ISC_SHOWUICOMPOSITIONWINDOW: u32 = 0x80000000;
pub const GCS_COMPSTR: u32 = 0x0008;
pub const GCS_RESULTSTR: u32 = 0x0800;

// ── helpers ─────────────────────────────────────────────────────

pub unsafe fn hiword(d: u32) -> u32 { (d >> 16) & 0xFFFF }

pub fn cfg_str(entries: &[config::Entry], key: &str, default: &str) -> String {
    entries.iter().find(|e| e.key == key).map(|e| e.value.clone()).unwrap_or_else(|| default.to_string())
}
pub fn cfg_f32(entries: &[config::Entry], key: &str, default: f32) -> f32 {
    entries.iter().find(|e| e.key == key).and_then(|e| e.value.parse().ok()).unwrap_or(default)
}
pub fn cfg_i32(entries: &[config::Entry], key: &str, default: i32) -> i32 {
    entries.iter().find(|e| e.key == key).and_then(|e| e.value.parse().ok()).unwrap_or(default)
}
pub fn cfg_usize(entries: &[config::Entry], key: &str, default: usize) -> usize {
    entries.iter().find(|e| e.key == key).and_then(|e| e.value.parse().ok()).unwrap_or(default)
}
pub fn cfg_bool(entries: &[config::Entry], key: &str, default: bool) -> bool {
    entries.iter().find(|e| e.key == key).map(|e| e.value == "true" || e.value == "1").unwrap_or(default)
}
pub fn cfg_color(entries: &[config::Entry], key: &str, default: u32) -> u32 {
    entries.iter()
        .find(|e| e.key == key)
        .and_then(|e| u32::from_str_radix(&e.value, 16).ok())
        .unwrap_or(default)
}
pub fn colorref(rgb: u32) -> COLORREF {
    let r = (rgb >> 16) & 0xFF;
    let g = (rgb >> 8) & 0xFF;
    let b = rgb & 0xFF;
    COLORREF((b << 16) | (g << 8) | r)
}

pub fn to_w(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

pub fn pcwstr(v: &[u16]) -> PCWSTR {
    PCWSTR(v.as_ptr())
}

pub fn font_px(size: f32, dpi: i32) -> i32 {
    (size * dpi as f32 / 96.0) as i32
}

pub fn status_bar_h(dpi: i32, sz: f32) -> i32 {
    font_px(sz, dpi) + 8
}

pub fn list_y(input_rect: &RECT) -> i32 {
    input_rect.bottom + GP
}

pub fn win_h(count: usize, item_h: i32, eh: i32, max_results: usize, status_h: i32) -> i32 {
    let visible = count.min(max_results);
    PD + eh + GP + visible as i32 * item_h + PD + if count > 0 { status_h } else { 0 }
}

pub fn entry_type(val: &str) -> &'static str {
    if val.ends_with(".exe") {
        "\u{7A0B}\u{5E8F}"
    } else if val.starts_with("http://") || val.starts_with("https://") {
        if val.contains('?') {
            "\u{641C}\u{7D22}"
        } else {
            "\u{7F51}\u{5740}"
        }
    } else if std::path::Path::new(val).is_dir() {
        "\u{6587}\u{4EF6}\u{5939}"
    } else if std::path::Path::new(val).is_file() {
        "\u{6587}\u{4EF6}"
    } else {
        "\u{5176}\u{4ED6}"
    }
}

pub unsafe fn round_win(h: HWND, w: i32, hh: i32, corner: i32) {
    let rgn = CreateRoundRectRgn(0, 0, w, hh, corner, corner);
    if !rgn.is_invalid() {
        SetWindowRgn(h, Some(rgn), true);
    }
}

/// 将窗口定位到工作区指定位置
/// ratio_x, ratio_y: 0.0~1.0，0.0=左上角 0.5=居中 1.0=右下角
pub unsafe fn center_win(h: HWND, w: i32, hh: i32, ratio_x: f32, ratio_y: f32) {
    let mon = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(mon, &mut mi).as_bool() {
        let rc = mi.rcWork;
        let x = rc.left + ((rc.right - rc.left - w) as f32 * ratio_x) as i32;
        let y = rc.top + ((rc.bottom - rc.top - hh) as f32 * ratio_y) as i32;
        SetWindowPos(h, Some(HWND_TOP), x, y, w, hh, SWP_NOZORDER);
    }
}

// ── app state ───────────────────────────────────────────────────

pub struct AppState {
    pub entries: Vec<config::Entry>,
    pub filter: String,
    pub input_text: String,
    pub cursor_pos: usize,
    pub search_query: String,
    pub filtered_indices: Vec<usize>,
    pub sel_index: usize,
    pub scroll_offset: usize,
    pub input_rect: RECT,
    pub visible: bool,
    pub hfont: Option<HFONT>,
    pub status_hfont: Option<HFONT>,
    pub status_font_size: f32,
    pub font_name: String,
    pub font_size: f32,
    pub item_h: i32,
    pub eh: i32,
    pub dpi: i32,
    pub max_results: usize,
    pub width: i32,
    pub round_corner: i32,
    pub always_on_top: bool,
    pub opacity: u8,
    pub case_sensitive: bool,
    pub hide_on_focus_loss: bool,
    pub theme_color: u32,
    pub input_bg_color: u32,
    pub accent_color: u32,
    pub text_color: u32,
    pub composing: String,
    pub config_mtime: Option<std::time::SystemTime>,
    /// 面板水平位置比例 0.0~1.0，由 _panel_position_x 配置计算
    pub panel_ratio_x: f32,
    /// 面板垂直位置比例 0.0~1.0，由 _panel_position_y 配置计算
    pub panel_ratio_y: f32,
}

pub unsafe fn make_font_with(dpi: i32, name: &str, size: f32) -> Result<HFONT> {
    let sz = -((size as i32 * dpi / 96) as i32);
    let pitch = FONT_PITCH(DEFAULT_PITCH.0 | FF_DONTCARE.0);
    let ws = to_w(name);
    let font = CreateFontW(
        sz, 0, 0, 0,
        FW_NORMAL.0 as i32,
        0, 0, 0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        FONT_QUALITY(5),
        pitch.0 as u32,
        PCWSTR(ws.as_ptr()),
    );
    if font.0.is_null() {
        return Err(Error::empty());
    }
    Ok(font)
}
