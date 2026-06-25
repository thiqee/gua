// Gua — 窗口管理

use std::mem;
use std::ptr;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::UI::WindowsAndMessaging::*;

#[link(name = "kernel32")]
extern "system" {
    fn SetProcessWorkingSetSize(h: HANDLE, min: usize, max: usize) -> i32;
    fn SetProcessInformation(h: HANDLE, class: i32, info: *const u8, size: u32) -> i32;
}

use crate::config;
use crate::executor;
use crate::plugin;
use crate::state::*;

/// 隐藏窗口并清空状态
///
/// # Safety
/// - `h` 必须是有效的窗口句柄
/// - `s` 必须是可变的 AppState 引用
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
    let _ = DestroyCaret();
    let _ = ShowWindow(h, SW_HIDE);
    // 降优先级后立即换出，保持低优先级让系统持续修剪
    let hp = GetCurrentProcess();
    let prio = MemPrio { priority: MEM_PRIO_VERY_LOW };
    let _ = SetProcessInformation(hp, PROCESS_MEMORY_PRIORITY, &prio as *const _ as *const u8, mem::size_of::<MemPrio>() as u32);
    let _ = SetProcessWorkingSetSize(hp, usize::MAX, usize::MAX);
}

/// 填充筛选列表并调整窗口高度
///
/// # Safety
/// - `h` 必须是有效的窗口句柄
/// - `s` 必须是可变的 AppState 引用，且 `s.entries` 等字段须正确初始化
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
        round_win(h, s.width, nh, s.round_corner);
    }

    s.sel_index = 0;
    s.scroll_offset = 0;
}

/// 执行当前选中项
///
/// # Safety
/// - `h` 必须是有效的窗口句柄
/// - `s` 必须是可变的 AppState 引用
pub unsafe fn execute_sel(h: HWND, s: &mut AppState) {
    // 有搜索词时（输入包含空格），用精确匹配的识别码执行搜索，忽略列表选中项
    let idx = if !s.search_query.is_empty() {
        let key_part = s.filter.split(' ').next().unwrap_or("");
        // 同名识别码中优先选搜索引擎 URL（http/https 开头且以 = 结尾的 URL 模板）
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
    // 没有搜索词时走正常的选中项
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

/// 重建字体对象
///
/// # Safety
/// - `s` 必须是可变的 AppState 引用
/// - 旧字体对象会被释放，调用后旧指针不应继续使用
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

/// 重载配置文件，返回 (配置是否变更, 更新后的字体名, 更新后的字号)
/// 热重载配置（检查 mtime）
///
/// # Safety
/// - `h` 必须是有效的窗口句柄
/// - `s` 必须是可变的 AppState 引用
pub unsafe fn reload_config(h: HWND, s: &mut AppState) -> (bool, String, f32) {
    let cur = std::fs::metadata(CONFIG_FILE)
        .ok()
        .and_then(|m| m.modified().ok());
    let mut font_name = s.font_name.clone();
    let mut font_size = s.font_size;
    let config_changed = s.config_mtime != cur;
    if config_changed {
        let raw = config::load(CONFIG_FILE);
        let has_explicit_font = raw.iter().any(|e| e.key == "_font");
        // 热重载时重新注册 fonts/ 里的字体
        let private_font_name = crate::state::load_private_fonts();
        font_name = if has_explicit_font {
            cfg_str(&raw, "_font", &s.font_name)
        } else {
            private_font_name.unwrap_or_else(|| cfg_str(&raw, "_font", &s.font_name))
        };
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
        s.fuzzy_enabled = cfg_bool(&raw, "_fuzzy_match", s.fuzzy_enabled);
        s.pinyin_enabled = cfg_bool(&raw, "_pinyin_search", s.pinyin_enabled);
        // 面板位置：0~100 转为 0.0~1.0 比例
        s.panel_ratio_x = cfg_f32(&raw, "_panel_position_x", 50.0).clamp(0.0, 100.0) / 100.0;
        s.panel_ratio_y = cfg_f32(&raw, "_panel_position_y", 50.0).clamp(0.0, 100.0) / 100.0;
        // 热键变更时重新注册（在 into_iter 之前，raw 尚未被消费）
        let new_hotkey_str = cfg_str(&raw, "_hotkey", "Alt+Space");
        if let Some((new_mod, new_vk)) = parse_hotkey(&new_hotkey_str) {
            if new_mod != s.mod_keys || new_vk != s.hotkey_vk {
                // 先注销旧的，再注册新的（同一 ID 不能重复注册）
                let _ = UnregisterHotKey(h, HOTKEY_ID);
                if RegisterHotKey(h, HOTKEY_ID, new_mod, new_vk).as_bool() {
                    s.mod_keys = new_mod;
                    s.hotkey_vk = new_vk;
                } else {
                    eprintln!("config: 新热键 \"{new_hotkey_str}\" 注册失败，恢复原热键");
                    let _ = std::fs::write("panic.log", format!("config: 新热键 \"{new_hotkey_str}\" 注册失败，恢复原热键\n"));
                    let _ = RegisterHotKey(h, HOTKEY_ID, s.mod_keys, s.hotkey_vk);
                }
            }
        } else {
            eprintln!("config: 新热键 \"{new_hotkey_str}\" 无法识别，保持原热键");
            let _ = std::fs::write("panic.log", format!("config: 新热键 \"{new_hotkey_str}\" 无法识别，保持原热键\n"));
        }
        // 重载黑名单
        s.blacklist = cfg_blacklist(&raw, "_blacklist");
        // 重载多音字覆写表
        s.pinyin_overrides = cfg_pinyin_overrides(&raw, "_pinyin_overrides");
        let plugin_configs = config::build_plugin_configs(&raw);
        let new_entries: Vec<_> = raw.into_iter().filter(|e| !e.key.starts_with('_')).collect();
        s.entries = new_entries;
        s.config_mtime = cur;
        plugin::notify_reload(&plugin_configs);
        // 应用窗口样式变更
        if s.opacity < 255 {
            let style = GetWindowLongPtrW(h, GWL_EXSTYLE);
            if style & WS_EX_LAYERED.0 as isize == 0 {
                let _ = SetWindowLongPtrW(h, GWL_EXSTYLE, style | WS_EX_LAYERED.0 as isize);
            }
            let _ = SetLayeredWindowAttributes(h, COLORREF(0), s.opacity, LWA_ALPHA);
        } else {
            let style = GetWindowLongPtrW(h, GWL_EXSTYLE);
            if style & WS_EX_LAYERED.0 as isize != 0 {
                let _ = SetWindowLongPtrW(h, GWL_EXSTYLE, style & !(WS_EX_LAYERED.0 as isize));
            }
        }
        let after = if s.always_on_top { Some(HWND_TOPMOST) } else { Some(HWND_NOTOPMOST) };
        let _ = SetWindowPos(h, after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
    (config_changed, font_name, font_size)
}

/// 切换窗口可见状态（显示/隐藏）
///
/// # Safety
/// - `h` 必须是有效的窗口句柄
/// - `s` 必须是可变的 AppState 引用
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
        // 恢复 Normal 优先级（为下一次隐藏时的标记做准备）
        let hp = GetCurrentProcess();
        let prio = MemPrio { priority: MEM_PRIO_NORMAL };
        let _ = SetProcessInformation(hp, PROCESS_MEMORY_PRIORITY, &prio as *const _ as *const u8, mem::size_of::<MemPrio>() as u32);
        let (config_changed, font_name, font_size) = reload_config(h, s);

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
        if config_changed || s.config_mtime.is_none() {
            let sh = status_bar_h(s.dpi, s.status_font_size);
            center_win(h, s.width, win_h(0, s.item_h, s.eh, s.max_results, sh), s.panel_ratio_x, s.panel_ratio_y);
        }
        let _ = ShowWindow(h, SW_SHOW);
        let _ = SetForegroundWindow(h);
        // UIPI 降权：如果 SetForegroundWindow 失败（全屏游戏等），
        // 尝试 SetWindowPos + HWND_TOP 作为替代方案
        if GetForegroundWindow() != h {
            let _ = SetWindowPos(h, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
            let _ = SetForegroundWindow(h);
        }
        let _ = SetFocus(h);
        create_input_caret(h, s);
    }
}

/// 创建输入光标
///
/// # Safety
/// - `h` 必须是有效的窗口句柄
/// - `s` 必须是有效的 AppState 引用
pub unsafe fn create_input_caret(h: HWND, s: &AppState) {
    if let Some(ref f) = s.hfont {
        let dc = GetDC(Some(h));
        let old = SelectObject(dc, HGDIOBJ(f.0));
        let mut tm = TEXTMETRICW::default();
        let _ = GetTextMetricsW(dc, &mut tm);
        let _ = SelectObject(dc, old);
        let _ = ReleaseDC(Some(h), dc);
        let caret_h = tm.tmHeight;
        let _ = CreateCaret(h, Some(HBITMAP(ptr::null_mut())), 2, caret_h as i32);
        let _ = SetCaretPos(s.input_rect.left + 8, s.input_rect.top + ((s.input_rect.bottom - s.input_rect.top) - caret_h) / 2);
        let _ = ShowCaret(Some(h));
    }
}

/// 更新光标位置至当前输入位置
///
/// # Safety
/// - `s` 必须是有效的 AppState 引用
/// - `h` 必须是有效的窗口句柄，且输入光标须已通过 `create_input_caret` 创建
pub unsafe fn update_caret(s: &AppState, h: HWND) {
    if !s.visible { return; }
    if let Some(ref f) = s.hfont {
        let dc = GetDC(Some(h));
        let old = SelectObject(dc, HGDIOBJ(f.0));
        let mut tm = TEXTMETRICW::default();
        let _ = GetTextMetricsW(dc, &mut tm);
        let caret_h = tm.tmHeight;
        let prefix = &s.input_text[..s.cursor_pos];
        let display = prefix.replace("&", "&&");
        let ws: Vec<u16> = display.encode_utf16().collect();
        let mut sz = SIZE::default();
        let _ = GetTextExtentPoint32W(dc, &ws, &mut sz);
        let cx = s.input_rect.left + 8 + sz.cx + 1;
        let cy = s.input_rect.top + ((s.input_rect.bottom - s.input_rect.top) - caret_h) / 2;
        let _ = SelectObject(dc, old);
        let _ = ReleaseDC(Some(h), dc);
        let _ = SetCaretPos(cx, cy);
    }
}
