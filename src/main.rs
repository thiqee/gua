// KeyHop — Windows 桌面搜索启动器
// 纯 Win32 API，自绘列表，无闪烁

#![cfg(target_os = "windows")]
#![allow(unused_must_use)]

use std::io::Write;

mod config;
mod executor;
mod tray;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::GdiPlus::{
    GdiplusStartup, GdiplusShutdown, GdiplusStartupInput as GpStartupInput,
    GdipCreateFromHDC, GdipCreateSolidFill,
    GdipCreatePath, GdipAddPathArc,
    GdipClosePathFigure, GdipFillPath, GdipDeletePath,
    GdipDeleteBrush, GdipDeleteGraphics,
    GdipSetSmoothingMode,
    GpGraphics, GpSolidFill, GpBrush, GpPath, FillModeAlternate,
    SmoothingMode,
};

use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// ── GDI+ 辅助 ──

unsafe fn fill_round_rect(hdc: HDC, x: i32, y: i32, w: i32, h: i32, r: i32, argb: u32) {
    let mut g: *mut GpGraphics = std::ptr::null_mut();
    if GdipCreateFromHDC(hdc, &mut g).0 != 0 || g.is_null() { return; }
    let mut b: *mut GpSolidFill = std::ptr::null_mut();
    if GdipCreateSolidFill(argb, &mut b).0 != 0 || b.is_null() {
        GdipDeleteGraphics(g);
        return;
    }
    let mut path: *mut GpPath = std::ptr::null_mut();
    if GdipCreatePath(FillModeAlternate, &mut path).0 != 0 || path.is_null() {
        GdipDeleteBrush(b as *mut GpBrush);
        GdipDeleteGraphics(g);
        return;
    }
    let fx = x as f32; let fy = y as f32;
    let fw = w as f32; let fh = h as f32; let fr = r as f32;
    GdipSetSmoothingMode(g, SmoothingMode(4));
    GdipAddPathArc(path, fx, fy, fr * 2.0, fr * 2.0, 180.0, 90.0);
    GdipAddPathArc(path, fx + fw - fr * 2.0, fy, fr * 2.0, fr * 2.0, 270.0, 90.0);
    GdipAddPathArc(path, fx + fw - fr * 2.0, fy + fh - fr * 2.0, fr * 2.0, fr * 2.0, 0.0, 90.0);
    GdipAddPathArc(path, fx, fy + fh - fr * 2.0, fr * 2.0, fr * 2.0, 90.0, 90.0);
    GdipClosePathFigure(path);
    GdipFillPath(g, b as *mut GpBrush, path);
    GdipDeletePath(path);
    GdipDeleteBrush(b as *mut GpBrush);
    GdipDeleteGraphics(g);
}

// ── constants ──────────────────────────────────────────────────

const WW: i32 = 420;
const PD: i32 = 6;
const GP: i32 = 2;
const MV: usize = 8;
const FW: f32 = 18.0;
const CONFIG_FILE: &str = "config.toml";

const HOTKEY_ID: i32 = 1;
const TRAY_MSG: u32 = WM_APP + 256;
const TRAY_ID: u32 = 1;
const IDM_TOGGLE: u16 = 100;
const IDM_OPEN_CONFIG: u16 = 101;
const IDM_EXIT: u16 = 102;
const MOD_ALT: u32 = 1;
const VK_SPACE: u32 = 0x20;
const VK_ESCAPE: u32 = 0x1B;
const VK_RETURN: u32 = 0x0D;

#[link(name = "user32")]
extern "system" {
    fn RegisterHotKey(hwnd: HWND, id: i32, fs_modifiers: u32, vk: u32) -> BOOL;
    fn SetFocus(hwnd: HWND) -> HWND;
}

#[link(name = "kernel32")]
extern "system" {
    fn SetUnhandledExceptionFilter(lpTopLevelExceptionFilter: Option<unsafe extern "system" fn(*mut EXCEPTION_POINTERS) -> i32>) -> *mut std::ffi::c_void;
}

#[link(name = "imm32")]
extern "system" {
    fn ImmGetContext(hwnd: HWND) -> isize;
    fn ImmSetCompositionWindow(himc: isize, lpCompForm: *const COMPOSITIONFORM) -> BOOL;
    fn ImmGetCompositionStringW(himc: isize, dwIndex: u32, lpBuf: *mut std::ffi::c_void, dwBufLen: u32) -> u32;
    fn ImmReleaseContext(hwnd: HWND, himc: isize) -> BOOL;
}

#[repr(C)]
#[allow(non_snake_case)]
struct COMPOSITIONFORM {
    dwStyle: u32,
    ptCurrentPos: POINT,
    rcArea: RECT,
}

const CFS_FORCE_POSITION: u32 = 0x0020;
const ISC_SHOWUICOMPOSITIONWINDOW: u32 = 0x80000000;
const GCS_COMPSTR: u32 = 0x0008;
const GCS_RESULTSTR: u32 = 0x0800;

#[repr(C)]
struct EXCEPTION_RECORD {
    exception_code: u32,
    exception_flags: u32,
    exception_record: *mut EXCEPTION_RECORD,
    exception_address: *mut std::ffi::c_void,
    number_parameters: u32,
    exception_information: [usize; 15],
}

#[repr(C)]
struct EXCEPTION_POINTERS {
    exception_record: *mut EXCEPTION_RECORD,
    context_record: *mut std::ffi::c_void,
}

unsafe fn hiword(d: u32) -> u32 { (d >> 16) & 0xFFFF }

// ── helpers ─────────────────────────────────────────────────────

fn cfg_str(entries: &[config::Entry], key: &str, default: &str) -> String {
    entries.iter().find(|e| e.key == key).map(|e| e.value.clone()).unwrap_or_else(|| default.to_string())
}
fn cfg_f32(entries: &[config::Entry], key: &str, default: f32) -> f32 {
    entries.iter().find(|e| e.key == key).and_then(|e| e.value.parse().ok()).unwrap_or(default)
}
fn cfg_i32(entries: &[config::Entry], key: &str, default: i32) -> i32 {
    entries.iter().find(|e| e.key == key).and_then(|e| e.value.parse().ok()).unwrap_or(default)
}
fn cfg_usize(entries: &[config::Entry], key: &str, default: usize) -> usize {
    entries.iter().find(|e| e.key == key).and_then(|e| e.value.parse().ok()).unwrap_or(default)
}
fn cfg_bool(entries: &[config::Entry], key: &str, default: bool) -> bool {
    entries.iter().find(|e| e.key == key).map(|e| e.value == "true" || e.value == "1").unwrap_or(default)
}
fn cfg_color(entries: &[config::Entry], key: &str, default: u32) -> u32 {
    entries.iter()
        .find(|e| e.key == key)
        .and_then(|e| u32::from_str_radix(&e.value, 16).ok())
        .unwrap_or(default)
}
fn colorref(rgb: u32) -> COLORREF {
    let r = (rgb >> 16) & 0xFF;
    let g = (rgb >> 8) & 0xFF;
    let b = rgb & 0xFF;
    COLORREF((b << 16) | (g << 8) | r)
}

fn to_w(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

fn pcwstr(v: &[u16]) -> PCWSTR {
    PCWSTR(v.as_ptr())
}

fn font_px(size: f32, dpi: i32) -> i32 {
    (size * dpi as f32 / 96.0) as i32
}

fn status_bar_h(dpi: i32, sz: f32) -> i32 {
    font_px(sz, dpi) + 8
}

fn list_y(input_rect: &RECT) -> i32 {
    input_rect.bottom + GP
}

fn win_h(count: usize, item_h: i32, eh: i32, max_results: usize, status_h: i32) -> i32 {
    let visible = count.min(max_results);
    PD + eh + GP + visible as i32 * item_h + PD + if count > 0 { status_h } else { 0 }
}

fn entry_type(val: &str) -> &'static str {
    if val.ends_with(".exe") {
        "程序"
    } else if val.starts_with("http://") || val.starts_with("https://") {
        if val.contains('?') {
            "搜索"
        } else {
            "网址"
        }
    } else if std::path::Path::new(val).is_dir() {
        "文件夹"
    } else if std::path::Path::new(val).is_file() {
        "文件"
    } else {
        "其他"
    }
}

unsafe fn round_win(h: HWND, w: i32, hh: i32, corner: i32) {
    let rgn = CreateRoundRectRgn(0, 0, w, hh, corner, corner);
    if !rgn.is_invalid() {
        SetWindowRgn(h, Some(rgn), true);
    }
}

/// 将窗口定位到工作区指定位置
/// ratio_x, ratio_y: 0.0~1.0，0.0=左上角 0.5=居中 1.0=右下角
unsafe fn center_win(h: HWND, w: i32, hh: i32, ratio_x: f32, ratio_y: f32) {
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

struct AppState {
    entries: Vec<config::Entry>,
    filter: String,
    input_text: String,
    cursor_pos: usize,
    search_query: String,
    filtered_indices: Vec<usize>,
    sel_index: usize,
    scroll_offset: usize,
    input_rect: RECT,
    visible: bool,
    suppress_activate: bool,
    hfont: Option<HFONT>,
    status_hfont: Option<HFONT>,
    status_font_size: f32,
    font_name: String,
    font_size: f32,
    item_h: i32,
    eh: i32,
    dpi: i32,
    max_results: usize,
    width: i32,
    round_corner: i32,
    always_on_top: bool,
    opacity: u8,
    case_sensitive: bool,
    hide_on_focus_loss: bool,
    theme_color: u32,
    input_bg_color: u32,
    accent_color: u32,
    text_color: u32,
    composing: String,
    config_mtime: Option<std::time::SystemTime>,
    /// 面板水平位置比例 0.0~1.0，由 _panel_position_x 配置计算
    panel_ratio_x: f32,
    /// 面板垂直位置比例 0.0~1.0，由 _panel_position_y 配置计算
    panel_ratio_y: f32,
}

unsafe fn hide_clear(h: HWND, s: &mut AppState) {
    s.visible = false;
    s.filter.clear();
    s.input_text.clear();
    s.cursor_pos = 0;
    s.filtered_indices.clear();
    s.sel_index = 0;
    s.scroll_offset = 0;
    s.search_query.clear();
    DestroyCaret();
    ShowWindow(h, SW_HIDE);
}

unsafe fn fill_list(s: &mut AppState, h: HWND) {
    // 按第一个空格拆成 key 和搜索词
    let (key_part, query_part) = if let Some(pos) = s.filter.find(' ') {
        let (k, _) = s.filter.split_at(pos);
        (k.to_string(), s.filter[pos + 1..].to_string())
    } else {
        (s.filter.clone(), String::new())
    };
    s.search_query = query_part;

    s.filtered_indices.clear();

    if key_part.is_empty() {
        let sh = status_bar_h(s.dpi, s.status_font_size);
        let nh = win_h(0, s.item_h, s.eh, s.max_results, sh);
        let mut rc = RECT::default();
        GetWindowRect(h, &mut rc);
        SetWindowPos(h, Some(HWND_TOP), rc.left, rc.top, s.width, nh, SWP_NOZORDER);
        round_win(h, s.width, nh, s.round_corner);
        return;
    }

    // 匹配并收集索引
    for (i, e) in s.entries.iter().enumerate() {
        let matched = if s.case_sensitive {
            e.key.starts_with(&key_part)
        } else {
            e.key.to_lowercase().starts_with(&key_part.to_lowercase())
        };
        if matched {
            s.filtered_indices.push(i);
        }
    }

    let n = s.filtered_indices.len();
    let sh = status_bar_h(s.dpi, s.status_font_size);
    let nh = win_h(n, s.item_h, s.eh, s.max_results, sh);
    let mut rc = RECT::default();
    GetWindowRect(h, &mut rc);
    SetWindowPos(h, Some(HWND_TOP), rc.left, rc.top, s.width, nh, SWP_NOZORDER);
    round_win(h, s.width, nh, s.round_corner);

    s.sel_index = 0;
    s.scroll_offset = 0;
}

unsafe fn execute_sel(h: HWND, s: &mut AppState) {
    if s.sel_index < s.filtered_indices.len() {
        let idx = s.filtered_indices[s.sel_index];
        if idx < s.entries.len() {
            executor::execute(&s.entries[idx].key, &s.entries[idx].value, &s.search_query);
            hide_clear(h, s);
        }
    }
}

unsafe fn rebuild_font(s: &mut AppState, dpi: i32) {
    if let Some(old) = s.hfont.take() {
        let _ = DeleteObject(HGDIOBJ(old.0));
    }
    let fp = font_px(s.font_size, dpi);
    s.eh = fp + 24;
    s.item_h = fp + 20;
    if let Ok(f) = make_font_with(dpi, &s.font_name, s.font_size) {
        s.hfont = Some(f);
    }
}

// ── drawing ─────────────────────────────────────────────────────

/// 在指定 DC 上画单项的高亮 + 文字（不清背景，用于 VK_UP/DOWN 直接绘制）
unsafe fn draw_item_hl_text(dc: HDC, s: &AppState, list_index: usize, rect: &RECT, selected: bool) {
    // 高亮圆角
    let rcr = (s.round_corner / 2).max(1);
    let color = if selected { s.accent_color } else { s.theme_color };
    let argb = 0xFF000000 | color;
    fill_round_rect(dc, rect.left + 2, rect.top + 2,
        rect.right - rect.left - 4, rect.bottom - rect.top - 4,
        rcr, argb);

    // 文字
    let old_font = s.hfont.as_ref().map(|f| SelectObject(dc, HGDIOBJ(f.0)));
    if let Some(&idx) = s.filtered_indices.get(list_index) {
        if idx < s.entries.len() {
            let e = &s.entries[idx];
            let tag = entry_type(&e.value);
            let txt = format!("[{}]  {}  →  {}", tag, e.key, e.value);
            let mut ws: Vec<u16> = txt.encode_utf16().collect();
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, if selected { COLORREF(0xFFFFFF) } else { colorref(s.text_color) });
            let mut r = RECT {
                left: rect.left + 8, top: rect.top + 6,
                right: rect.right - 4, bottom: rect.bottom - 6,
            };
            DrawTextW(dc, &mut ws, &mut r, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
        }
    }
    if let Some(old) = old_font {
        SelectObject(dc, old);
    }
}

unsafe fn draw_filtered_item(hdc: HDC, s: &AppState, list_index: usize, rect: &RECT) {
    let is_sel = list_index == s.sel_index;

    // 背景
    let bg_brush = CreateSolidBrush(colorref(s.theme_color));
    FillRect(hdc, rect, bg_brush);
    let _ = DeleteObject(HGDIOBJ(bg_brush.0));

    // 选中高亮
    if is_sel {
        let rcr = (s.round_corner / 2).max(1);
        let argb = 0xFF000000 | s.accent_color;
        fill_round_rect(hdc, rect.left + 2, rect.top + 2,
            rect.right - rect.left - 4, rect.bottom - rect.top - 4,
            rcr, argb);
    }

    // 文字
    let old_font = s.hfont.as_ref().map(|f| SelectObject(hdc, HGDIOBJ(f.0)));
    if let Some(&idx) = s.filtered_indices.get(list_index) {
        if idx < s.entries.len() {
            let e = &s.entries[idx];
            let tag = entry_type(&e.value);
            let txt = format!("[{}]  {}  →  {}", tag, e.key, e.value);
            let mut ws: Vec<u16> = txt.encode_utf16().collect();
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, if is_sel { COLORREF(0xFFFFFF) } else { colorref(s.text_color) });
            let mut r = RECT {
                left: rect.left + 8, top: rect.top + 6,
                right: rect.right - 4, bottom: rect.bottom - 6,
            };
            DrawTextW(hdc, &mut ws, &mut r, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
        }
    }
    if let Some(old) = old_font {
        SelectObject(hdc, old);
    }
}

// ── 状态栏重绘（VK_UP/DOWN 和 WM_PAINT 共用）──────────────────

unsafe fn redraw_status_bar(dc: HDC, s: &mut AppState, ly: i32, vis: usize) {
    let sh = status_bar_h(s.dpi, s.status_font_size);
    let sy = ly + vis as i32 * s.item_h;
    let sr = RECT { left: PD + 4, top: sy + 2, right: s.width - PD - 4, bottom: sy + sh - 2 };
    let bg_brush = CreateSolidBrush(colorref(s.theme_color));
    FillRect(dc, &sr, bg_brush);
    let _ = DeleteObject(HGDIOBJ(bg_brush.0));
    if s.status_hfont.is_none() {
        s.status_hfont = make_font_with(s.dpi, &s.font_name, s.status_font_size).ok();
    }
    if let Some(ref sf) = s.status_hfont {
        SelectObject(dc, HGDIOBJ(sf.0));
    }
    let pos = if s.sel_index < s.filtered_indices.len() { s.sel_index + 1 } else { 0 };
    let txt = format!("第{}条/共{}条", pos, s.filtered_indices.len());
    let mut ws: Vec<u16> = txt.encode_utf16().collect();
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, colorref(s.text_color));
    let mut sr2 = sr;
    DrawTextW(dc, &mut ws, &mut sr2, DT_RIGHT | DT_VCENTER | DT_SINGLELINE);
    if let Some(ref f) = s.hfont {
        SelectObject(dc, HGDIOBJ(f.0));
    }
}

// ── window procedure ────────────────────────────────────────────

unsafe extern "system" fn wndproc(
    h: HWND, msg: u32, wp: WPARAM, lp: LPARAM,
) -> LRESULT {
    let ptr = GetWindowLongPtrW(h, GWLP_USERDATA);
    if ptr == 0 {
        // early messages before userdata is set
        match msg {
            WM_MEASUREITEM | WM_CREATE | WM_NCCREATE => return LRESULT(1),
            _ => return DefWindowProcW(h, msg, wp, lp),
        }
    }
    let s = &mut *(ptr as *mut AppState);

    match msg {
        WM_ERASEBKGND => return LRESULT(1),

        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            BeginPaint(h, &mut ps);
            let hdc = ps.hdc;

            // 窗口高度
            let total = s.filtered_indices.len();
            let vis = total.min(s.max_results);
            let lh = s.item_h;
            let ly = list_y(&s.input_rect);
            let sh = status_bar_h(s.dpi, s.status_font_size);
            let win_height = win_h(total, lh, s.eh, s.max_results, sh);

            // 内存 DC 双缓冲
            let mem_dc = CreateCompatibleDC(Some(hdc));
            let bmp = CreateCompatibleBitmap(hdc, s.width, win_height);
            let old_bmp = SelectObject(mem_dc, HGDIOBJ(bmp.0));

            // 窗口背景
            let bg_brush = CreateSolidBrush(colorref(s.theme_color));
            let full_rect = RECT { left: 0, top: 0, right: s.width, bottom: win_height };
            FillRect(mem_dc, &full_rect, bg_brush);
            let _ = DeleteObject(HGDIOBJ(bg_brush.0));

            // 输入框
            let argb = 0xFF000000 | s.input_bg_color;
            let ir = &s.input_rect;
            let ic = (s.round_corner * 3 / 4).max(1);
            fill_round_rect(mem_dc, ir.left, ir.top, ir.right - ir.left, ir.bottom - ir.top, ic, argb);

            if let Some(ref f) = s.hfont {
                SelectObject(mem_dc, HGDIOBJ(f.0));
            }
            SetBkMode(mem_dc, OPAQUE);
            SetBkColor(mem_dc, colorref(s.input_bg_color));
            SetTextColor(mem_dc, colorref(s.text_color));
            let mut r = s.input_rect;
            r.left += 8;
            r.right -= 4;
            let mut ws: Vec<u16> = s.input_text.encode_utf16().collect();
            ws.push(0);
            if !ws.is_empty() && r.right > r.left && r.bottom > r.top {
                DrawTextW(mem_dc, &mut ws, &mut r, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
            }

            // 绘制正在输入的拼音
            if !s.composing.is_empty() {
                let mut sz = SIZE::default();
                if !s.input_text.is_empty() {
                    let pws: Vec<u16> = s.input_text.encode_utf16().collect();
                    GetTextExtentPoint32W(mem_dc, &pws, &mut sz);
                }
                let cx = s.input_rect.left + 8 + sz.cx;
                SetTextColor(mem_dc, colorref(s.text_color & 0xC0C0C0 | 0x404040));
                let mut cws: Vec<u16> = s.composing.encode_utf16().collect();
                cws.push(0);
                let mut cr = RECT {
                    left: cx,
                    top: s.input_rect.top,
                    right: s.input_rect.right,
                    bottom: s.input_rect.bottom,
                };
                DrawTextW(mem_dc, &mut cws, &mut cr, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
                SetTextColor(mem_dc, colorref(s.text_color));
            }

            // 列表
            let start = s.scroll_offset.min(total.saturating_sub(vis));
            for i in 0..vis {
                let fi = start + i;
                if fi >= total { break; }
                let y = ly + i as i32 * lh;
                let rc = RECT { left: PD, top: y, right: s.width - PD, bottom: y + lh };
                draw_filtered_item(mem_dc, s, fi, &rc);
            }

            // 状态栏
            if vis > 0 {
                redraw_status_bar(mem_dc, s, ly, vis);
            }

            // 一次拷屏
            BitBlt(hdc, 0, 0, s.width, win_height, Some(mem_dc), 0, 0, SRCCOPY);

            // 清理内存 DC
            SelectObject(mem_dc, old_bmp);
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            let _ = DeleteDC(mem_dc);

            EndPaint(h, &ps);
            return LRESULT(0);
        }

        WM_DESTROY => {
            tray::destroy();
            PostQuitMessage(0);
            return LRESULT(0);
        }

        WM_ACTIVATE => {
            if s.suppress_activate {
                return DefWindowProcW(h, msg, wp, lp);
            }
            if wp.0 == 0 {
                if !s.visible { return LRESULT(0); }
                if s.hide_on_focus_loss { hide_clear(h, s); }
                return LRESULT(0);
            }
            return DefWindowProcW(h, msg, wp, lp);
        }

        WM_IME_SETCONTEXT => {
            // 禁用组合窗口（白框），让拼音不弹白框
            return DefWindowProcW(h, msg, wp, LPARAM(lp.0 & !(ISC_SHOWUICOMPOSITIONWINDOW as isize)));
        }

        WM_IME_STARTCOMPOSITION => {
            HideCaret(Some(h));
            let himc = ImmGetContext(h);
            if himc != 0 {
                let cf = COMPOSITIONFORM {
                    dwStyle: CFS_FORCE_POSITION,
                    ptCurrentPos: POINT { x: s.input_rect.left, y: s.input_rect.bottom },
                    rcArea: RECT::default(),
                };
                ImmSetCompositionWindow(himc, &cf);
                ImmReleaseContext(h, himc);
            }
            return LRESULT(0);
        }

        WM_IME_COMPOSITION => {
            let himc = ImmGetContext(h);
            if himc != 0 {
                // 更新位置
                let cf = COMPOSITIONFORM {
                    dwStyle: CFS_FORCE_POSITION,
                    ptCurrentPos: POINT { x: s.input_rect.left, y: s.input_rect.bottom },
                    rcArea: RECT::default(),
                };
                ImmSetCompositionWindow(himc, &cf);

                // 读取拼音
                if lp.0 as u32 & GCS_COMPSTR != 0 {
                    let len = ImmGetCompositionStringW(himc, GCS_COMPSTR, ptr::null_mut(), 0);
                    if len > 0 {
                        let bytes = len as usize;
                        let mut buf = vec![0u16; bytes / 2 + 1];
                        ImmGetCompositionStringW(himc, GCS_COMPSTR, buf.as_mut_ptr() as *mut std::ffi::c_void, len);
                        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                        s.composing = String::from_utf16_lossy(&buf[..end]);
                    } else {
                        s.composing.clear();
                    }
                }
                // 读取确认后的中文
                if lp.0 as u32 & GCS_RESULTSTR != 0 {
                    let len = ImmGetCompositionStringW(himc, GCS_RESULTSTR, ptr::null_mut(), 0);
                    if len > 0 {
                        let bytes = len as usize;
                        let mut buf = vec![0u16; bytes / 2 + 1];
                        ImmGetCompositionStringW(himc, GCS_RESULTSTR, buf.as_mut_ptr() as *mut std::ffi::c_void, len);
                        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                        let result = String::from_utf16_lossy(&buf[..end]);
                        s.input_text.insert_str(s.cursor_pos, &result);
                        s.cursor_pos += result.len();
                        s.filter = s.input_text.clone();
                        s.composing.clear();
                        fill_list(s, h);
                        update_caret(s, h);
                    }
                }
                ImmReleaseContext(h, himc);
                RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
            }
            return LRESULT(0);
        }

        WM_IME_ENDCOMPOSITION => {
            s.composing.clear();
            ShowCaret(Some(h));
            RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
            return LRESULT(0);
        }

        WM_HOTKEY => {
            toggle_win(h, s);
            return LRESULT(0);
        }

        TRAY_MSG => {
            match lp.0 as u32 & 0xFFFF {
                0x0205 => {
                    s.suppress_activate = true;
                    tray::show_menu(h);
                    s.suppress_activate = false;
                }
                0x0201 => toggle_win(h, s),
                _ => {}
            }
            return LRESULT(0);
        }

        WM_COMMAND => {
            let id = (wp.0 as u32 & 0xFFFF) as u16;
            match id {
                IDM_TOGGLE => { toggle_win(h, s); return LRESULT(0); }
                IDM_OPEN_CONFIG => {
                    let p = to_w(CONFIG_FILE);
                    ShellExecuteW(Some(h), w!("open"), pcwstr(&p), PCWSTR(ptr::null()), PCWSTR(ptr::null()), SW_SHOWNORMAL);
                    return LRESULT(0);
                }
                IDM_EXIT => {
                    tray::destroy();
                    PostQuitMessage(0);
                    return LRESULT(0);
                }
                _ => {}
            }
            return LRESULT(0);
        }

        WM_KEYDOWN => {
            match wp.0 as u32 {
                VK_ESCAPE => { hide_clear(h, s); return LRESULT(0); }
                VK_RETURN => {
                    execute_sel(h, s);
                    return LRESULT(0);
                }
                0x08 /*VK_BACK*/ => {
                    if s.cursor_pos > 0 {
                        let prev = s.input_text.floor_char_boundary(s.cursor_pos - 1);
                        s.input_text.replace_range(prev..s.cursor_pos, "");
                        s.cursor_pos = prev;
                        s.filter = s.input_text.clone();
                        fill_list(s, h);
                        update_caret(s, h);
                        RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                    }
                    return LRESULT(0);
                }
                0x25 /*VK_LEFT*/ => {
                    if s.cursor_pos > 0 {
                        s.cursor_pos = s.input_text.floor_char_boundary(s.cursor_pos - 1);
                        update_caret(s, h);
                    }
                    return LRESULT(0);
                }
                0x27 /*VK_RIGHT*/ => {
                    if s.cursor_pos < s.input_text.len() {
                        s.cursor_pos = s.input_text.ceil_char_boundary(s.cursor_pos + 1);
                        update_caret(s, h);
                    }
                    return LRESULT(0);
                }
                0x2E /*VK_DELETE*/ => {
                    if s.cursor_pos < s.input_text.len() {
                        let next = s.input_text.ceil_char_boundary(s.cursor_pos + 1);
                        s.input_text.replace_range(s.cursor_pos..next, "");
                        s.filter = s.input_text.clone();
                        fill_list(s, h);
                        RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                    }
                    return LRESULT(0);
                }
                0x26 /*VK_UP*/ | 0x28 /*VK_DOWN*/ => {
                    let n = s.filtered_indices.len();
                    if n == 0 { return LRESULT(0); }
                    let old_sel = s.sel_index;
                    if wp.0 as u32 == 0x26 {
                        if s.sel_index > 0 {
                            s.sel_index -= 1;
                            if s.sel_index < s.scroll_offset {
                                s.scroll_offset = s.sel_index;
                            }
                        }
                    } else {
                        if s.sel_index + 1 < n {
                            s.sel_index += 1;
                            let bottom = s.scroll_offset + s.max_results - 1;
                            if s.sel_index > bottom && s.scroll_offset + s.max_results < n {
                                s.scroll_offset += 1;
                            }
                        }
                    }
                    if old_sel != s.sel_index {
                        let ly = list_y(&s.input_rect);
                        let lh = s.item_h;
                        let vis = n.min(s.max_results);
                        let dc = GetDC(Some(h));
                        // 旧选中项（擦高亮 → 非选中色文字）
                        let old_vis = old_sel as i32 - s.scroll_offset as i32;
                        if old_vis >= 0 && old_vis < vis as i32 {
                            let y = ly + old_vis * lh;
                            let rc = RECT { left: PD, top: y, right: s.width - PD, bottom: y + lh };
                            draw_filtered_item(dc, s, old_sel, &rc);
                        }
                        // 新选中项（画高亮 → 白色文字）
                        let new_vis = s.sel_index as i32 - s.scroll_offset as i32;
                        if new_vis >= 0 && new_vis < vis as i32 {
                            let y = ly + new_vis * lh;
                            let rc = RECT { left: PD, top: y, right: s.width - PD, bottom: y + lh };
                            draw_item_hl_text(dc, s, s.sel_index, &rc, true);
                        }
                        redraw_status_bar(dc, s, ly, vis);
                        let _ = ReleaseDC(Some(h), dc);
                    }
                    return LRESULT(0);
                }
                _ => {
                    return DefWindowProcW(h, msg, wp, lp);
                }
            }
        }

        WM_CHAR => {
            let ch = match char::from_u32(wp.0 as u32) {
                Some(c) if !c.is_control() => c,
                _ => { return LRESULT(0); }
            };
            s.input_text.insert(s.cursor_pos, ch);
            s.cursor_pos += ch.len_utf8();
            s.filter = s.input_text.clone();
            fill_list(s, h);
            update_caret(s, h);
            RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
            return LRESULT(0);
        }

        WM_SIZE => {
            let mut rc = RECT::default();
            GetClientRect(h, &mut rc);
            let w = rc.right - rc.left;
            let fp = font_px(s.font_size, s.dpi);
            let eh = fp + 24;
            s.eh = eh;
            s.item_h = fp + 20;
            s.width = w;
            s.input_rect = RECT { left: PD, top: PD, right: w - PD, bottom: PD + eh };
            return LRESULT(0);
        }

        WM_DPICHANGED => {
            let dpi = hiword(wp.0 as u32) as i32;
            s.dpi = dpi;
            rebuild_font(s, dpi);
            let rc = &*(lp.0 as *const RECT);
            SetWindowPos(h, Some(HWND_TOP), rc.left, rc.top,
                rc.right - rc.left, rc.bottom - rc.top,
                SWP_NOZORDER | SWP_NOACTIVATE);
            return LRESULT(0);
        }

        _ => {}
    }

    DefWindowProcW(h, msg, wp, lp)
}

unsafe fn create_input_caret(h: HWND, s: &AppState) {
    if let Some(ref f) = s.hfont {
        let dc = GetDC(Some(h));
        let old = SelectObject(dc, HGDIOBJ(f.0));
        let mut tm = TEXTMETRICW::default();
        GetTextMetricsW(dc, &mut tm);
        SelectObject(dc, old);
        let _ = ReleaseDC(Some(h), dc);
        let caret_h = tm.tmHeight;
        CreateCaret(h, Some(HBITMAP(ptr::null_mut())), 2, caret_h as i32);
        SetCaretPos(s.input_rect.left + 8, s.input_rect.top + ((s.input_rect.bottom - s.input_rect.top) - caret_h) / 2);
        ShowCaret(Some(h));
    }
}

unsafe fn update_caret(s: &AppState, h: HWND) {
    if !s.visible { return; }
    if let Some(ref f) = s.hfont {
        let dc = GetDC(Some(h));
        let old = SelectObject(dc, HGDIOBJ(f.0));
        let mut tm = TEXTMETRICW::default();
        GetTextMetricsW(dc, &mut tm);
        let caret_h = tm.tmHeight;
        let prefix = &s.input_text[..s.cursor_pos];
        let ws: Vec<u16> = prefix.encode_utf16().collect();
        let mut sz = SIZE::default();
        GetTextExtentPoint32W(dc, &ws, &mut sz);
        let cx = s.input_rect.left + 8 + sz.cx + 1;
        let cy = s.input_rect.top + ((s.input_rect.bottom - s.input_rect.top) - caret_h) / 2;
        SelectObject(dc, old);
        let _ = ReleaseDC(Some(h), dc);
        SetCaretPos(cx, cy);
    }
}

unsafe fn toggle_win(h: HWND, s: &mut AppState) {
    if s.visible {
        hide_clear(h, s);
    } else {
        // 按需重载配置
        let cur = std::fs::metadata(CONFIG_FILE)
            .ok()
            .and_then(|m| m.modified().ok());
        let mut font_name = s.font_name.clone();
        let mut font_size = s.font_size;
        let config_changed = s.config_mtime != cur;
        if config_changed {
            let raw = config::load(CONFIG_FILE);
            font_name = cfg_str(&raw, "_font", &s.font_name);
            font_size = cfg_f32(&raw, "_font_size", s.font_size);
            s.max_results = cfg_usize(&raw, "_max_results", s.max_results);
            s.width = cfg_i32(&raw, "_width", s.width);
            s.round_corner = cfg_i32(&raw, "_round_corner", s.round_corner);
            s.hide_on_focus_loss = cfg_bool(&raw, "_hide_on_focus_loss", s.hide_on_focus_loss);
            s.theme_color = cfg_color(&raw, "_theme_color", s.theme_color);
            s.input_bg_color = cfg_color(&raw, "_input_bg_color", s.input_bg_color);
            s.accent_color = cfg_color(&raw, "_accent_color", s.accent_color);
            s.text_color = cfg_color(&raw, "_text_color", s.text_color);
            s.status_font_size = cfg_f32(&raw, "_status_font_size", s.status_font_size);
            if let Some(old) = s.status_hfont.take() {
                let _ = DeleteObject(HGDIOBJ(old.0));
            }
            s.always_on_top = cfg_bool(&raw, "_always_on_top", s.always_on_top);
            s.opacity = cfg_usize(&raw, "_opacity", s.opacity as usize).min(255) as u8;
            s.case_sensitive = cfg_bool(&raw, "_case_sensitive", s.case_sensitive);
            // 面板位置：0~100 转为 0.0~1.0 比例
            s.panel_ratio_x = cfg_f32(&raw, "_panel_position_x", 50.0).clamp(0.0, 100.0) / 100.0;
            s.panel_ratio_y = cfg_f32(&raw, "_panel_position_y", 50.0).clamp(0.0, 100.0) / 100.0;
            let new_entries: Vec<_> = raw.into_iter().filter(|e| !e.key.starts_with('_')).collect();
            if !new_entries.is_empty() {
                s.entries = new_entries;
            }
            s.config_mtime = cur;
            // 应用窗口样式变更
            if s.opacity < 255 {
                let style = GetWindowLongPtrW(h, GWL_EXSTYLE);
                if style & WS_EX_LAYERED.0 as isize == 0 {
                    SetWindowLongPtrW(h, GWL_EXSTYLE, style | WS_EX_LAYERED.0 as isize);
                }
                SetLayeredWindowAttributes(h, COLORREF(0), s.opacity, LWA_ALPHA);
            } else {
                let style = GetWindowLongPtrW(h, GWL_EXSTYLE);
                if style & WS_EX_LAYERED.0 as isize != 0 {
                    SetWindowLongPtrW(h, GWL_EXSTYLE, style & !(WS_EX_LAYERED.0 as isize));
                }
            }
            let after = if s.always_on_top { Some(HWND_TOPMOST) } else { Some(HWND_NOTOPMOST) };
            SetWindowPos(h, after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        // 字体变化时重建
        if s.font_name != font_name || (s.font_size - font_size).abs() > 0.5 {
            s.font_name = font_name;
            s.font_size = font_size;
            rebuild_font(s, s.dpi);
        }
        s.visible = true;
        s.filter.clear();
        s.input_text.clear();
        s.cursor_pos = 0;
        s.filtered_indices.clear();
        s.sel_index = 0;
        s.scroll_offset = 0;
        fill_list(s, h);
        // 首次启动或配置变更后重新定位，否则复用上次位置
        if config_changed {
            let sh = status_bar_h(s.dpi, s.status_font_size);
            center_win(h, s.width, win_h(0, s.item_h, s.eh, s.max_results, sh), s.panel_ratio_x, s.panel_ratio_y);
        }
        ShowWindow(h, SW_SHOW);
        SetForegroundWindow(h);
        SetFocus(h);
        create_input_caret(h, s);
    }
}

// ── entry point ─────────────────────────────────────────────────

unsafe extern "system" fn seh_filter(ep: *mut EXCEPTION_POINTERS) -> i32 {
    if !ep.is_null() {
        let rec = (*ep).exception_record;
        if !rec.is_null() {
            let code = (*rec).exception_code;
            let addr = (*rec).exception_address;
            let mut f = std::fs::File::create("crash.log").ok();
            if let Some(ref mut f) = f {
                let _ = write!(f, "EXCEPTION code=0x{:08X} addr={:p}\n", code, addr);
            }
        }
    }
    1
}

fn main() -> Result<()> {
    let mut gdiplus_token: usize = 0;
    unsafe {
        let input = GpStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: false.into(),
            SuppressExternalCodecs: false.into(),
        };
        GdiplusStartup(&mut gdiplus_token, &input, std::ptr::null_mut());
    }

    unsafe {
        SetUnhandledExceptionFilter(Some(seh_filter));
    }

    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {}\n", info);
        let _ = std::fs::write("panic.log", &msg);
    }));

    unsafe {
        let _ = SetProcessDPIAware();

        let screen_dc = GetDC(None);
        let dpi = GetDeviceCaps(Some(screen_dc), LOGPIXELSY);
        let _ = ReleaseDC(None, screen_dc);

        // Config
        let raw_entries = config::load(CONFIG_FILE);
        let font_name = cfg_str(&raw_entries, "_font", "Segoe UI");
        let font_size = cfg_f32(&raw_entries, "_font_size", FW);
        let width = cfg_i32(&raw_entries, "_width", WW);
        let max_results = cfg_usize(&raw_entries, "_max_results", MV);
        let round_corner = cfg_i32(&raw_entries, "_round_corner", 12);
        let always_on_top = cfg_bool(&raw_entries, "_always_on_top", true);
        let opacity = cfg_usize(&raw_entries, "_opacity", 255).min(255) as u8;
        let case_sensitive = cfg_bool(&raw_entries, "_case_sensitive", true);
        let hide_on_focus_loss = cfg_bool(&raw_entries, "_hide_on_focus_loss", true);
        let theme_color = cfg_color(&raw_entries, "_theme_color", 0x1E1E1E);
        let input_bg_color = cfg_color(&raw_entries, "_input_bg_color", 0x2A2A2A);
        let accent_color = cfg_color(&raw_entries, "_accent_color", 0x4A6FA5);
        let text_color = cfg_color(&raw_entries, "_text_color", 0xCCCCCC);
        let status_font_size = cfg_f32(&raw_entries, "_status_font_size", 12.0);
        // 面板位置：0~100（0=左上 50=居中 100=右下），转为 0.0~1.0 比例
        let panel_ratio_x = cfg_f32(&raw_entries, "_panel_position_x", 50.0).clamp(0.0, 100.0) / 100.0;
        let panel_ratio_y = cfg_f32(&raw_entries, "_panel_position_y", 50.0).clamp(0.0, 100.0) / 100.0;
        let entries: Vec<config::Entry> = raw_entries.into_iter().filter(|e| !e.key.starts_with('_')).collect();

        let inst = GetModuleHandleW(None)?;

        let cn = to_w("KeyHop");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: inst.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH(ptr::null_mut()),
            lpszClassName: PCWSTR(cn.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let cn2 = to_w("KeyHop");
        let ex_style = WS_EX_TOOLWINDOW
            | if always_on_top { WS_EX_TOPMOST } else { WINDOW_EX_STYLE::default() };
        let hwnd = CreateWindowExW(
            ex_style,
            PCWSTR(cn2.as_ptr()),
            w!("KeyHop"),
            WS_POPUP,
            0, 0, width, 1,
            None,
            None,
            Some(inst.into()),
            None,
        )?;
        if opacity < 255 {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE,
                (GetWindowLongPtrW(hwnd, GWL_EXSTYLE) | WS_EX_LAYERED.0 as isize) as isize);
            SetLayeredWindowAttributes(hwnd, COLORREF(0), opacity, LWA_ALPHA);
        }

        let fp = font_px(font_size, dpi);
        let hfont = make_font_with(dpi, &font_name, font_size).ok();

        // 首次启动时设为 None，触发首次弹出时读取配置并定位
        let config_mtime = None;
        let state = AppState {
            entries,
            filter: String::new(),
            input_text: String::new(),
            cursor_pos: 0,
            search_query: String::new(),
            filtered_indices: Vec::new(),
            sel_index: 0,
            scroll_offset: 0,
            input_rect: RECT { left: PD, top: PD, right: width - PD, bottom: PD + fp + 24 },
            visible: false,
            suppress_activate: false,
            hfont,
            status_hfont: None,
            status_font_size,
            font_name,
            font_size,
            item_h: fp + 20,
            eh: fp + 24,
            dpi,
            max_results,
            width,
            round_corner,
            always_on_top,
            opacity,
            case_sensitive,
            hide_on_focus_loss,
            theme_color,
            input_bg_color,
            accent_color,
            text_color,
            composing: String::new(),
            config_mtime,
            panel_ratio_x,
            panel_ratio_y,
        };

        let boxed = Box::into_raw(Box::new(state));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, boxed as isize);

        let _ = RegisterHotKey(hwnd, HOTKEY_ID, MOD_ALT, VK_SPACE);
        tray::init(hwnd);
    }

    unsafe {
        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 == 0 || ret.0 == -1 {
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&mut msg);
        }
    }

    unsafe { GdiplusShutdown(gdiplus_token); }
    Ok(())
}

unsafe fn make_font_with(dpi: i32, name: &str, size: f32) -> Result<HFONT> {
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
