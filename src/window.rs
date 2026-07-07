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
    fn SetProcessInformation(h: HANDLE, class: i32, info: *const u8, size: u32) -> i32;
}

use crate::executor;
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
    let device = match device { Some(d) => d, None => return Err(Error::from(HRESULT(-2147467259))) };
    let ctx = match ctx { Some(c) => c, None => return Err(Error::from(HRESULT(-2147467259))) };

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
        if let Ok(v) = dcomp.CreateVisual() {
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
    s.text_format = make_text_format(factory, &family, &crate::state::FONT_LOCALE, font_size);
    s.status_text_format = make_text_format(factory, &family, &crate::state::FONT_LOCALE, status_font_size);
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
    s.input_text.clear();
    s.cursor_pos = 0;
    s.sel_start = None;
    s.sel_end = 0;
    s.filtered_indices.clear();
    s.sel_index = 0;
    s.scroll_offset = 0;
    s.search_query.clear();
    s.composing.clear();
    s.input_undo.clear();
    let _ = ShowWindow(h, SW_HIDE);
    let hp = GetCurrentProcess();
    let prio = MemPrio { priority: MEM_PRIO_VERY_LOW };
    let _ = SetProcessInformation(hp, PROCESS_MEMORY_PRIORITY, &prio as *const _ as *const u8, mem::size_of::<MemPrio>() as u32);
}

/// 填充筛选列表并调整窗口高度
pub unsafe fn fill_list(s: &mut AppState, h: HWND) {
    let (key_part, query_part) = if let Some(pos) = s.input_text.find(' ') {
        let (k, _) = s.input_text.split_at(pos);
        (k.to_string(), s.input_text[pos + 1..].to_string())
    } else {
        (s.input_text.clone(), String::new())
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
        let key_part = s.input_text.split(' ').next().unwrap_or("");
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

        s.visible = true;
        s.input_text.clear();
        s.cursor_pos = 0;
        s.filtered_indices.clear();
        s.sel_index = 0;
        s.scroll_offset = 0;
        fill_list(s, h);

        let sh = status_bar_h(s.dpi, s.status_font_size);
        center_win(h, s.width, win_h(s.filtered_indices.len(), s.item_h, s.eh, s.max_results, sh), s.panel_ratio_x, s.panel_ratio_y);

        let _ = ShowWindow(h, SW_SHOW);
        let _ = SetForegroundWindow(h);
        if GetForegroundWindow() != h {
            let _ = SetWindowPos(h, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
            let _ = SetForegroundWindow(h);
        }
        let _ = SetFocus(h);
        let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
    }
}


