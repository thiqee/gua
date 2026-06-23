// System tray icon + context menu (Windows-only, uses windows crate)

use std::ptr;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// 编译时嵌入 gua.ico 的字节数据
static GUA_ICO_DATA: &[u8] = include_bytes!("../gua.ico");

const TRAY_MSG: u32 = super::TRAY_MSG;
const TRAY_ID: u32 = super::TRAY_ID;
const IDM_TOGGLE: u16 = super::IDM_TOGGLE;
const IDM_OPEN_CONFIG: u16 = super::IDM_OPEN_CONFIG;
const IDM_EXIT: u16 = super::IDM_EXIT;

static mut TRAY_HWND: HWND = HWND(ptr::null_mut());
static mut TRAY_HICON: HICON = HICON(ptr::null_mut());

/// 从 .ico 文件字节数据中提取第一个图标条目并创建 HICON
unsafe fn load_ico_from_bytes(data: &[u8]) -> Option<HICON> {
    if data.len() < 6 { return None; }
    let count = u16::from_le_bytes([data[4], data[5]]) as usize;
    if count == 0 { return None; }
    // 取第一个条目（ICONDIRENTRY 从偏移 6 开始，每条 16 字节）
    let entry_off = 6;
    if entry_off + 16 > data.len() { return None; }
    let img_off = u32::from_le_bytes([
        data[entry_off + 12], data[entry_off + 13],
        data[entry_off + 14], data[entry_off + 15],
    ]) as usize;
    let img_size = u32::from_le_bytes([
        data[entry_off + 8], data[entry_off + 9],
        data[entry_off + 10], data[entry_off + 11],
    ]) as usize;
    if img_off + img_size > data.len() { return None; }
    CreateIconFromResourceEx(&data[img_off..img_off + img_size], true, 0x00030000, 0, 0, IMAGE_FLAGS(0)).ok()
}

pub unsafe fn init(hwnd: HWND) {
    TRAY_HWND = hwnd;

    // 从内嵌的字节数据创建图标
    let hicon = load_ico_from_bytes(GUA_ICO_DATA).unwrap_or_else(|| LoadIconW(None, IDI_APPLICATION).unwrap_or_default());

    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
        uCallbackMessage: TRAY_MSG,
        ..Default::default()
    };
    nid.hIcon = hicon;
    let tip: Vec<u16> = "Gua\0".encode_utf16().collect();
    for (i, &c) in tip.iter().enumerate() {
        if i < nid.szTip.len() {
            nid.szTip[i] = c;
        }
    }
    let _ = Shell_NotifyIconW(NIM_ADD, &nid);
    TRAY_HICON = hicon;
}

pub unsafe fn destroy() {
    let _ = DestroyIcon(TRAY_HICON);
    let nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: TRAY_HWND,
        uID: TRAY_ID,
        ..Default::default()
    };
    let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
}

pub unsafe fn show_menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    let menu = match menu {
        Ok(m) => m,
        _ => return,
    };

    let _ = AppendMenuW(menu, MF_STRING, IDM_TOGGLE as usize, w!("打开 Gua"));
    let _ = AppendMenuW(menu, MF_STRING, IDM_OPEN_CONFIG as usize, w!("打开配置文件"));
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR(ptr::null()));
    let _ = AppendMenuW(menu, MF_STRING, IDM_EXIT as usize, w!("退出"));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);

    let _ = SetForegroundWindow(hwnd);

    let _ = TrackPopupMenu(
        menu,
        TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
        pt.x, pt.y,
        Some(0),
        hwnd,
        None,
    );

    let _ = DestroyMenu(menu);
}
