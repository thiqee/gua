// KeyHop — Windows 桌面搜索启动器
// 纯 Win32 API，自绘列表，无闪烁

#![cfg(target_os = "windows")]
#![allow(unused_must_use)]

mod config;
mod draw;
mod executor;
mod state;
mod tray;
mod window;
mod wndproc;

use std::ptr;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::GdiPlus::{
    GdiplusStartup, GdiplusShutdown, GdiplusStartupInput as GpStartupInput,
};

use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::state::*;



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
            lpfnWndProc: Some(wndproc::wndproc),
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

