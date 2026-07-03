// Gua — 窗口过程（消息处理）

use std::mem;
use std::ptr;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

#[link(name = "user32")]
extern "system" {
    fn GetKeyState(vk: i32) -> i16;
}

#[link(name = "kernel32")]
extern "system" {
    fn SetProcessInformation(h: HANDLE, class: i32, info: *const u8, size: u32) -> i32;
}

use crate::config;
use crate::draw::*;
use crate::plugin;
use crate::state::*;
use crate::theme;
use crate::widget::{clipboard_copy, clipboard_paste};
use crate::tray;
use crate::window::*;

#[link(name = "imm32")]
extern "system" {
    fn ImmGetContext(hwnd: HWND) -> isize;
    fn ImmSetCompositionWindow(himc: isize, lpCompForm: *const COMPOSITIONFORM) -> BOOL;
    fn ImmGetCompositionStringW(himc: isize, dwIndex: u32, lpBuf: *mut std::ffi::c_void, dwBufLen: u32) -> u32;
    fn ImmReleaseContext(hwnd: HWND, himc: isize) -> BOOL;
}

const D2DERR_RECREATE_TARGET: i32 = 0x88990002_u32 as _;

unsafe fn is_device_lost(hr: &HRESULT) -> bool {
    hr.0 == DXGI_ERROR_DEVICE_REMOVED.0 || hr.0 == DXGI_ERROR_DEVICE_RESET.0 || hr.0 == D2DERR_RECREATE_TARGET
}

fn push_undo(s: &mut AppState) {
    s.input_undo.push((s.input_text.clone(), s.cursor_pos));
    if s.input_undo.len() > 30 {
        s.input_undo.remove(0);
    }
}

pub unsafe extern "system" fn wndproc(
    h: HWND, msg: u32, wp: WPARAM, lp: LPARAM,
) -> LRESULT {
    let ptr = GetWindowLongPtrW(h, GWLP_USERDATA);
    if ptr == 0 {
        match msg {
            WM_MEASUREITEM | WM_CREATE | WM_NCCREATE => return LRESULT(1),
            _ => return DefWindowProcW(h, msg, wp, lp),
        }
    }
    let s = &mut *(ptr as *mut AppState);

    match msg {
        WM_ERASEBKGND => return LRESULT(1),

        WM_PAINT => {
            if s.device_recover_attempts > 0 || s.renderer.is_null() {
                let mut ps = PAINTSTRUCT::default();
                BeginPaint(h, &mut ps);
                let _ = EndPaint(h, &ps);
                return LRESULT(0);
            }

            let mut ps = PAINTSTRUCT::default();
            BeginPaint(h, &mut ps);

            let r = match gua_renderer(s) {
                Some(r) => r,
                None => { let _ = EndPaint(h, &ps); return LRESULT(0); }
            };

            if let Some(ref target) = r.target {
                r.d2d_context.SetTarget(target);
            }

            let total = s.filtered_indices.len();
            let vis = total.min(s.max_results);
            let lh = s.item_h;
            let ly = list_y(&s.input_rect) as f32;
            let sh = status_bar_h(s.dpi, s.status_font_size);
            let win_height = win_h(total, lh, s.eh, s.max_results, sh) as f32;

            let _ = r.d2d_context.BeginDraw();

            // 透明清屏（PREMULTIPLIED 下透明部分会被 DComp 合成，实现圆角透出桌面）
            let clear = D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
            let _ = r.d2d_context.Clear(Some(&clear as *const _));

            // 窗口背景圆角（覆盖在主题色之上，DComp clip 裁掉角落后自然透明）
            let w = s.width as f32;
            let rc = s.round_corner as f32;
            if let Some(ref brush) = s.theme_brush {
                d2d_fill_round_rect(&r.d2d_context, 0.0, 0.0, w, win_height, rc, brush);
            }

            // 输入框背景
            let ir = &s.input_rect;
            let ic = (s.round_corner * 3 / 4).max(1) as f32;
            if let Some(ref brush) = s.input_bg_brush {
                d2d_fill_round_rect(&r.d2d_context,
                    ir.left as f32, ir.top as f32,
                    (ir.right - ir.left) as f32, (ir.bottom - ir.top) as f32,
                    ic, brush);
            }

            // 输入框文字 + 光标（含 IME 拼写中文字，一起显示在光标处）
            if let Some(ref tf) = s.text_format {
                if let Some(ref text_brush) = s.text_brush {
                    let fh = font_px(s.font_size, s.dpi) as f32;
                    // 文字用整个输入框高度，DWrite 的 SetParagraphAlignment(CENTER) 自动垂直居中
                    let input_rect = D2D_RECT_F {
                        left: ir.left as f32 + 8.0,
                        top: ir.top as f32,
                        right: ir.right as f32 - 4.0,
                        bottom: ir.bottom as f32,
                    };
                    let display = s.input_text.replace("&", "&&");
                    let comp_display = s.composing.replace("&", "&&");

                    // 完整文字 = 已输入 + 拼写中
                    let full_text = if comp_display.is_empty() {
                        display.clone()
                    } else {
                        format!("{}{}", display, comp_display)
                    };
                    d2d_draw_text(&r.d2d_context, &full_text, tf, &input_rect, text_brush);

                    // 选中高亮
                    if s.visible && comp_display.is_empty() {
                        if let Some(ss) = s.sel_start {
                            if s.sel_end > ss {
                                let left = ir.left as f32 + 8.0 + measure_text_width(s, &s.input_text[..ss].replace("&", "&&"));
                                let right = ir.left as f32 + 8.0 + measure_text_width(s, &s.input_text[..s.sel_end].replace("&", "&&"));
                                let cy = ir.top as f32 + ((ir.bottom - ir.top) as f32 - fh) / 2.0;
                                if let Some(b) = theme::brush(&r.d2d_context, theme::T.accent, 0.25) {
                                    d2d_fill_round_rect(&r.d2d_context, left, cy, right - left, fh, 2.0, &b);
                                }
                            }
                        }
                    }

                    // 光标位置 = 已输入 + 拼写中（append 模式）
                    if s.visible {
                        let text_before_caret = if comp_display.is_empty() {
                            let prefix = &s.input_text[..s.cursor_pos];
                            prefix.replace("&", "&&")
                        } else {
                            // 拼写中：光标在已经输入的文字 + 全部拼写文字之后
                            format!("{}{}", display, comp_display)
                        };
                        let caret_x = ir.left as f32 + 8.0 + measure_text_width(s, &text_before_caret);
                        let cy = ir.top as f32 + ((ir.bottom - ir.top) as f32 - fh) / 2.0;
                        if let Some(ref wb) = s.white_brush {
                            d2d_fill_round_rect(&r.d2d_context, caret_x, cy, 2.0, fh, 0.0, wb);
                        }
                    }
                }
            }

            // 列表项
            let start = s.scroll_offset.min(total.saturating_sub(vis));
            for i in 0..vis {
                let fi = start + i;
                if fi >= total { break; }
                let y = ly + i as f32 * lh as f32;
                let item_rect = D2D_RECT_F {
                    left: PD as f32,
                    top: y,
                    right: w - PD as f32,
                    bottom: y + lh as f32,
                };
                d2d_draw_filtered_item(&r.d2d_context, s, fi, &item_rect);
            }

            if vis > 0 {
                d2d_redraw_status_bar(&r.d2d_context, s, ly, vis);
            }

            // EndDraw
            if let Err(e) = r.d2d_context.EndDraw(None, None) {
                if is_device_lost(&e.code()) {
                    s.device_recover_attempts = 1;
                    let _ = EndPaint(h, &ps);
                    recreate_renderer(s, h);
                    if s.renderer.is_null() {
                        return LRESULT(0);
                    }
                    s.device_recover_attempts = 0;
                    let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                    return LRESULT(0);
                }
            }

            // Present
            let flags = if r.supports_tearing { DXGI_PRESENT_ALLOW_TEARING } else { DXGI_PRESENT(0) };
            let hr = r.swap_chain.Present(0, flags);
            if is_device_lost(&hr) {
                s.device_recover_attempts = 1;
                let _ = EndPaint(h, &ps);
                recreate_renderer(s, h);
                if s.renderer.is_null() {
                    return LRESULT(0);
                }
                s.device_recover_attempts = 0;
                let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                return LRESULT(0);
            }

            let _ = EndPaint(h, &ps);
            return LRESULT(0);
        }

        WM_DESTROY => {
            tray::destroy();
            PostQuitMessage(0);
            return LRESULT(0);
        }

        WM_ACTIVATE => {
            if wp.0 == 0 {
                if !s.visible { return LRESULT(0); }
                if s.hide_on_focus_loss { hide_clear(h, s); }
                return LRESULT(0);
            }
            return DefWindowProcW(h, msg, wp, lp);
        }

        WM_IME_SETCONTEXT => {
            return DefWindowProcW(h, msg, wp, LPARAM(lp.0 & !(ISC_SHOWUICOMPOSITIONWINDOW as isize)));
        }

        WM_IME_STARTCOMPOSITION => {
            let himc = ImmGetContext(h);
            if himc != 0 {
                let prefix = &s.input_text[..s.cursor_pos];
                let prefix_display = prefix.replace("&", "&&");
                let text_w = measure_text_width(s, &prefix_display) as i32;
                let cf = COMPOSITIONFORM {
                    dwStyle: CFS_FORCE_POSITION,
                    ptCurrentPos: POINT { x: s.input_rect.left + 8 + text_w, y: s.input_rect.bottom + 4 },
                    rcArea: RECT::default(),
                };
                let _ = ImmSetCompositionWindow(himc, &cf);
                let _ = ImmReleaseContext(h, himc);
            }
            return LRESULT(0);
        }

        WM_IME_COMPOSITION => {
            let himc = ImmGetContext(h);
            if himc != 0 {
                let cf = COMPOSITIONFORM {
                    dwStyle: CFS_FORCE_POSITION,
                    ptCurrentPos: POINT { x: s.input_rect.left, y: s.input_rect.bottom },
                    rcArea: RECT::default(),
                };
                let _ = ImmSetCompositionWindow(himc, &cf);

                if lp.0 as usize & GCS_COMPSTR as usize != 0 {
                    let len = ImmGetCompositionStringW(himc, GCS_COMPSTR, ptr::null_mut(), 0);
                    if len > 0 {
                        let mut buf = vec![0u16; (len as usize) / 2 + 1];
                        let _ = ImmGetCompositionStringW(himc, GCS_COMPSTR, buf.as_mut_ptr() as *mut std::ffi::c_void, len);
                        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                        s.composing = String::from_utf16_lossy(&buf[..end]);
                    } else {
                        s.composing.clear();
                    }
                }
                if lp.0 as usize & GCS_RESULTSTR as usize != 0 {
                    let len = ImmGetCompositionStringW(himc, GCS_RESULTSTR, ptr::null_mut(), 0);
                    if len > 0 {
                        let mut buf = vec![0u16; (len as usize) / 2 + 1];
                        let _ = ImmGetCompositionStringW(himc, GCS_RESULTSTR, buf.as_mut_ptr() as *mut std::ffi::c_void, len);
                        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                        let result = String::from_utf16_lossy(&buf[..end]);
                        push_undo(s);
                        s.input_text.insert_str(s.cursor_pos, &result);
                        s.cursor_pos += result.len();
                        s.composing.clear();
                        fill_list(s, h);
                    }
                }
                let _ = ImmReleaseContext(h, himc);
                let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
            }
            return LRESULT(0);
        }

        WM_IME_ENDCOMPOSITION => {
            s.composing.clear();
            let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
            return LRESULT(0);
        }

        WM_HOTKEY => {
            let hotkey_id = wp.0 as i32;
            if plugin::is_plugin_hotkey(hotkey_id) {
                plugin::dispatch_hotkey(hotkey_id);
            } else {
                toggle_win(h, s);
            }
            return LRESULT(0);
        }

        TRAY_MSG => {
            match lp.0 as u32 & 0xFFFF {
                0x0205 => { tray::show_menu(h); }
                0x0201 => {
                    if let Some(t) = s.last_hide_time {
                        if t.elapsed() < std::time::Duration::from_millis(200) {
                            s.last_hide_time = None;
                            return LRESULT(0);
                        }
                    }
                    toggle_win(h, s);
                }
                _ => {}
            }
            return LRESULT(0);
        }

        WM_COMMAND => {
            let id = (wp.0 as u32 & 0xFFFF) as u16;
            match id {
                IDM_TOGGLE => { toggle_win(h, s); return LRESULT(0); }
                IDM_SETTINGS => {
                    if let Some(r) = gua_renderer(s) {
                        crate::settings::open_settings(h, r);
                    }
                    return LRESULT(0);
                }
                IDM_OPEN_CONFIG => {
                    let dir = config::config_dir();
                    let p = to_w(&dir.to_string_lossy());
                    let _ = ShellExecuteW(Some(h), w!("open"), pcwstr(&p), PCWSTR(ptr::null()), PCWSTR(ptr::null()), SW_SHOWNORMAL);
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
            let ctrl = unsafe { (GetKeyState(0x11) as i16) < 0 };
            match wp.0 as u32 {
                VK_ESCAPE => { hide_clear(h, s); return LRESULT(0); }
                VK_RETURN => {
                    execute_sel(h, s);
                    return LRESULT(0);
                }
                0x08 => {
                    s.sel_start = None;
                    if s.cursor_pos > 0 {
                        push_undo(s);
                        let prev = s.input_text.floor_char_boundary(s.cursor_pos - 1);
                        s.input_text.replace_range(prev..s.cursor_pos, "");
                        s.cursor_pos = prev;
                        fill_list(s, h);
                        let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                    }
                    return LRESULT(0);
                }
                0x25 => {
                    s.sel_start = None;
                    if s.cursor_pos > 0 {
                        s.cursor_pos = s.input_text.floor_char_boundary(s.cursor_pos - 1);
                    }
                    return LRESULT(0);
                }
                0x27 => {
                    s.sel_start = None;
                    if s.cursor_pos < s.input_text.len() {
                        s.cursor_pos = s.input_text.ceil_char_boundary(s.cursor_pos + 1);
                    }
                    return LRESULT(0);
                }
                0x2E => {
                    s.sel_start = None;
                    if s.cursor_pos < s.input_text.len() {
                        push_undo(s);
                        let next = s.input_text.ceil_char_boundary(s.cursor_pos + 1);
                        s.input_text.replace_range(s.cursor_pos..next, "");
                        fill_list(s, h);
                        let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                    }
                    return LRESULT(0);
                }
                0x26 | 0x28 => {
                    let n = s.filtered_indices.len();
                    if n == 0 { return LRESULT(0); }
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
                    let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                    return LRESULT(0);
                }
                0x21 | 0x22 | 0x23 | 0x24 => {
                    let n = s.filtered_indices.len();
                    if n == 0 { return LRESULT(0); }
                    match wp.0 as u32 {
                        0x24 => { s.sel_index = 0; s.scroll_offset = 0; }
                        0x23 => {
                            s.sel_index = n - 1;
                            if s.sel_index >= s.max_results {
                                s.scroll_offset = s.sel_index - s.max_results + 1;
                            }
                        }
                        0x21 => {
                            s.sel_index = s.sel_index.saturating_sub(s.max_results);
                            if s.sel_index < s.scroll_offset {
                                s.scroll_offset = s.sel_index;
                            }
                        }
                        _ => {
                            s.sel_index = (s.sel_index + s.max_results).min(n - 1);
                            let bottom = s.scroll_offset + s.max_results - 1;
                            if s.sel_index > bottom && s.scroll_offset + s.max_results < n {
                                s.scroll_offset = s.sel_index - s.max_results + 1;
                            }
                        }
                    }
                    let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                    return LRESULT(0);
                }
                _ => {
                    if ctrl {
                        match wp.0 as u32 {
                            0x41 => { // Ctrl+A
                                s.sel_start = Some(0);
                                s.sel_end = s.input_text.len();
                                s.cursor_pos = s.input_text.len();
                                let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                            }
                            0x43 => { // Ctrl+C
                                let _ = clipboard_copy(&s.input_text);
                            }
                            0x56 => { // Ctrl+V
                                if let Some(text) = clipboard_paste() {
                                    push_undo(s);
                                    s.input_text.insert_str(s.cursor_pos, &text);
                                    s.cursor_pos += text.len();
                                    fill_list(s, h);
                                    let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                                }
                            }
                            0x58 => { // Ctrl+X
                                push_undo(s);
                                let _ = clipboard_copy(&s.input_text);
                                s.input_text.clear();
                                s.cursor_pos = 0;
                                fill_list(s, h);
                                let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                            }
                            0x5A => { // Ctrl+Z
                                if let Some((prev_text, prev_cursor)) = s.input_undo.pop() {
                                    s.input_text = prev_text;
                                    s.cursor_pos = prev_cursor.min(s.input_text.len());
                                    s.sel_start = None;
                                    s.sel_end = 0;
                                    s.composing.clear();
                                    fill_list(s, h);
                                    let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                                }
                            }
                            _ => return DefWindowProcW(h, msg, wp, lp),
                        }
                        return LRESULT(0);
                    }
                    return DefWindowProcW(h, msg, wp, lp);
                }
            }
        }

        WM_CHAR => {
            s.sel_start = None;
            let ch = match char::from_u32(wp.0 as u32) {
                Some(c) if !c.is_control() => c,
                _ => { return LRESULT(0); }
            };
            push_undo(s);
            s.input_text.insert(s.cursor_pos, ch);
            s.cursor_pos += ch.len_utf8();
            fill_list(s, h);
            let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
            return LRESULT(0);
        }

        WM_SIZE => {
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            let w = rc.right - rc.left;
            let hh = rc.bottom - rc.top;
            let fp = font_px(s.font_size, s.dpi);
            s.eh = fp + 24;
            s.item_h = fp + 20;
            s.width = w;
            s.input_rect = RECT { left: PD, top: PD, right: w - PD, bottom: PD + s.eh };

            // Resize swap chain（先释放 D2D 对旧 back buffer 的引用，再 resize）
            if let Some(r) = gua_renderer_mut(s) {
                r.target = None;
                r.d2d_context.SetTarget(None as Option<&ID2D1Image>);
                let _ = r.swap_chain.ResizeBuffers(0, w.max(1) as u32, hh.max(1) as u32, DXGI_FORMAT_UNKNOWN, DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING);
                if let Ok(back) = r.swap_chain.GetBuffer::<IDXGISurface>(0) {
                    let props = D2D1_BITMAP_PROPERTIES1 {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                        },
                        dpiX: 96.0, dpiY: 96.0,
                        bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                        colorContext: std::mem::ManuallyDrop::new(None),
                    };
                    if let Ok(target) = r.d2d_context.CreateBitmapFromDxgiSurface(&back, Some(&props)) {
                        r.target = Some(target);
                        r.d2d_context.SetTarget(r.target.as_ref().unwrap());
                    }
                }
            }

            // Composition 交换链不需要 DComp clip——圆角由 D2D FillRoundedRectangle 抗锯齿绘制
            return LRESULT(0);
        }

        WM_DPICHANGED => {
            let dpi = hiword(wp.0 as u32) as i32;
            s.dpi = dpi;
            let rc = &*(lp.0 as *const RECT);
            let _ = SetWindowPos(h, Some(HWND_TOP), rc.left, rc.top,
                rc.right - rc.left, rc.bottom - rc.top,
                SWP_NOZORDER | SWP_NOACTIVATE);
            rebuild_font(s, dpi);
            return LRESULT(0);
        }

        WM_POWERBROADCAST => {
            let evt = wp.0 as u32;
            if evt == PBT_APMRESUMESUSPEND || evt == PBT_APMRESUMEAUTOMATIC {
                let hp = GetCurrentProcess();
                let prio = MemPrio { priority: MEM_PRIO_VERY_LOW };
                let _ = SetProcessInformation(hp, PROCESS_MEMORY_PRIORITY, &prio as *const _ as *const u8, mem::size_of::<MemPrio>() as u32);
            }
            return LRESULT(0);
        }

        _ => {
            if msg != WM_HOTKEY && msg != WM_DESTROY && msg != WM_COMMAND {
                if plugin::dispatch_wndproc(msg, wp.0 as u64, lp.0 as i64) {
                    return LRESULT(0);
                }
            }
        }
    }

    DefWindowProcW(h, msg, wp, lp)
}
