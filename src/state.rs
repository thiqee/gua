// Gua — 数据结构、常量、工具函数

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
    /// 当前热键修饰键位掩码，由 _hotkey 配置解析
    pub mod_keys: u32,
    /// 当前热键虚拟键码，由 _hotkey 配置解析
    pub hotkey_vk: u32,
    /// 黑名单程序 exe 文件名列表（小写）。当前台窗口在其中时热键不响应。
    pub blacklist: Vec<String>,
    /// 上次隐藏的时间戳，用于托盘点击防抖（避免失焦隐藏后立即被托盘消息重新唤起）
    pub last_hide_time: Option<std::time::Instant>,
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

// ── 私有字体加载 ────────────────────────────────────────────────

/// 从字体文件的 name table 中读取 Font Family 名称（name ID = 1）
/// Windows 平台优先，其次 Mac 平台；英文优先，其他语言次之。
fn read_font_family(data: &[u8]) -> Option<String> {
    let buf = |off: usize, len: usize| -> Option<&[u8]> {
        data.get(off..off + len)
    };
    let u16be = |off: usize| -> Option<u16> {
        let b = buf(off, 2)?;
        Some(u16::from_be_bytes([b[0], b[1]]))
    };
    let u32be = |off: usize| -> Option<u32> {
        let b = buf(off, 4)?;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };

    let num_tables = u16be(4)? as usize;

    // 在 table directory 中查找 "name" 表
    let mut name_off = None;
    let mut name_len = None;
    for i in 0..num_tables {
        let entry = 12 + i * 16;
        let tag = buf(entry, 4)?;
        if tag == b"name" {
            name_off = Some(u32be(entry + 8)? as usize);
            name_len = Some(u32be(entry + 12)? as usize);
            break;
        }
    }
    let name_off = name_off?;
    let name_len = name_len?;
    let nt = buf(name_off, name_len)?;

    let format = u16be(name_off)?;
    let count = u16be(name_off + 2)? as usize;
    let string_off = u16be(name_off + 4)? as usize;

    // 计算 NameRecord 起始偏移：format=1 时有额外的语言标签区
    let name_record_off = if format == 0 {
        name_off + 6
    } else if format == 1 {
        let lang_tag_count = u16be(name_off + 6)? as usize;
        name_off + 6 + 2 + lang_tag_count * 12
    } else {
        return None;
    };

    // 收集所有 nameID = 1 的记录，优先 Windows/英文
    struct Rec {
        platform: u16,
        encoding: u16,
        lang: u16,
        offset: usize,
        length: usize,
    }
    let mut candidates: Vec<Rec> = Vec::new();
    for i in 0..count {
        let r = name_record_off + i * 12;
        let platform = u16be(r)?;
        let encoding = u16be(r + 2)?;
        let lang = u16be(r + 4)?;
        let name_id = u16be(r + 6)?;
        let length = u16be(r + 8)? as usize;
        let offset = u16be(r + 10)? as usize;
        if name_id == 1 {
            candidates.push(Rec { platform, encoding, lang, offset, length });
        }
    }

    // 优先 Windows (platform=3) + 英文 (lang=0x0409)
    for c in &candidates {
        if c.platform == 3 && c.lang == 0x0409 {
            let start = string_off + c.offset;
            if start + c.length > nt.len() { continue; }
            let raw = &nt[start..start + c.length];
            if c.encoding == 1 || c.encoding == 10 {
                // UTF-16BE
                let mut u16s = Vec::with_capacity(c.length / 2);
                for j in (0..c.length).step_by(2) {
                    if j + 2 <= raw.len() {
                        u16s.push(u16::from_be_bytes([raw[j], raw[j + 1]]));
                    }
                }
                return Some(String::from_utf16_lossy(&u16s));
            }
        }
    }
    // 其次 Windows 任意语言
    for c in &candidates {
        if c.platform == 3 {
            let start = string_off + c.offset;
            if start + c.length > nt.len() { continue; }
            let raw = &nt[start..start + c.length];
            if c.encoding == 1 || c.encoding == 10 {
                let mut u16s = Vec::with_capacity(c.length / 2);
                for j in (0..c.length).step_by(2) {
                    if j + 2 <= raw.len() {
                        u16s.push(u16::from_be_bytes([raw[j], raw[j + 1]]));
                    }
                }
                return Some(String::from_utf16_lossy(&u16s));
            }
        }
    }
    // 最后 Mac（platform=1，ASCII/MacRoman）
    for c in &candidates {
        if c.platform == 1 {
            let start = string_off + c.offset;
            if start + c.length > nt.len() { continue; }
            let raw = &nt[start..start + c.length];
            return Some(String::from_utf8_lossy(raw).to_string());
        }
    }
    None
}

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

/// 返回匹配层级：
///   Some(1) = 精确匹配（key == input）
///   Some(2) = 前缀匹配（key 以 input 开头）
///   Some(3) = 子串匹配（key 包含 input）
///   Some(4) = 模糊匹配（input 字符按顺序出现在 key 中，输入至少 2 字符）
///   None    = 不匹配
pub fn match_level(input: &str, key: &str, case_sensitive: bool) -> Option<u8> {
    let (inp, k) = if case_sensitive {
        (input.to_string(), key.to_string())
    } else {
        (input.to_lowercase(), key.to_lowercase())
    };

    if inp == k {
        return Some(1);
    }
    if k.starts_with(&inp) {
        return Some(2);
    }
    if k.contains(&inp) {
        return Some(3);
    }
    // 输入至少 2 个字符才走模糊匹配，避免单字符命中大量结果
    if input.chars().count() >= 2 && fuzzy_match(&inp, &k) {
        return Some(4);
    }
    None
}

/// 扫描 fonts/ 目录，注册第一个字体并返回其家族名称
pub fn load_private_fonts() -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "gdi32")]
    extern "system" {
        fn AddFontResourceExW(
            lpszFilename: PCWSTR,
            fl: u32,
            pdv: *const std::ffi::c_void,
        ) -> i32;
    }
    const FR_PRIVATE: u32 = 0x10;

    let dir = std::fs::read_dir("fonts").ok()?;
    let cwd = std::env::current_dir().ok()?;

    let mut entries: Vec<_> = dir.flatten().collect();
    // 按文件名排序，保证"第一个"稳定
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let path = entry.path();
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext != "ttf" && ext != "otf" {
            continue;
        }
        let full = if path.is_absolute() { path.clone() } else { cwd.join(&path) };
        let ws: Vec<u16> = full.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            AddFontResourceExW(PCWSTR(ws.as_ptr()), FR_PRIVATE, std::ptr::null());
        }
        // 读取家族名称
        if let Ok(data) = std::fs::read(&full) {
            if let Some(name) = read_font_family(&data) {
                return Some(name);
            }
        }
    }
    None
}
