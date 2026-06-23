// Gua — Windows 桌面搜索启动器
// 纯 Win32 API，自绘列表，无闪烁

#![cfg(target_os = "windows")]
#![windows_subsystem = "windows"]
mod config;
mod draw;
mod executor;
mod plugin;
mod state;
mod tray;
mod window;
mod wndproc;

use std::ptr;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(
        lpMutexAttributes: *const std::ffi::c_void,
        bInitialOwner: BOOL,
        lpName: PCWSTR,
    ) -> HANDLE;
}
use windows::Win32::Graphics::GdiPlus::{
    GdiplusStartup, GdiplusShutdown, GdiplusStartupInput as GpStartupInput,
};

use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::state::*;



fn main() -> Result<()> {
    // 单例检查：确保只有一个实例在运行
    let mutex_name = to_w("Local\\Gua-Singleton-Mutex");
    let mutex = unsafe { CreateMutexW(std::ptr::null(), BOOL(0), PCWSTR(mutex_name.as_ptr())) };
    if mutex.0.is_null() || unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        if !mutex.0.is_null() {
            unsafe { let _ = CloseHandle(mutex); }
        }
        return Ok(());
    }

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
        // 检查 _font 是否在配置中显式指定
        let has_explicit_font = raw_entries.iter().any(|e| e.key == "_font");
        // 始终加载私有字体（注册到进程），没有显式配置时才自动检测家族名
        let private_font_name = load_private_fonts();
        let font_name = if !has_explicit_font {
            match private_font_name {
                Some(ref name) => name.clone(),
                None => cfg_str(&raw_entries, "_font", "Segoe UI"),
            }
        } else {
            cfg_str(&raw_entries, "_font", "Segoe UI")
        };
        let font_size = cfg_f32(&raw_entries, "_font_size", FW);
        let width = cfg_i32(&raw_entries, "_width", WW);
        let max_results = cfg_usize(&raw_entries, "_max_results", MV);
        let round_corner = cfg_i32(&raw_entries, "_round_corner", 12);
        let always_on_top = cfg_bool(&raw_entries, "_always_on_top", true);
        let opacity = cfg_usize(&raw_entries, "_opacity", 255).min(255) as u8;
        let case_sensitive = cfg_bool(&raw_entries, "_case_sensitive", true);
        let fuzzy_enabled = cfg_bool(&raw_entries, "_fuzzy_match", FUZZY_MATCH_DEFAULT);
        let pinyin_enabled = cfg_bool(&raw_entries, "_pinyin_search", PINYIN_SEARCH_DEFAULT);
        let pinyin_overrides = cfg_pinyin_overrides(&raw_entries, "_pinyin_overrides");
        let hide_on_focus_loss = cfg_bool(&raw_entries, "_hide_on_focus_loss", true);
        let theme_color = cfg_color(&raw_entries, "_theme_color", 0x1E1E1E);
        let input_bg_color = cfg_color(&raw_entries, "_input_bg_color", 0x2A2A2A);
        let accent_color = cfg_color(&raw_entries, "_accent_color", 0x4A6FA5);
        let text_color = cfg_color(&raw_entries, "_text_color", 0xCCCCCC);
        let status_font_size = cfg_f32(&raw_entries, "_status_font_size", 12.0);
        // 面板位置：0~100（0=左上 50=居中 100=右下），转为 0.0~1.0 比例
        let panel_ratio_x = cfg_f32(&raw_entries, "_panel_position_x", 50.0).clamp(0.0, 100.0) / 100.0;
        let panel_ratio_y = cfg_f32(&raw_entries, "_panel_position_y", 50.0).clamp(0.0, 100.0) / 100.0;
        // 热键
        let hotkey_str = cfg_str(&raw_entries, "_hotkey", "Alt+Space");
        let (mod_keys, hotkey_vk) = match parse_hotkey(&hotkey_str) {
            Some(v) => v,
            None => {
                eprintln!("config: 热键 \"{hotkey_str}\" 无法识别，回退为 Alt+Space");
                (MOD_ALT, VK_SPACE)
            }
        };
        // 黑名单
        let blacklist = cfg_blacklist(&raw_entries, "_blacklist");
        let plugin_configs = config::build_plugin_configs(&raw_entries);
        let entries: Vec<config::Entry> = raw_entries.into_iter().filter(|e| !e.key.starts_with('_')).collect();

        let inst = GetModuleHandleW(None)?;

        let cn = to_w("Gua");
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

        let cn2 = to_w("Gua");
        let ex_style = WS_EX_TOOLWINDOW
            | if always_on_top { WS_EX_TOPMOST } else { WINDOW_EX_STYLE::default() };
        let hwnd = CreateWindowExW(
            ex_style,
            PCWSTR(cn2.as_ptr()),
            w!("Gua"),
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
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), opacity, LWA_ALPHA);
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
            fuzzy_enabled,
            pinyin_enabled,
            hide_on_focus_loss,
            theme_color,
            input_bg_color,
            accent_color,
            text_color,
            composing: String::new(),
            config_mtime,
            panel_ratio_x,
            panel_ratio_y,
            mod_keys,
            hotkey_vk,
            blacklist,
            pinyin_overrides,
            last_hide_time: None,
        };

        let boxed = Box::into_raw(Box::new(state));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, boxed as isize);

        if !RegisterHotKey(hwnd, HOTKEY_ID, mod_keys, hotkey_vk).as_bool() {
            eprintln!("config: 热键 \"{hotkey_str}\" 注册失败，可能被其他程序占用");
        }
        tray::init(hwnd);
        plugin::load_all(hwnd, &plugin_configs);

        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 == 0 || ret.0 == -1 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&mut msg);
        }

        plugin::unload_all();
        // 回收 AppState（对应 Box::into_raw）
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        if ptr != 0 {
            drop(Box::from_raw(ptr as *mut AppState));
        }
    }

    unsafe {
        let _ = CloseHandle(mutex);
        GdiplusShutdown(gdiplus_token);
    }
    Ok(())
}

