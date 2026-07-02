// Gua — Windows 桌面搜索启动器
// DComp + D3D11 + Direct2D 渲染

#![cfg(target_os = "windows")]
#![windows_subsystem = "windows"]
mod config;
mod draw;
mod executor;
mod plugin;
mod settings;
mod state;
mod theme;
mod widget;
mod tray;
mod window;
mod wndproc;

use std::ptr;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::WindowsAndMessaging::*;

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(
        lpMutexAttributes: *const std::ffi::c_void,
        bInitialOwner: BOOL,
        lpName: PCWSTR,
    ) -> HANDLE;
}

use crate::state::*;
use crate::window::*;

fn main() -> Result<()> {
    let mutex_name = to_w("Local\\Gua-Singleton-Mutex");
    let mutex = unsafe { CreateMutexW(std::ptr::null(), BOOL(0), PCWSTR(mutex_name.as_ptr())) };
    if mutex.0.is_null() || unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        if !mutex.0.is_null() { unsafe { let _ = CloseHandle(mutex); } }
        return Ok(());
    }

    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {}\n", info);
        let _ = std::fs::write(config::config_dir().join("panic.log"), &msg);
    }));

    unsafe {
        let _ = SetProcessDPIAware();
        let screen_dc = GetDC(None);
        let dpi = GetDeviceCaps(Some(screen_dc), LOGPIXELSY);
        let _ = ReleaseDC(None, screen_dc);

        let settings = config::load_settings();
        let has_explicit_font = settings.iter().any(|e| e.key == "_font");
        let private_font_name = load_private_fonts();
        let font_name = if !has_explicit_font {
            private_font_name.unwrap_or_else(|| cfg_str(&settings, "_font", "Segoe UI"))
        } else {
            cfg_str(&settings, "_font", "Segoe UI")
        };
        let font_size = cfg_f32(&settings, "_font_size", FW);
        let width = cfg_i32(&settings, "_width", WW);
        let max_results = cfg_usize(&settings, "_max_results", MV);
        let round_corner = cfg_i32(&settings, "_round_corner", 12);
        let opacity = cfg_usize(&settings, "_opacity", 255).min(255) as u8;
        let case_sensitive = cfg_bool(&settings, "_case_sensitive", true);
        let fuzzy_enabled = cfg_bool(&settings, "_fuzzy_match", FUZZY_MATCH_DEFAULT);
        let pinyin_enabled = cfg_bool(&settings, "_pinyin_search", PINYIN_SEARCH_DEFAULT);
        let pinyin_overrides = cfg_pinyin_overrides(&settings, "_pinyin_overrides");
        let hide_on_focus_loss = cfg_bool(&settings, "_hide_on_focus_loss", true);
        let theme_color = cfg_color(&settings, "_theme_color", 0x1E1E1E);
        let input_bg_color = cfg_color(&settings, "_input_bg_color", 0x2A2A2A);
        let accent_color = cfg_color(&settings, "_accent_color", 0x4A6FA5);
        let text_color = cfg_color(&settings, "_text_color", 0xCCCCCC);
        let status_font_size = cfg_f32(&settings, "_status_font_size", 12.0);
        let panel_ratio_x = cfg_f32(&settings, "_panel_position_x", 50.0).clamp(0.0, 100.0) / 100.0;
        let panel_ratio_y = cfg_f32(&settings, "_panel_position_y", 50.0).clamp(0.0, 100.0) / 100.0;
        let hotkey_str = cfg_str(&settings, "_hotkey", "Alt+Space");
        let (mod_keys, hotkey_vk) = match parse_hotkey(&hotkey_str) {
            Some(v) => v,
            None => {
                eprintln!("config: 热键 \"{hotkey_str}\" 无法识别，回退为 Alt+Space");
                let _ = std::fs::write(config::config_dir().join("panic.log"), format!("config: 热键 \"{hotkey_str}\" 无法识别，回退为 Alt+Space\n"));
                (MOD_ALT, VK_SPACE)
            }
        };
        let blacklist = cfg_blacklist(&settings, "_blacklist");
        let plugin_configs = config::build_plugin_configs(&settings);
        let entries = config::load_codes();

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
        if RegisterClassW(&wc) == 0 { return Err(windows::core::Error::from(HRESULT(-2147467259))); }

        let cn2 = to_w("Gua");
        let ex_style = WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP;
        let hwnd = CreateWindowExW(
            ex_style,
            PCWSTR(cn2.as_ptr()),
            w!("Gua"),
            WS_POPUP,
            0, 0, width, 1, None, None, Some(inst.into()), None,
        )?;

        let fp = font_px(font_size, dpi);
        let state = AppState {
            entries,
            input_text: String::new(),
            cursor_pos: 0,
            sel_start: None,
            sel_end: 0,
            search_query: String::new(),
            filtered_indices: Vec::new(),
            sel_index: 0,
            scroll_offset: 0,
            input_rect: RECT { left: PD, top: PD, right: width - PD, bottom: PD + fp + 24 },
            visible: false,
            text_format: None,
            status_text_format: None,
            status_font_size,
            font_name,
            font_size,
            item_h: fp + 20,
            eh: fp + 24,
            dpi,
            max_results,
            width,
            round_corner,
            opacity,
            case_sensitive,
            fuzzy_enabled,
            pinyin_enabled,
            hide_on_focus_loss,
            theme_color,
            input_bg_color,
            accent_color,
            text_color,
            theme_brush: None,
            input_bg_brush: None,
            accent_brush: None,
            text_brush: None,
            white_brush: None,
            renderer: ptr::null_mut(),
            device_recover_attempts: 0,
            composing: String::new(),
            config_mtime: None,
            panel_ratio_x,
            panel_ratio_y,
            codes_cat_state: Vec::new(),
            mod_keys,
            hotkey_vk,
            blacklist,
            pinyin_overrides,
            last_hide_time: None,
        };

        let boxed = Box::into_raw(Box::new(state));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, boxed as isize);
        MAIN_HWND = hwnd.0 as usize;

        // 创建渲染器
        let s = &mut *boxed;
        match create_renderer(hwnd, s) {
            Ok(r) => {
                s.renderer = Box::into_raw(Box::new(r));
                rebuild_text_format(s);
                create_and_cache_brushes(s);
            }
            Err(e) => {
                let msg = format!("renderer: 创建 D2D 渲染器失败!\n{e:?}");
                let w = to_w(&msg);
                let _ = MessageBoxW(None, PCWSTR(w.as_ptr()), w!("Gua"), MB_ICONERROR);
                let _ = CloseHandle(mutex);
                return Err(e);
            }
        }

        // 注册热键
        if !RegisterHotKey(hwnd, HOTKEY_ID, mod_keys, hotkey_vk).as_bool() {
            let msg = format!("config: 热键 \"{hotkey_str}\" 注册失败，可能被其他程序占用");
            let w = to_w(&msg);
            let _ = MessageBoxW(None, PCWSTR(w.as_ptr()), w!("Gua"), MB_ICONWARNING);
        }

        tray::init(hwnd);
        plugin::load_all(hwnd, &plugin_configs);

        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 == 0 || ret.0 == -1 { break; }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&mut msg);
        }

        plugin::unload_all();
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        if ptr != 0 {
            let s = &mut *(ptr as *mut AppState);
            if !s.renderer.is_null() {
                let _ = Box::from_raw(s.renderer);
                s.renderer = ptr::null_mut();
            }
            drop(Box::from_raw(ptr as *mut AppState));
        }

        let _ = CloseHandle(mutex);
    }
    Ok(())
}
