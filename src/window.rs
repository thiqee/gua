// Gua — 窗口管理

use std::mem;
use std::ptr;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Dxgi::Common::*;

#[link(name = "kernel32")]
extern "system" {
    fn SetProcessWorkingSetSize(h: HANDLE, min: usize, max: usize) -> i32;
    fn SetProcessInformation(h: HANDLE, class: i32, info: *const u8, size: u32) -> i32;
}

use crate::config;
use crate::executor;
use crate::plugin;
use crate::state::*;

/// 创建 D2D 渲染器（Composition SwapChain + DComp，透明圆角均正常工作）
pub unsafe fn create_renderer(hwnd: HWND, s: &AppState) -> Result<GuaRenderer> {
    let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    if hr.0 != 0 && hr.0 != 1 {
        return Err(Error::from(hr));
    }
    let com_initialized = hr.0 == 0;

    let mut device: Option<ID3D11Device> = None;
    let mut ctx: Option<ID3D11DeviceContext> = None;
    D3D11CreateDevice(
        None as Option<&IDXGIAdapter>,
        D3D_DRIVER_TYPE_HARDWARE,
        HMODULE::default(),
        D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        None,
        D3D11_SDK_VERSION,
        Some(&mut device),
        None,
        Some(&mut ctx),
    )?;
    let device = device.unwrap();
    let ctx = ctx.unwrap();

    let dxgi_device: IDXGIDevice = device.cast()?;
    let adapter = dxgi_device.GetAdapter()?;
    let dxgi_factory: IDXGIFactory2 = adapter.GetParent()?;

    let supports_tearing = {
        let mut feature = DXGI_FEATURE::default();
        let factory5: IDXGIFactory5 = dxgi_factory.cast()?;
        factory5.CheckFeatureSupport(
            DXGI_FEATURE_PRESENT_ALLOW_TEARING,
            &mut feature as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<DXGI_FEATURE>() as u32,
        ).is_ok() && feature.0 != 0
    };

    // Composition 交换链（透明通道 + 抗锯齿圆角均支持）
    let swap_desc = DXGI_SWAP_CHAIN_DESC1 {
        Width: s.width.max(1) as u32,
        Height: 1u32,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        Stereo: false.into(),
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 2,
        Scaling: DXGI_SCALING_STRETCH,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
        AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
        Flags: DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING.0 as u32,
    };
    let swap_chain = dxgi_factory.CreateSwapChainForComposition(&device, &swap_desc, None)?;

    let d2d_factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
    let d2d_device = d2d_factory.CreateDevice(&dxgi_device)?;
    let d2d_context = d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;

    let back_buffer: IDXGISurface = swap_chain.GetBuffer(0)?;
    let props = D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
        colorContext: std::mem::ManuallyDrop::new(None),
    };
    let target = d2d_context.CreateBitmapFromDxgiSurface(&back_buffer, Some(&props))?;
    d2d_context.SetTarget(&target);

    let dwrite_factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;

    // DComp：承载交换链 + 圆角裁剪（需持有 IDCompositionTarget 防止释放）
    let dcomp_device: Option<IDCompositionDevice> = DCompositionCreateDevice::<_, IDCompositionDevice>(&dxgi_device).ok();
    let mut dcomp_visual: Option<IDCompositionVisual> = None;
    let mut dcomp_target: Option<IDCompositionTarget> = None;
    if let Some(ref dcomp) = dcomp_device {
        if let Some(v) = dcomp.CreateVisual().ok() {
            let _ = v.SetContent(&swap_chain);
            if let Ok(t) = dcomp.CreateTargetForHwnd(hwnd, true) {
                let _ = t.SetRoot(&v);
                dcomp_target = Some(t);
            }
            let _ = dcomp.Commit();
            dcomp_visual = Some(v);
        }
    }

    Ok(GuaRenderer {
        d3d_device: device,
        d3d_context: ctx,
        dxgi_factory,
        swap_chain,
        supports_tearing,
        d2d_factory,
        d2d_device,
        d2d_context,
        dwrite_factory,
        target: Some(target),
        dcomp_device,
        dcomp_visual,
        dcomp_target,
        com_initialized,
    })
}

/// 重建设备丢失后的渲染器
pub unsafe fn recreate_renderer(s: &mut AppState, h: HWND) {
    s.theme_brush = None;
    s.input_bg_brush = None;
    s.accent_brush = None;
    s.text_brush = None;
    s.white_brush = None;
    s.text_format = None;
    s.status_text_format = None;

    if !s.renderer.is_null() {
        let r = Box::from_raw(s.renderer);
        drop(r);
    }
    s.renderer = ptr::null_mut();

    match create_renderer(h, s) {
        Ok(r) => {
            s.renderer = Box::into_raw(Box::new(r));
            create_and_cache_brushes(s);
            rebuild_text_format(s);
        }
        Err(_) => {
            s.renderer = ptr::null_mut();
        }
    }
}

/// 创建或更新画刷缓存
pub unsafe fn create_and_cache_brushes(s: &mut AppState) {
    let d2d = match gua_renderer(s) { Some(r) => &r.d2d_context as *const ID2D1DeviceContext, None => return };
    let theme = s.theme_color; let input = s.input_bg_color; let accent = s.accent_color; let text = s.text_color; let op = s.opacity;
    s.theme_brush = create_brush(d2d, theme, op as f32 / 255.0);
    s.input_bg_brush = create_brush(d2d, input, op as f32 / 255.0);
    s.accent_brush = create_brush(d2d, accent, 1.0);
    s.text_brush = create_brush(d2d, text, 1.0);
    let white = D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    s.white_brush = unsafe { (*d2d).CreateSolidColorBrush(&white as *const _, None).ok() };
}

unsafe fn create_brush(d2d: *const ID2D1DeviceContext, rgb: u32, alpha: f32) -> Option<ID2D1SolidColorBrush> {
    let color = color_to_d2d(rgb, alpha);
    (*d2d).CreateSolidColorBrush(&color as *const _, None).ok()
}

/// 重建 DWrite TextFormat
pub unsafe fn rebuild_text_format(s: &mut AppState) {
    s.text_format = None;
    s.status_text_format = None;
    let factory = match gua_renderer(s) { Some(r) => &r.dwrite_factory as *const IDWriteFactory, None => return };
    let family = to_w(&s.font_name);
    let font_size = s.font_size;
    let status_font_size = s.status_font_size;
    s.text_format = make_text_format(factory, &family, &*crate::state::FONT_LOCALE, font_size);
    s.status_text_format = make_text_format(factory, &family, &*crate::state::FONT_LOCALE, status_font_size);
    if let Some(ref tf) = s.status_text_format {
        let _ = tf.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING);
    }
}

unsafe fn make_text_format(factory: *const IDWriteFactory, family: &[u16], locale: &[u16], sz: f32) -> Option<IDWriteTextFormat> {
    // 通知 DWrite 刷新字体缓存（拾取刚刚用 AddFontResourceExW 注册的私有字体）
    let mut coll: Option<IDWriteFontCollection> = None;
    let _ = (*factory).GetSystemFontCollection(&mut coll, true);
    let tf = (*factory).CreateTextFormat(
        PCWSTR(family.as_ptr()),
        None as Option<&IDWriteFontCollection>,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL,
        sz,
        PCWSTR(locale.as_ptr()),
    ).ok()?;
    let _ = tf.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
    Some(tf)
}

/// 测量文本宽度（像素）
pub unsafe fn measure_text_width(s: &AppState, text: &str) -> f32 {
    let r = match gua_renderer(s) { Some(r) => r, None => return 0.0 };
    let tf = match s.text_format { Some(ref tf) => tf, None => return 0.0 };
    let ws: Vec<u16> = text.encode_utf16().collect();
    if ws.is_empty() { return 0.0; }
    if let Ok(layout) = r.dwrite_factory.CreateTextLayout(&ws, tf, 10000.0, 10000.0) {
        let mut metrics = DWRITE_TEXT_METRICS::default();
        if layout.GetMetrics(&mut metrics).is_ok() {
            return metrics.widthIncludingTrailingWhitespace;
        }
    }
    0.0
}

/// 隐藏窗口并清空状态
pub unsafe fn hide_clear(h: HWND, s: &mut AppState) {
    s.last_hide_time = Some(std::time::Instant::now());
    s.visible = false;
    s.filter.clear();
    s.input_text.clear();
    s.cursor_pos = 0;
    s.filtered_indices.clear();
    s.sel_index = 0;
    s.scroll_offset = 0;
    s.search_query.clear();
    s.composing.clear();
    let _ = ShowWindow(h, SW_HIDE);
    let hp = GetCurrentProcess();
    let prio = MemPrio { priority: MEM_PRIO_VERY_LOW };
    let _ = SetProcessInformation(hp, PROCESS_MEMORY_PRIORITY, &prio as *const _ as *const u8, mem::size_of::<MemPrio>() as u32);
    let _ = SetProcessWorkingSetSize(hp, usize::MAX, usize::MAX);
}

/// 填充筛选列表并调整窗口高度
pub unsafe fn fill_list(s: &mut AppState, h: HWND) {
    let (key_part, query_part) = if let Some(pos) = s.filter.find(' ') {
        let (k, _) = s.filter.split_at(pos);
        (k.to_string(), s.filter[pos + 1..].to_string())
    } else {
        (s.filter.clone(), String::new())
    };
    s.search_query = query_part;
    s.filtered_indices.clear();

    if !key_part.is_empty() {
        let mut matched: Vec<(u8, usize, usize)> = Vec::new();
        for (i, e) in s.entries.iter().enumerate() {
            if let Some(level) = match_level(&key_part, &e.key, s.case_sensitive, s.fuzzy_enabled, s.pinyin_enabled, &s.pinyin_overrides) {
                matched.push((level, e.key.len(), i));
            }
        }
        matched.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        s.filtered_indices = matched.into_iter().map(|(_, _, i)| i).collect();
    }

    let n = s.filtered_indices.len();
    let sh = status_bar_h(s.dpi, s.status_font_size);
    let nh = win_h(n, s.item_h, s.eh, s.max_results, sh);
    let mut rc = RECT::default();
    let _ = GetWindowRect(h, &mut rc);
    let cur_h = rc.bottom - rc.top;
    if cur_h != nh {
        let _ = SetWindowPos(h, Some(HWND_TOP), rc.left, rc.top, s.width, nh, SWP_NOZORDER);
    }

    s.sel_index = 0;
    s.scroll_offset = 0;
}

/// 执行当前选中项
pub unsafe fn execute_sel(h: HWND, s: &mut AppState) {
    let idx = if !s.search_query.is_empty() {
        let key_part = s.filter.split(' ').next().unwrap_or("");
        s.entries.iter().enumerate()
            .filter(|(_, e)| e.key == key_part)
            .max_by_key(|(_, e)| {
                let v = &e.value;
                (v.starts_with("http://") || v.starts_with("https://")) && v.ends_with('=')
            } as usize)
            .map(|(i, _)| i)
    } else {
        None
    };
    let idx = idx.or_else(|| {
        if s.sel_index < s.filtered_indices.len() {
            Some(s.filtered_indices[s.sel_index])
        } else {
            None
        }
    });
    if let Some(idx) = idx {
        if idx < s.entries.len() {
            executor::execute(&s.entries[idx].key, &s.entries[idx].value, &s.search_query);
            hide_clear(h, s);
        }
    }
}

/// 重建 DWrite TextFormat（响应 DPI 变化）
pub unsafe fn rebuild_font(s: &mut AppState, dpi: i32) {
    let fp = font_px(s.font_size, dpi);
    s.eh = fp + 24;
    s.item_h = fp + 20;
    rebuild_text_format(s);
}

/// 热重载配置
pub unsafe fn reload_config(h: HWND, s: &mut AppState) -> (bool, String, f32) {
    let set_path = config::settings_path();
    let cur = std::fs::metadata(&set_path)
        .ok()
        .and_then(|m| m.modified().ok());
    let mut font_name = s.font_name.clone();
    let mut font_size = s.font_size;
    let config_changed = s.config_mtime != cur;
    if config_changed {
        let settings = config::load_settings();
        let has_explicit_font = settings.iter().any(|e| e.key == "_font");
        let private_font_name = load_private_fonts();
        font_name = if has_explicit_font {
            cfg_str(&settings, "_font", &s.font_name)
        } else {
            private_font_name.unwrap_or_else(|| cfg_str(&settings, "_font", &s.font_name))
        };
        font_size = cfg_f32(&settings, "_font_size", s.font_size);
        s.max_results = cfg_usize(&settings, "_max_results", s.max_results);
        s.width = cfg_i32(&settings, "_width", s.width);
        s.round_corner = cfg_i32(&settings, "_round_corner", s.round_corner);
        s.hide_on_focus_loss = cfg_bool(&settings, "_hide_on_focus_loss", s.hide_on_focus_loss);
        let old_theme = s.theme_color;
        let old_input_bg = s.input_bg_color;
        let old_accent = s.accent_color;
        let old_text = s.text_color;
        let old_opacity = s.opacity;
        s.theme_color = cfg_color(&settings, "_theme_color", s.theme_color);
        s.input_bg_color = cfg_color(&settings, "_input_bg_color", s.input_bg_color);
        s.accent_color = cfg_color(&settings, "_accent_color", s.accent_color);
        s.text_color = cfg_color(&settings, "_text_color", s.text_color);
        let old_status_font_size = s.status_font_size;
        s.status_font_size = cfg_f32(&settings, "_status_font_size", s.status_font_size);
        s.opacity = cfg_usize(&settings, "_opacity", s.opacity as usize).min(255) as u8;
        s.case_sensitive = cfg_bool(&settings, "_case_sensitive", s.case_sensitive);
        s.fuzzy_enabled = cfg_bool(&settings, "_fuzzy_match", s.fuzzy_enabled);
        s.pinyin_enabled = cfg_bool(&settings, "_pinyin_search", s.pinyin_enabled);
        s.panel_ratio_x = cfg_f32(&settings, "_panel_position_x", 50.0).clamp(0.0, 100.0) / 100.0;
        s.panel_ratio_y = cfg_f32(&settings, "_panel_position_y", 50.0).clamp(0.0, 100.0) / 100.0;

        // 更新画刷颜色（SetColor 不重建）
        if old_theme != s.theme_color || old_opacity != s.opacity {
            if let Some(ref brush) = s.theme_brush {
                let c = color_to_d2d(s.theme_color, s.opacity as f32 / 255.0);
                brush.SetColor(&c as *const _);
            }
        }
        if old_input_bg != s.input_bg_color || old_opacity != s.opacity {
            if let Some(ref brush) = s.input_bg_brush {
                let c = color_to_d2d(s.input_bg_color, s.opacity as f32 / 255.0);
                brush.SetColor(&c as *const _);
            }
        }
        if old_accent != s.accent_color {
            if let Some(ref brush) = s.accent_brush {
                let c = color_to_d2d(s.accent_color, 1.0);
                brush.SetColor(&c as *const _);
            }
        }
        if old_text != s.text_color {
            if let Some(ref brush) = s.text_brush {
                let c = color_to_d2d(s.text_color, 1.0);
                brush.SetColor(&c as *const _);
            }
        }

        // 状态栏字号变更时重建 TextFormat
        if (old_status_font_size - s.status_font_size).abs() > 0.1 {
            rebuild_text_format(s);
        }

        // 热键变更
        let new_hotkey_str = cfg_str(&settings, "_hotkey", "Alt+Space");
        if let Some((new_mod, new_vk)) = parse_hotkey(&new_hotkey_str) {
            if new_mod != s.mod_keys || new_vk != s.hotkey_vk {
                let _ = UnregisterHotKey(h, HOTKEY_ID);
                if RegisterHotKey(h, HOTKEY_ID, new_mod, new_vk).as_bool() {
                    s.mod_keys = new_mod;
                    s.hotkey_vk = new_vk;
                } else {
                    eprintln!("config: 新热键 \"{new_hotkey_str}\" 注册失败，恢复原热键");
                    let _ = std::fs::write(config::config_dir().join("panic.log"), format!("config: 新热键 \"{new_hotkey_str}\" 注册失败，恢复原热键\n"));
                    let _ = RegisterHotKey(h, HOTKEY_ID, s.mod_keys, s.hotkey_vk);
                }
            }
        } else {
            eprintln!("config: 新热键 \"{new_hotkey_str}\" 无法识别，保持原热键");
            let _ = std::fs::write(config::config_dir().join("panic.log"), format!("config: 新热键 \"{new_hotkey_str}\" 无法识别，保持原热键\n"));
        }

        s.blacklist = cfg_blacklist(&settings, "_blacklist");
        s.pinyin_overrides = cfg_pinyin_overrides(&settings, "_pinyin_overrides");
        let plugin_configs = config::build_plugin_configs(&settings);
        s.entries = config::load_codes();
        s.config_mtime = cur;
        plugin::notify_reload(&plugin_configs);

    }
    (config_changed, font_name, font_size)
}

/// 切换窗口可见状态
pub unsafe fn toggle_win(h: HWND, s: &mut AppState) {
    if !s.blacklist.is_empty() {
        if let Some(exe) = get_foreground_exe() {
            if s.blacklist.iter().any(|b| b.eq_ignore_ascii_case(&exe)) {
                return;
            }
        }
    }

    if s.visible {
        hide_clear(h, s);
    } else {
        let hp = GetCurrentProcess();
        let prio = MemPrio { priority: MEM_PRIO_NORMAL };
        let _ = SetProcessInformation(hp, PROCESS_MEMORY_PRIORITY, &prio as *const _ as *const u8, mem::size_of::<MemPrio>() as u32);

        let (config_changed, font_name, font_size) = reload_config(h, s);

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

        if config_changed || s.config_mtime.is_none() {
            let sh = status_bar_h(s.dpi, s.status_font_size);
            center_win(h, s.width, win_h(0, s.item_h, s.eh, s.max_results, sh), s.panel_ratio_x, s.panel_ratio_y);
        }

        let _ = ShowWindow(h, SW_SHOW);
        let _ = SetForegroundWindow(h);
        if GetForegroundWindow() != h {
            let _ = SetWindowPos(h, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
            let _ = SetForegroundWindow(h);
        }
        let _ = SetFocus(h);
        // 强制立即重绘
        let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
    }
}


