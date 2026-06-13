// KeyHop — 窗口管理

use std::ptr;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config;
use crate::executor;
use crate::state::*;

pub unsafe fn hide_clear(h: HWND, s: &mut AppState) {
    s.visible = false;
    s.filter.clear();
    s.input_text.clear();
    s.cursor_pos = 0;
    s.filtered_indices.clear();
    s.sel_index = 0;
    s.scroll_offset = 0;
    s.search_query.clear();
    DestroyCaret();
    ShowWindow(h, SW_HIDE);
}

pub unsafe fn fill_list(s: &mut AppState, h: HWND) {
    // 按第一个空格拆成 key 和搜索词
    let (key_part, query_part) = if let Some(pos) = s.filter.find(' ') {
        let (k, _) = s.filter.split_at(pos);
        (k.to_string(), s.filter[pos + 1..].to_string())
    } else {
        (s.filter.clone(), String::new())
    };
    s.search_query = query_part;

    s.filtered_indices.clear();

    if key_part.is_empty() {
        let sh = status_bar_h(s.dpi, s.status_font_size);
        let nh = win_h(0, s.item_h, s.eh, s.max_results, sh);
        let mut rc = RECT::default();
        GetWindowRect(h, &mut rc);
        let cur_h = rc.bottom - rc.top;
        if cur_h != nh {
            SetWindowPos(h, Some(HWND_TOP), rc.left, rc.top, s.width, nh, SWP_NOZORDER);
            round_win(h, s.width, nh, s.round_corner);
        }
        return;
    }

    // 匹配并收集索引
    for (i, e) in s.entries.iter().enumerate() {
        let matched = if s.case_sensitive {
            e.key.starts_with(&key_part)
        } else {
            e.key.to_lowercase().starts_with(&key_part.to_lowercase())
        };
        if matched {
            s.filtered_indices.push(i);
        }
    }

    let n = s.filtered_indices.len();
    let sh = status_bar_h(s.dpi, s.status_font_size);
    let nh = win_h(n, s.item_h, s.eh, s.max_results, sh);
    let mut rc = RECT::default();
    GetWindowRect(h, &mut rc);
    let cur_h = rc.bottom - rc.top;
    if cur_h != nh {
        SetWindowPos(h, Some(HWND_TOP), rc.left, rc.top, s.width, nh, SWP_NOZORDER);
        round_win(h, s.width, nh, s.round_corner);
    }

    s.sel_index = 0;
    s.scroll_offset = 0;
}

pub unsafe fn execute_sel(h: HWND, s: &mut AppState) {
    if s.sel_index < s.filtered_indices.len() {
        let idx = s.filtered_indices[s.sel_index];
        if idx < s.entries.len() {
            executor::execute(&s.entries[idx].key, &s.entries[idx].value, &s.search_query);
            hide_clear(h, s);
        }
    }
}

pub unsafe fn rebuild_font(s: &mut AppState, dpi: i32) {
    if let Some(old) = s.hfont.take() {
        let _ = DeleteObject(HGDIOBJ(old.0));
    }
    let fp = font_px(s.font_size, dpi);
    s.eh = fp + 24;
    s.item_h = fp + 20;
    if let Ok(f) = make_font_with(dpi, &s.font_name, s.font_size) {
        s.hfont = Some(f);
    }
}

pub unsafe fn toggle_win(h: HWND, s: &mut AppState) {
    // 黑名单检查：当前台窗口在黑名单中时，热键不响应
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
        // 按需重载配置
        let cur = std::fs::metadata(CONFIG_FILE)
            .ok()
            .and_then(|m| m.modified().ok());
        let mut font_name = s.font_name.clone();
        let mut font_size = s.font_size;
        let config_changed = s.config_mtime != cur;
        if config_changed {
            let raw = config::load(CONFIG_FILE);
            font_name = cfg_str(&raw, "_font", &s.font_name);
            font_size = cfg_f32(&raw, "_font_size", s.font_size);
            s.max_results = cfg_usize(&raw, "_max_results", s.max_results);
            s.width = cfg_i32(&raw, "_width", s.width);
            s.round_corner = cfg_i32(&raw, "_round_corner", s.round_corner);
            s.hide_on_focus_loss = cfg_bool(&raw, "_hide_on_focus_loss", s.hide_on_focus_loss);
            s.theme_color = cfg_color(&raw, "_theme_color", s.theme_color);
            s.input_bg_color = cfg_color(&raw, "_input_bg_color", s.input_bg_color);
            s.accent_color = cfg_color(&raw, "_accent_color", s.accent_color);
            s.text_color = cfg_color(&raw, "_text_color", s.text_color);
            s.status_font_size = cfg_f32(&raw, "_status_font_size", s.status_font_size);
            if let Some(old) = s.status_hfont.take() {
                let _ = DeleteObject(HGDIOBJ(old.0));
            }
            s.always_on_top = cfg_bool(&raw, "_always_on_top", s.always_on_top);
            s.opacity = cfg_usize(&raw, "_opacity", s.opacity as usize).min(255) as u8;
            s.case_sensitive = cfg_bool(&raw, "_case_sensitive", s.case_sensitive);
            // 面板位置：0~100 转为 0.0~1.0 比例
            s.panel_ratio_x = cfg_f32(&raw, "_panel_position_x", 50.0).clamp(0.0, 100.0) / 100.0;
            s.panel_ratio_y = cfg_f32(&raw, "_panel_position_y", 50.0).clamp(0.0, 100.0) / 100.0;
            // 热键变更时重新注册（在 into_iter 之前，raw 尚未被消费）
            let new_hotkey_str = cfg_str(&raw, "_hotkey", "Alt+Space");
            if let Some((new_mod, new_vk)) = parse_hotkey(&new_hotkey_str) {
                if new_mod != s.mod_keys || new_vk != s.hotkey_vk {
                    // 先注销旧的，再注册新的（同一 ID 不能重复注册）
                    UnregisterHotKey(h, HOTKEY_ID);
                    if RegisterHotKey(h, HOTKEY_ID, new_mod, new_vk).as_bool() {
                        s.mod_keys = new_mod;
                        s.hotkey_vk = new_vk;
                    } else {
                        eprintln!("config: 新热键 \"{new_hotkey_str}\" 注册失败，恢复原热键");
                        RegisterHotKey(h, HOTKEY_ID, s.mod_keys, s.hotkey_vk);
                    }
                }
            } else {
                eprintln!("config: 新热键 \"{new_hotkey_str}\" 无法识别，保持原热键");
            }
            // 重载黑名单
            s.blacklist = cfg_blacklist(&raw, "_blacklist");
            let new_entries: Vec<_> = raw.into_iter().filter(|e| !e.key.starts_with('_')).collect();
            if !new_entries.is_empty() {
                s.entries = new_entries;
            }
            s.config_mtime = cur;
            // 应用窗口样式变更
            if s.opacity < 255 {
                let style = GetWindowLongPtrW(h, GWL_EXSTYLE);
                if style & WS_EX_LAYERED.0 as isize == 0 {
                    SetWindowLongPtrW(h, GWL_EXSTYLE, style | WS_EX_LAYERED.0 as isize);
                }
                SetLayeredWindowAttributes(h, COLORREF(0), s.opacity, LWA_ALPHA);
            } else {
                let style = GetWindowLongPtrW(h, GWL_EXSTYLE);
                if style & WS_EX_LAYERED.0 as isize != 0 {
                    SetWindowLongPtrW(h, GWL_EXSTYLE, style & !(WS_EX_LAYERED.0 as isize));
                }
            }
            let after = if s.always_on_top { Some(HWND_TOPMOST) } else { Some(HWND_NOTOPMOST) };
            SetWindowPos(h, after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        // 字体变化时重建
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
        // 首次启动或配置变更后重新定位，否则复用上次位置
        if config_changed {
            let sh = status_bar_h(s.dpi, s.status_font_size);
            center_win(h, s.width, win_h(0, s.item_h, s.eh, s.max_results, sh), s.panel_ratio_x, s.panel_ratio_y);
        }
        ShowWindow(h, SW_SHOW);
        SetForegroundWindow(h);
        SetFocus(h);
        create_input_caret(h, s);
    }
}

pub unsafe fn create_input_caret(h: HWND, s: &AppState) {
    if let Some(ref f) = s.hfont {
        let dc = GetDC(Some(h));
        let old = SelectObject(dc, HGDIOBJ(f.0));
        let mut tm = TEXTMETRICW::default();
        GetTextMetricsW(dc, &mut tm);
        SelectObject(dc, old);
        let _ = ReleaseDC(Some(h), dc);
        let caret_h = tm.tmHeight;
        CreateCaret(h, Some(HBITMAP(ptr::null_mut())), 2, caret_h as i32);
        SetCaretPos(s.input_rect.left + 8, s.input_rect.top + ((s.input_rect.bottom - s.input_rect.top) - caret_h) / 2);
        ShowCaret(Some(h));
    }
}

pub unsafe fn update_caret(s: &AppState, h: HWND) {
    if !s.visible { return; }
    if let Some(ref f) = s.hfont {
        let dc = GetDC(Some(h));
        let old = SelectObject(dc, HGDIOBJ(f.0));
        let mut tm = TEXTMETRICW::default();
        GetTextMetricsW(dc, &mut tm);
        let caret_h = tm.tmHeight;
        let prefix = &s.input_text[..s.cursor_pos];
        let ws: Vec<u16> = prefix.encode_utf16().collect();
        let mut sz = SIZE::default();
        GetTextExtentPoint32W(dc, &ws, &mut sz);
        let cx = s.input_rect.left + 8 + sz.cx + 1;
        let cy = s.input_rect.top + ((s.input_rect.bottom - s.input_rect.top) - caret_h) / 2;
        SelectObject(dc, old);
        let _ = ReleaseDC(Some(h), dc);
        SetCaretPos(cx, cy);
    }
}
