// Gua — 数据结构、常量、工具函数

use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config;

// ── constants ──────────────────────────────────────────────────

pub const WW: i32 = 420;
pub const PD: i32 = 6;
pub const GP: i32 = 2;
pub const MV: usize = 8;
pub const FW: f32 = 18.0;
pub const FUZZY_MATCH_DEFAULT: bool = true;
pub const PINYIN_SEARCH_DEFAULT: bool = true;
/// 配置路径改为动态获取: config::config_path()

pub const HOTKEY_ID: i32 = 1;

pub const TRAY_MSG: u32 = WM_APP + 256;
pub const TRAY_ID: u32 = 1;
pub const IDM_TOGGLE: u16 = 100;
pub const IDM_OPEN_CONFIG: u16 = 101;
pub const IDM_EXIT: u16 = 102;
pub const IDM_SETTINGS: u16 = 103;
pub const MOD_ALT: u32 = 1;
pub const MOD_CONTROL: u32 = 2;
pub const MOD_SHIFT: u32 = 4;
pub const MOD_WIN: u32 = 8;
pub const VK_SPACE: u32 = 0x20;
pub const VK_ESCAPE: u32 = 0x1B;
pub const VK_RETURN: u32 = 0x0D;

#[link(name = "user32")]
extern "system" {
    pub fn RegisterHotKey(hwnd: HWND, id: i32, fs_modifiers: u32, vk: u32) -> BOOL;
    pub fn SetFocus(hwnd: HWND) -> HWND;
    pub fn UnregisterHotKey(hwnd: HWND, id: i32) -> BOOL;
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

// ── D2D 渲染器 ─────────────────────────────────────────────────

#[allow(dead_code)]
pub struct GuaRenderer {
    pub d3d_device: ID3D11Device,
    pub d3d_context: ID3D11DeviceContext,
    pub dxgi_factory: IDXGIFactory2,
    pub swap_chain: IDXGISwapChain1,
    pub supports_tearing: bool,
    pub d2d_factory: ID2D1Factory1,
    pub d2d_device: ID2D1Device,
    pub d2d_context: ID2D1DeviceContext,
    pub dwrite_factory: IDWriteFactory,
    pub target: Option<ID2D1Bitmap1>,
    pub dcomp_device: Option<IDCompositionDevice>,
    pub dcomp_visual: Option<IDCompositionVisual>,
    pub dcomp_target: Option<IDCompositionTarget>,
    pub com_initialized: bool,
}

pub fn gua_renderer(s: &AppState) -> Option<&GuaRenderer> {
    if s.renderer.is_null() { None } else { Some(unsafe { &*s.renderer }) }
}

pub fn gua_renderer_mut(s: &mut AppState) -> Option<&mut GuaRenderer> {
    if s.renderer.is_null() { None } else { Some(unsafe { &mut *s.renderer }) }
}

pub fn color_to_d2d(rgb: u32, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 0xFF) as f32 / 255.0,
        g: ((rgb >> 8) & 0xFF) as f32 / 255.0,
        b: (rgb & 0xFF) as f32 / 255.0,
        a: alpha,
    }
}

// ── helpers ─────────────────────────────────────────────────────

/// 提取 DWORD 的高 16 位作为 UINT
///
/// # Safety
/// 纯数值运算，无安全要求
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
    entries.iter().find(|e| e.key == key).map(|e| e.value.eq_ignore_ascii_case("true") || e.value == "1").unwrap_or(default)
}
pub fn cfg_color(entries: &[config::Entry], key: &str, default: u32) -> u32 {
    entries.iter()
        .find(|e| e.key == key)
        .and_then(|e| u32::from_str_radix(e.value.trim_start_matches('#'), 16).ok())
        .unwrap_or(default)
}
#[repr(C)]
pub struct MemPrio {
    pub priority: u32,
}

pub const PROCESS_MEMORY_PRIORITY: i32 = 0;
pub const MEM_PRIO_VERY_LOW: u32 = 1;
pub const MEM_PRIO_NORMAL: u32 = 5;

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

/// 将窗口定位到工作区指定位置
/// ratio_x, ratio_y: 0.0~1.0，0.0=左上角 0.5=居中 1.0=右下角
/// 将窗口定位到工作区指定比例位置
///
/// # Safety
/// - `h` 必须是有效的窗口句柄
/// - 需在窗口创建后才可调用
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
        let _ = SetWindowPos(h, Some(HWND_TOP), x, y, w, hh, SWP_NOZORDER);
    }
}

// ── 热键解析 ────────────────────────────────────────────────────

fn modifier_bit(name: &str) -> Option<u32> {
    match name.to_lowercase().as_str() {
        "alt" => Some(MOD_ALT),
        "ctrl" | "control" => Some(MOD_CONTROL),
        "shift" => Some(MOD_SHIFT),
        "win" | "windows" | "super" => Some(MOD_WIN),
        _ => None,
    }
}

fn vk_code(name: &str) -> Option<u32> {
    let upper = name.to_uppercase();
    let bytes = upper.as_bytes();

    // A-Z, 0-9
    if bytes.len() == 1 {
        let b = bytes[0];
        if b'A' <= b && b <= b'Z' {
            return Some(b as u32);
        }
        if b'0' <= b && b <= b'9' {
            return Some(b as u32);
        }
    }

    // F1-F24
    if bytes.len() >= 2 && bytes.len() <= 3 && bytes[0] == b'F' {
        if let Ok(n) = upper[1..].parse::<u32>() {
            if (1..=24).contains(&n) {
                return Some(0x6F + n);
            }
        }
    }

    match upper.as_str() {
        "SPACE" => Some(0x20),
        "ENTER" => Some(0x0D),
        "ESCAPE" | "ESC" => Some(0x1B),
        "TAB" => Some(0x09),
        "BACKSPACE" => Some(0x08),
        "DELETE" | "DEL" => Some(0x2E),
        "INSERT" | "INS" => Some(0x2D),
        "HOME" => Some(0x24),
        "END" => Some(0x23),
        "PAGEUP" | "PGUP" => Some(0x21),
        "PAGEDOWN" | "PGDN" => Some(0x22),
        "PAUSE" | "BREAK" => Some(0x13),
        "CAPSLOCK" => Some(0x14),
        "SCROLLLOCK" => Some(0x91),
        "UP" => Some(0x26),
        "DOWN" => Some(0x28),
        "LEFT" => Some(0x25),
        "RIGHT" => Some(0x27),
        "[" | "LBRACKET" => Some(0xDB),
        "]" | "RBRACKET" => Some(0xDD),
        "\\" | "BACKSLASH" => Some(0xDC),
        ";" | "SEMICOLON" => Some(0xBA),
        "'" | "APOSTROPHE" | "QUOTE" => Some(0xDE),
        "," | "COMMA" => Some(0xBC),
        "." | "PERIOD" | "DOT" => Some(0xBE),
        "/" | "SLASH" => Some(0xBF),
        "-" | "MINUS" | "HYPHEN" => Some(0xBD),
        "=" | "EQUALS" | "EQUAL" => Some(0xBB),
        "`" | "BACKTICK" | "TILDE" | "GRAVE" => Some(0xC0),
        "*" | "MULTIPLY" => Some(0x6A),
        "+" | "ADD" | "PLUS" => Some(0x6B),
        "SEPARATOR" | "SEP" => Some(0x6C),
        _ => None,
    }
}

/// 解析 "Mod1+Mod2+Key" 格式热键字符串，返回 (modifiers, vk)。
/// 至少需要两个键，第一个必须是修饰键，最多五个部分。
pub fn parse_hotkey(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split('+')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() < 2 || parts.len() > 5 {
        return None;
    }
    let mut mods = 0u32;
    for p in &parts[..parts.len() - 1] {
        mods |= modifier_bit(p)?;
    }
    if mods == 0 {
        return None;
    }
    let vk = vk_code(parts[parts.len() - 1])?;
    Some((mods, vk))
}

// ── 黑名单 ──────────────────────────────────────────────────────

/// 解析 _pinyin_overrides 多音字追加读音配置
/// 格式：_pinyin_overrides = 茄=qie, 了=le  （逗号分隔）
/// 或分行：
///   _pinyin_overrides = 茄=qie
///   _pinyin_overrides = 了=le
/// 每个字对应一个追加读音列表（不覆盖 crate 默认读音）
pub fn cfg_pinyin_overrides(entries: &[config::Entry], key: &str) -> HashMap<char, Vec<String>> {
    let mut map: HashMap<char, Vec<String>> = HashMap::new();
    for entry in entries.iter().filter(|e| e.key == key) {
        for part in entry.value.split(',') {
            let part = part.trim().trim_matches('"').trim_matches('\'');
            if let Some(pos) = part.find('=') {
                let ch = part[..pos].trim().chars().next();
                let py = part[pos + 1..].trim().trim_matches('"').trim_matches('\'').to_lowercase();
                if let Some(c) = ch {
                    if !py.is_empty() {
                        map.entry(c).or_insert_with(Vec::new).push(py);
                    }
                }
            }
        }
    }
    map
}

/// 解析逗号分隔的黑名单程序列表（exe 文件名，不区分大小写）
pub fn cfg_blacklist(entries: &[config::Entry], key: &str) -> Vec<String> {
    let mut result = Vec::new();
    for entry in entries.iter().filter(|e| e.key == key) {
        for item in entry.value.split(',') {
            let trimmed = item.trim();
            if !trimmed.is_empty() {
                result.push(trimmed.to_string());
            }
        }
    }
    result
}

/// 获取前台窗口的 exe 文件名（小写，不含路径）
/// 获取前台窗口的 exe 文件名
///
/// # Safety
/// - 需在支持 PROCESS_QUERY_LIMITED_INFORMATION 权限的进程中调用
/// - 返回值为进程名快照，多线程场景下前台窗口可能已变化
pub unsafe fn get_foreground_exe() -> Option<String> {
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(
            dwDesiredAccess: u32,
            bInheritHandle: i32,
            dwProcessId: u32,
        ) -> HANDLE;
        fn QueryFullProcessImageNameW(
            hProcess: HANDLE,
            dwFlags: u32,
            lpExeName: *mut u16,
            lpdwSize: &mut u32,
        ) -> BOOL;
    }

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    let hwnd = GetForegroundWindow();
    if hwnd.0.is_null() {
        return None;
    }
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 {
        return None;
    }
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if process.0.is_null() {
        return None;
    }
    let mut buf = [0u16; 260];
    let mut size = buf.len() as u32;
    let result = if QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut size).as_bool() {
        let s = String::from_utf16_lossy(&buf[..size as usize]);
        std::path::Path::new(&s)
            .file_name()
            .map(|f| f.to_string_lossy().to_lowercase())
    } else {
        None
    };
    let _ = CloseHandle(process);
    result
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
    pub text_format: Option<IDWriteTextFormat>,
    pub status_text_format: Option<IDWriteTextFormat>,
    pub status_font_size: f32,
    pub font_name: String,
    pub font_size: f32,
    pub item_h: i32,
    pub eh: i32,
    pub dpi: i32,
    pub max_results: usize,
    pub width: i32,
    pub round_corner: i32,
    pub opacity: u8,
    pub case_sensitive: bool,
    pub hide_on_focus_loss: bool,
    pub theme_color: u32,
    pub input_bg_color: u32,
    pub accent_color: u32,
    pub text_color: u32,
    pub theme_brush: Option<ID2D1SolidColorBrush>,
    pub input_bg_brush: Option<ID2D1SolidColorBrush>,
    pub accent_brush: Option<ID2D1SolidColorBrush>,
    pub text_brush: Option<ID2D1SolidColorBrush>,
    pub white_brush: Option<ID2D1SolidColorBrush>,
    pub renderer: *mut GuaRenderer,
    pub device_recover_attempts: u32,
    pub composing: String,
    pub config_mtime: Option<std::time::SystemTime>,
    /// 面板水平位置比例 0.0~1.0，由 _panel_position_x 配置计算
    pub panel_ratio_x: f32,
    /// 面板垂直位置比例 0.0~1.0，由 _panel_position_y 配置计算
    pub panel_ratio_y: f32,
    /// 识别码分类展开/折叠状态
    pub codes_cat_state: Vec<bool>,
    /// 当前热键修饰键位掩码，由 _hotkey 配置解析
    pub mod_keys: u32,
    /// 当前热键虚拟键码，由 _hotkey 配置解析
    pub hotkey_vk: u32,
    /// 黑名单程序 exe 文件名列表（小写）。当前台窗口在其中时热键不响应。
    pub blacklist: Vec<String>,
    /// 上次隐藏的时间戳，用于托盘点击防抖（避免失焦隐藏后立即被托盘消息重新唤起）
    pub last_hide_time: Option<std::time::Instant>,
    /// 是否启用模糊匹配（_fuzzy_match）
    pub fuzzy_enabled: bool,
    /// 是否启用拼音搜索（_pinyin_search）
    pub pinyin_enabled: bool,
    /// 多音字覆写表：字 → 追加读音列表，匹配时与 crate 默认读音共存
    pub pinyin_overrides: HashMap<char, Vec<String>>,
}

// ── 私有字体加载 ────────────────────────────────────────────────

fn read_font_family(data: &[u8]) -> Option<String> {
    let buf = |off: usize, len: usize| -> Option<&[u8]> { data.get(off..off + len) };
    let u16be = |off: usize| -> Option<u16> { let b = buf(off, 2)?; Some(u16::from_be_bytes([b[0], b[1]])) };
    let u32be = |off: usize| -> Option<u32> { let b = buf(off, 4)?; Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]])) };
    let num_tables = u16be(4)? as usize;
    let mut name_off = None;
    let mut name_len = None;
    for i in 0..num_tables {
        let entry = 12 + i * 16;
        let tag = buf(entry, 4)?;
        if tag == b"name" { name_off = Some(u32be(entry + 8)? as usize); name_len = Some(u32be(entry + 12)? as usize); break; }
    }
    let name_off = name_off?; let name_len = name_len?;
    let nt = buf(name_off, name_len)?;
    let format = u16be(name_off)?;
    let count = u16be(name_off + 2)? as usize;
    let string_off = u16be(name_off + 4)? as usize;
    let name_record_off = if format == 0 { name_off + 6 }
        else if format == 1 { let lang_tag_count = u16be(name_off + 6)? as usize; name_off + 6 + 2 + lang_tag_count * 12 }
        else { return None; };
    struct Rec { platform: u16, encoding: u16, lang: u16, offset: usize, length: usize }
    let mut candidates: Vec<Rec> = Vec::new();
    for i in 0..count {
        let r = name_record_off + i * 12;
        let platform = u16be(r)?; let encoding = u16be(r + 2)?; let lang = u16be(r + 4)?;
        let name_id = u16be(r + 6)?; let length = u16be(r + 8)? as usize; let offset = u16be(r + 10)? as usize;
        if name_id == 1 { candidates.push(Rec { platform, encoding, lang, offset, length }); }
    }
    for c in &candidates {
        if c.platform == 3 && c.lang == 0x0409 {
            let Some(start) = string_off.checked_add(c.offset) else { continue; };
            if start + c.length > nt.len() { continue; }
            let raw = &nt[start..start + c.length];
            if c.encoding == 1 || c.encoding == 10 {
                let mut u16s = Vec::with_capacity(c.length / 2);
                for j in (0..c.length).step_by(2) { if j + 2 <= raw.len() { u16s.push(u16::from_be_bytes([raw[j], raw[j + 1]])); } }
                return Some(String::from_utf16_lossy(&u16s));
            }
        }
    }
    for c in &candidates {
        if c.platform == 3 {
            let Some(start) = string_off.checked_add(c.offset) else { continue; };
            if start + c.length > nt.len() { continue; }
            let raw = &nt[start..start + c.length];
            if c.encoding == 1 || c.encoding == 10 {
                let mut u16s = Vec::with_capacity(c.length / 2);
                for j in (0..c.length).step_by(2) { if j + 2 <= raw.len() { u16s.push(u16::from_be_bytes([raw[j], raw[j + 1]])); } }
                return Some(String::from_utf16_lossy(&u16s));
            }
        }
    }
    for c in &candidates {
        if c.platform == 1 {
            let Some(start) = string_off.checked_add(c.offset) else { continue; };
            if start + c.length > nt.len() { continue; }
            return Some(String::from_utf8_lossy(&nt[start..start + c.length]).to_string());
        }
    }
    None
}

pub fn load_private_fonts() -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "gdi32")]
    extern "system" {
        fn AddFontResourceExW(lpszFilename: PCWSTR, fl: u32, pdv: *const std::ffi::c_void) -> i32;
        fn RemoveFontResourceExW(lpFilename: PCWSTR, fl: u32, pdv: *const std::ffi::c_void) -> i32;
    }
    const FR_PRIVATE: u32 = 0x10;
    let dir = std::fs::read_dir("fonts").ok()?;
    let cwd = std::env::current_dir().ok()?;
    let mut entries: Vec<_> = dir.flatten().collect();
    entries.sort_by_key(|e| e.file_name());

    // 收集当前所有字体文件的完整路径
    let mut current: Vec<String> = Vec::new();
    for entry in &entries {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if ext != "ttf" && ext != "otf" { continue; }
        let full = if path.is_absolute() { path.clone() } else { cwd.join(&path) };
        current.push(full.to_string_lossy().to_string());
    }

    unsafe {
        // 读取当前登记表（通过裸指针避免 static_mut_refs 警告）
        let prev = core::ptr::read(core::ptr::addr_of!(REGISTERED_FONTS));
        // 取消注册已删除的字体
        for old in &prev {
            if !current.contains(old) {
                let ws: Vec<u16> = std::ffi::OsStr::new(old).encode_wide().chain(Some(0)).collect();
                RemoveFontResourceExW(PCWSTR(ws.as_ptr()), FR_PRIVATE, std::ptr::null());
            }
        }
        // 注册新增的字体
        for f in &current {
            if !prev.contains(f) {
                let ws: Vec<u16> = std::ffi::OsStr::new(f).encode_wide().chain(Some(0)).collect();
                AddFontResourceExW(PCWSTR(ws.as_ptr()), FR_PRIVATE, std::ptr::null());
            }
        }
        core::ptr::write(core::ptr::addr_of_mut!(REGISTERED_FONTS), current);
    }

    // 返回第一个字体的家族名（给没有配 _font 的场景自动选择）
    for entry in &entries {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if ext != "ttf" && ext != "otf" { continue; }
        let full = if path.is_absolute() { path.clone() } else { cwd.join(&path) };
        if let Ok(data) = std::fs::read(&full) { if let Some(name) = read_font_family(&data) { return Some(name); } }
    }
    None
}

pub fn scan_font_families() -> Vec<String> {
    let dir = match std::fs::read_dir("fonts") {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let cwd = std::env::current_dir().ok();
    let mut result: Vec<String> = Vec::new();
    let mut entries: Vec<_> = dir.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in &entries {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if ext != "ttf" && ext != "otf" { continue; }
        let full = if path.is_absolute() { path } else if let Some(ref cwd) = cwd { cwd.join(&path) } else { continue; };
        if let Ok(data) = std::fs::read(&full) {
            if let Some(name) = read_font_family(&data) {
                if !result.contains(&name) {
                    result.push(name);
                }
            }
        }
    }
    result
}

static mut REGISTERED_FONTS: Vec<String> = Vec::new();

pub static mut MAIN_HWND: usize = 0;

// ── 匹配 ────────────────────────────────────────────────────────

/// 模糊匹配：input 的字符按顺序出现在 key 中（不连续即可）
/// 如 input="gh" → key="GitHub" 匹配（g→G, h→H）
fn fuzzy_match(input: &str, key: &str) -> bool {
    let mut key_chars = key.chars();
    for ic in input.chars() {
        let mut found = false;
        for kc in &mut key_chars {
            if kc == ic {
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
    }
    true
}

/// 获取单个字符的可选读音列表：crate 默认读音 + 用户覆写读音
/// 非中文返回字符自身（已小写）
fn get_readings(c: char, overrides: &HashMap<char, Vec<String>>) -> Vec<String> {
    use pinyin::ToPinyin;
    let mut readings: Vec<String> = Vec::new();
    if let Some(py) = c.to_pinyin() {
        readings.push(py.plain().to_string());
    }
    if let Some(extra) = overrides.get(&c) {
        for r in extra {
            if !readings.contains(r) {
                readings.push(r.clone());
            }
        }
    }
    if readings.is_empty() {
        readings.push(c.to_lowercase().to_string());
    }
    readings
}

/// 逐字前缀匹配：输入能否匹配 key 中从开头逐字消耗的读音
fn pinyin_prefix_match(inp: &str, key: &str, overrides: &HashMap<char, Vec<String>>) -> bool {
    let mut remaining = inp;
    for c in key.chars() {
        if remaining.is_empty() {
            return true;
        }
        let readings = get_readings(c, overrides);
        let mut matched = false;
        for reading in &readings {
            if remaining.starts_with(reading.as_str()) {
                remaining = &remaining[reading.len()..];
                matched = true;
                break;
            }
            // 读音前缀匹配：用户只打了读音的一部分（如 q 还没打完 qie）
            if reading.starts_with(remaining) {
                remaining = "";
                matched = true;
                break;
            }
        }
        if !matched {
            return false;
        }
    }
    remaining.is_empty()
}

/// 拼音子串匹配：输入能从 key 的某个字符位置开始逐字匹配
fn pinyin_substring_match(inp: &str, key: &str, overrides: &HashMap<char, Vec<String>>) -> bool {
    for (i, _) in key.char_indices() {
        if pinyin_prefix_match(inp, &key[i..], overrides) {
            return true;
        }
    }
    false
}

/// 拼音首字母匹配：输入逐个字符匹配每个中文字拼音的首字母
/// 如 input="fq" → key="番茄小说"：f→番(fan), q→茄(qie)
fn pinyin_first_letter_match(inp: &str, key: &str, overrides: &HashMap<char, Vec<String>>) -> bool {
    let mut inp_chars = inp.chars();
    for c in key.chars() {
        let Some(inp_c) = inp_chars.next() else {
            return true;
        };
        let readings = get_readings(c, overrides);
        let matched = readings.iter().any(|r| r.chars().next() == Some(inp_c));
        if !matched {
            return false;
        }
    }
    inp_chars.next().is_none()
}

/// 返回匹配层级：
///   Some(1) = 精确匹配（key）
///   Some(2) = 前缀匹配（key）
///   Some(3) = 子串匹配（key）
///   Some(4) = 拼音前缀（输入为 ASCII 且 ≥ 2 字符）
///   Some(5) = 拼音子串
///   Some(6) = 拼音首字母（输入为 ASCII 且 ≥ 2 字符）
///   Some(7) = 模糊匹配（key，输入至少 2 字符）
///   None    = 不匹配
/// 参数 fuzzy_enabled/pinyin_enabled 控制对应分支是否跳过。
pub fn match_level(input: &str, key: &str, case_sensitive: bool, fuzzy_enabled: bool, pinyin_enabled: bool, overrides: &HashMap<char, Vec<String>>) -> Option<u8> {
    use pinyin::ToPinyin;
    let (inp, k) = if case_sensitive {
        (input.to_string(), key.to_string())
    } else {
        (input.to_lowercase(), key.to_lowercase())
    };

    // 1-3：直接匹配 key
    if inp == k {
        return Some(1);
    }
    if k.starts_with(&inp) {
        return Some(2);
    }
    if k.contains(&inp) {
        return Some(3);
    }

    // 4-5：拼音匹配（逐字尝试，支持多音字覆写）
    if pinyin_enabled && input.chars().count() >= 2 && input.chars().all(|c| c.is_ascii_alphabetic()) {
        // key 不含中文时跳过拼音匹配，走模糊匹配（避免非中文 key 被拼音分支截胡）
        let key_has_chinese = key.chars().any(|c| c.to_pinyin().is_some());
        // 拼音匹配始终用小写输入，不受 _case_sensitive 影响（读音数据本身是小写的）
        let lower_input = input.to_lowercase();
        if key_has_chinese && pinyin_prefix_match(&lower_input, &k, overrides) {
            return Some(4);
        }
        if key_has_chinese && pinyin_substring_match(&lower_input, &k, overrides) {
            return Some(5);
        }
        // 6：拼音首字母（fq → 番茄）
        if key_has_chinese && pinyin_first_letter_match(&lower_input, &k, overrides) {
            return Some(6);
        }
    }

    // 7：模糊匹配
    if fuzzy_enabled && input.chars().count() >= 2 && fuzzy_match(&inp, &k) {
        return Some(7);
    }
    None
}
