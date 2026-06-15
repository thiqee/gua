// Gua — 窗口过程（消息处理）

use std::ptr;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::draw::*;
use crate::state::*;
use crate::tray;
use crate::window::*;

#[link(name = "imm32")]
extern "system" {
    fn ImmGetContext(hwnd: HWND) -> isize;
    fn ImmSetCompositionWindow(himc: isize, lpCompForm: *const COMPOSITIONFORM) -> BOOL;
    fn ImmGetCompositionStringW(himc: isize, dwIndex: u32, lpBuf: *mut std::ffi::c_void, dwBufLen: u32) -> u32;
    fn ImmReleaseContext(hwnd: HWND, himc: isize) -> BOOL;
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
            let mut ps = PAINTSTRUCT::default();
            BeginPaint(h, &mut ps);
            let hdc = ps.hdc;

            let total = s.filtered_indices.len();
            let vis = total.min(s.max_results);
            let lh = s.item_h;
            let ly = list_y(&s.input_rect);
            let sh = status_bar_h(s.dpi, s.status_font_size);
            let win_height = win_h(total, lh, s.eh, s.max_results, sh);

            let mem_dc = CreateCompatibleDC(Some(hdc));
            let bmp = CreateCompatibleBitmap(hdc, s.width, win_height);
            let old_bmp = SelectObject(mem_dc, HGDIOBJ(bmp.0));

            let bg_brush = CreateSolidBrush(colorref(s.theme_color));
            let full_rect = RECT { left: 0, top: 0, right: s.width, bottom: win_height };
            FillRect(mem_dc, &full_rect, bg_brush);
            let _ = DeleteObject(HGDIOBJ(bg_brush.0));

            let argb = 0xFF000000 | s.input_bg_color;
            let ir = &s.input_rect;
            let ic = (s.round_corner * 3 / 4).max(1);
            fill_round_rect(mem_dc, ir.left, ir.top, ir.right - ir.left, ir.bottom - ir.top, ic, argb);

            if let Some(ref f) = s.hfont {
                SelectObject(mem_dc, HGDIOBJ(f.0));
            }
            SetBkMode(mem_dc, OPAQUE);
            SetBkColor(mem_dc, colorref(s.input_bg_color));
            SetTextColor(mem_dc, colorref(s.text_color));
            let mut r = s.input_rect;
            r.left += 8;
            r.right -= 4;
            let display = s.input_text.replace("&", "&&");
            let mut ws: Vec<u16> = display.encode_utf16().collect();
            ws.push(0);
            if !ws.is_empty() && r.right > r.left && r.bottom > r.top {
                DrawTextW(mem_dc, &mut ws, &mut r, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
            }

            if !s.composing.is_empty() {
                let mut sz = SIZE::default();
                if !s.input_text.is_empty() {
                    let pws: Vec<u16> = s.input_text.encode_utf16().collect();
                    GetTextExtentPoint32W(mem_dc, &pws, &mut sz);
                }
                let cx = s.input_rect.left + 8 + sz.cx;
                SetTextColor(mem_dc, colorref(s.text_color & 0xC0C0C0 | 0x404040));
                let comp_display = s.composing.replace("&", "&&");
                let mut cws: Vec<u16> = comp_display.encode_utf16().collect();
                cws.push(0);
                let mut cr = RECT {
                    left: cx,
                    top: s.input_rect.top,
                    right: s.input_rect.right,
                    bottom: s.input_rect.bottom,
                };
                DrawTextW(mem_dc, &mut cws, &mut cr, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
                SetTextColor(mem_dc, colorref(s.text_color));
            }

            let start = s.scroll_offset.min(total.saturating_sub(vis));
            for i in 0..vis {
                let fi = start + i;
                if fi >= total { break; }
                let y = ly + i as i32 * lh;
                let rc = RECT { left: PD, top: y, right: s.width - PD, bottom: y + lh };
                draw_filtered_item(mem_dc, s, fi, &rc);
            }

            if vis > 0 {
                redraw_status_bar(mem_dc, s, ly, vis);
            }

            BitBlt(hdc, 0, 0, s.width, win_height, Some(mem_dc), 0, 0, SRCCOPY);

            SelectObject(mem_dc, old_bmp);
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            let _ = DeleteDC(mem_dc);

            EndPaint(h, &ps);
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
            HideCaret(Some(h));
            let himc = ImmGetContext(h);
            if himc != 0 {
                let cf = COMPOSITIONFORM {
                    dwStyle: CFS_FORCE_POSITION,
                    ptCurrentPos: POINT { x: s.input_rect.left, y: s.input_rect.bottom },
                    rcArea: RECT::default(),
                };
                ImmSetCompositionWindow(himc, &cf);
                ImmReleaseContext(h, himc);
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
                ImmSetCompositionWindow(himc, &cf);

                if lp.0 as usize & GCS_COMPSTR as usize != 0 {
                    let len = ImmGetCompositionStringW(himc, GCS_COMPSTR, ptr::null_mut(), 0);
                    if len > 0 {
                        let bytes = len as usize;
                        let mut buf = vec![0u16; bytes / 2 + 1];
                        ImmGetCompositionStringW(himc, GCS_COMPSTR, buf.as_mut_ptr() as *mut std::ffi::c_void, len);
                        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                        s.composing = String::from_utf16_lossy(&buf[..end]);
                    } else {
                        s.composing.clear();
                    }
                }
                if lp.0 as usize & GCS_RESULTSTR as usize != 0 {
                    let len = ImmGetCompositionStringW(himc, GCS_RESULTSTR, ptr::null_mut(), 0);
                    if len > 0 {
                        let bytes = len as usize;
                        let mut buf = vec![0u16; bytes / 2 + 1];
                        ImmGetCompositionStringW(himc, GCS_RESULTSTR, buf.as_mut_ptr() as *mut std::ffi::c_void, len);
                        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                        let result = String::from_utf16_lossy(&buf[..end]);
                        s.input_text.insert_str(s.cursor_pos, &result);
                        s.cursor_pos += result.len();
                        s.filter = s.input_text.clone();
                        s.composing.clear();
                        fill_list(s, h);
                        update_caret(s, h);
                    }
                }
                ImmReleaseContext(h, himc);
                RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
            }
            return LRESULT(0);
        }

        WM_IME_ENDCOMPOSITION => {
            s.composing.clear();
            ShowCaret(Some(h));
            RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
            return LRESULT(0);
        }

        WM_HOTKEY => {
            toggle_win(h, s);
            return LRESULT(0);
        }

        TRAY_MSG => {
            match lp.0 as u32 & 0xFFFF {
                0x0205 => {
                    tray::show_menu(h);
                }
                0x0201 => {
                    // 如果面板刚因失焦隐藏（<200ms），本次托盘点击不重新打开
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
                IDM_OPEN_CONFIG => {
                    let p = to_w(CONFIG_FILE);
                    ShellExecuteW(Some(h), w!("open"), pcwstr(&p), PCWSTR(ptr::null()), PCWSTR(ptr::null()), SW_SHOWNORMAL);
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
            match wp.0 as u32 {
                VK_ESCAPE => { hide_clear(h, s); return LRESULT(0); }
                VK_RETURN => {
                    execute_sel(h, s);
                    return LRESULT(0);
                }
                0x08 => {
                    if s.cursor_pos > 0 {
                        let prev = s.input_text.floor_char_boundary(s.cursor_pos - 1);
                        s.input_text.replace_range(prev..s.cursor_pos, "");
                        s.cursor_pos = prev;
                        s.filter = s.input_text.clone();
                        fill_list(s, h);
                        update_caret(s, h);
                        RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                    }
                    return LRESULT(0);
                }
                0x25 => {
                    if s.cursor_pos > 0 {
                        s.cursor_pos = s.input_text.floor_char_boundary(s.cursor_pos - 1);
                        update_caret(s, h);
                    }
                    return LRESULT(0);
                }
                0x27 => {
                    if s.cursor_pos < s.input_text.len() {
                        s.cursor_pos = s.input_text.ceil_char_boundary(s.cursor_pos + 1);
                        update_caret(s, h);
                    }
                    return LRESULT(0);
                }
                0x2E => {
                    if s.cursor_pos < s.input_text.len() {
                        let next = s.input_text.ceil_char_boundary(s.cursor_pos + 1);
                        s.input_text.replace_range(s.cursor_pos..next, "");
                        s.filter = s.input_text.clone();
                        fill_list(s, h);
                        RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                    }
                    return LRESULT(0);
                }
                0x26 | 0x28 => {
                    let n = s.filtered_indices.len();
                    if n == 0 { return LRESULT(0); }
                    let old_sel = s.sel_index;
                    let old_offset = s.scroll_offset;
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
                    if old_sel != s.sel_index {
                        if s.scroll_offset != old_offset {
                            RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
                            return LRESULT(0);
                        }
                        let ly = list_y(&s.input_rect);
                        let lh = s.item_h;
                        let vis = n.min(s.max_results);
                        let dc = GetDC(Some(h));
                        let old_vis = old_sel as i32 - s.scroll_offset as i32;
                        if old_vis >= 0 && old_vis < vis as i32 {
                            let y = ly + old_vis * lh;
                            let rc = RECT { left: PD, top: y, right: s.width - PD, bottom: y + lh };
                            draw_filtered_item(dc, s, old_sel, &rc);
                        }
                        let new_vis = s.sel_index as i32 - s.scroll_offset as i32;
                        if new_vis >= 0 && new_vis < vis as i32 {
                            let y = ly + new_vis * lh;
                            let rc = RECT { left: PD, top: y, right: s.width - PD, bottom: y + lh };
                            draw_item_hl_text(dc, s, s.sel_index, &rc, true);
                        }
                        redraw_status_bar(dc, s, ly, vis);
                        let _ = ReleaseDC(Some(h), dc);
                    }
                    return LRESULT(0);
                }
                _ => {
                    return DefWindowProcW(h, msg, wp, lp);
                }
            }
        }

        WM_CHAR => {
            let ch = match char::from_u32(wp.0 as u32) {
                Some(c) if !c.is_control() => c,
                _ => { return LRESULT(0); }
            };
            s.input_text.insert(s.cursor_pos, ch);
            s.cursor_pos += ch.len_utf8();
            s.filter = s.input_text.clone();
            fill_list(s, h);
            update_caret(s, h);
            RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
            return LRESULT(0);
        }

        WM_SIZE => {
            let mut rc = RECT::default();
            GetClientRect(h, &mut rc);
            let w = rc.right - rc.left;
            let fp = font_px(s.font_size, s.dpi);
            let eh = fp + 24;
            s.eh = eh;
            s.item_h = fp + 20;
            s.width = w;
            s.input_rect = RECT { left: PD, top: PD, right: w - PD, bottom: PD + eh };
            return LRESULT(0);
        }

        WM_DPICHANGED => {
            let dpi = hiword(wp.0 as u32) as i32;
            s.dpi = dpi;
            rebuild_font(s, dpi);
            let rc = &*(lp.0 as *const RECT);
            SetWindowPos(h, Some(HWND_TOP), rc.left, rc.top,
                rc.right - rc.left, rc.bottom - rc.top,
                SWP_NOZORDER | SWP_NOACTIVATE);
            return LRESULT(0);
        }

        _ => {}
    }

    DefWindowProcW(h, msg, wp, lp)
}
