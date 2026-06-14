// System tray icon + context menu (Windows-only, uses windows crate)

#![allow(unused_must_use)]

use std::ptr;
use std::os::windows::ffi::OsStrExt;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const TRAY_MSG: u32 = super::TRAY_MSG;
const TRAY_ID: u32 = super::TRAY_ID;
const IDM_TOGGLE: u16 = super::IDM_TOGGLE;
const IDM_OPEN_CONFIG: u16 = super::IDM_OPEN_CONFIG;
const IDM_EXIT: u16 = super::IDM_EXIT;

static mut TRAY_HWND: HWND = HWND(ptr::null_mut());

pub unsafe fn init(hwnd: HWND) {
    TRAY_HWND = hwnd;

    // 尝试加载 exe 目录下的 KeyHop.ico
    let custom_icon = std::env::current_exe()
        .ok()
        .map(|p| p.with_file_name("KeyHop.ico"))
        .filter(|p| p.exists())
        .and_then(|ico| {
            let ws: Vec<u16> = ico.as_os_str().encode_wide().chain(Some(0)).collect();
            LoadImageW(None, PCWSTR(ws.as_ptr()), IMAGE_ICON, 0, 0, LR_LOADFROMFILE).ok()
        })
        .map(|h| HICON(h.0));

    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
        uCallbackMessage: TRAY_MSG,
        ..Default::default()
    };
    nid.hIcon = custom_icon.unwrap_or_else(|| LoadIconW(None, IDI_APPLICATION).unwrap_or_default());
    let tip: Vec<u16> = "KeyHop\0".encode_utf16().collect();
    for (i, &c) in tip.iter().enumerate() {
        if i < nid.szTip.len() {
            nid.szTip[i] = c;
        }
    }
    Shell_NotifyIconW(NIM_ADD, &nid);
}

pub unsafe fn destroy() {
    let nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: TRAY_HWND,
        uID: TRAY_ID,
        ..Default::default()
    };
    Shell_NotifyIconW(NIM_DELETE, &nid);
}

pub unsafe fn show_menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    let menu = match menu {
        Ok(m) => m,
        _ => return,
    };

    AppendMenuW(menu, MF_STRING, IDM_TOGGLE as usize, w!("打开 KeyHop"));
    AppendMenuW(menu, MF_STRING, IDM_OPEN_CONFIG as usize, w!("打开配置文件"));
    AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR(ptr::null()));
    AppendMenuW(menu, MF_STRING, IDM_EXIT as usize, w!("退出"));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);

    SetForegroundWindow(hwnd);

    TrackPopupMenu(
        menu,
        TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
        pt.x, pt.y,
        Some(0),
        hwnd,
        None,
    );

    let _ = DestroyMenu(menu);
}
