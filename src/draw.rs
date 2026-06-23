// Gua — 自绘函数

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::GdiPlus::{
    GdipCreateFromHDC, GdipCreateSolidFill,
    GdipCreatePath, GdipAddPathArc,
    GdipClosePathFigure, GdipFillPath, GdipDeletePath,
    GdipDeleteBrush, GdipDeleteGraphics,
    GdipSetSmoothingMode,
    GpGraphics, GpSolidFill, GpBrush, GpPath, FillModeAlternate,
    SmoothingMode,
};
use crate::state::*;

// ── GDI+ 辅助 ──

/// 用 GDI+ 绘制圆角矩形填充区域
///
/// # Safety
/// - `hdc` 必须是有效的 HDC
/// - GDI+ 必须在调用前已初始化
pub unsafe fn fill_round_rect(hdc: HDC, x: i32, y: i32, w: i32, h: i32, r: i32, argb: u32) {
    let mut g: *mut GpGraphics = std::ptr::null_mut();
    if GdipCreateFromHDC(hdc, &mut g).0 != 0 || g.is_null() { return; }
    let mut b: *mut GpSolidFill = std::ptr::null_mut();
    if GdipCreateSolidFill(argb, &mut b).0 != 0 || b.is_null() {
        GdipDeleteGraphics(g);
        return;
    }
    let mut path: *mut GpPath = std::ptr::null_mut();
    if GdipCreatePath(FillModeAlternate, &mut path).0 != 0 || path.is_null() {
        GdipDeleteBrush(b as *mut GpBrush);
        GdipDeleteGraphics(g);
        return;
    }
    let fx = x as f32; let fy = y as f32;
    let fw = w as f32; let fh = h as f32; let fr = r as f32;
    GdipSetSmoothingMode(g, SmoothingMode(4));
    GdipAddPathArc(path, fx, fy, fr * 2.0, fr * 2.0, 180.0, 90.0);
    GdipAddPathArc(path, fx + fw - fr * 2.0, fy, fr * 2.0, fr * 2.0, 270.0, 90.0);
    GdipAddPathArc(path, fx + fw - fr * 2.0, fy + fh - fr * 2.0, fr * 2.0, fr * 2.0, 0.0, 90.0);
    GdipAddPathArc(path, fx, fy + fh - fr * 2.0, fr * 2.0, fr * 2.0, 90.0, 90.0);
    GdipClosePathFigure(path);
    GdipFillPath(g, b as *mut GpBrush, path);
    GdipDeletePath(path);
    GdipDeleteBrush(b as *mut GpBrush);
    GdipDeleteGraphics(g);
}

/// 提取公共文字渲染：格式化条目文本 → 设置字体/颜色 → DrawTextW
unsafe fn draw_entry_text(dc: HDC, s: &AppState, list_index: usize, rect: &RECT, selected: bool) {
    let old_font = s.hfont.as_ref().map(|f| SelectObject(dc, HGDIOBJ(f.0)));
    if let Some(&idx) = s.filtered_indices.get(list_index) {
        if idx < s.entries.len() {
            let e = &s.entries[idx];
            let tag = e.category.as_deref().unwrap_or_else(|| entry_type(&e.value));
            let display = e.description.as_deref().unwrap_or(&e.value);
            let txt = format!("[{}]  {}  →  {}", tag, e.key, display);
            let mut ws: Vec<u16> = txt.encode_utf16().collect();
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, if selected { COLORREF(0xFFFFFF) } else { colorref(s.text_color) });
            let mut r = RECT {
                left: rect.left + 8, top: rect.top + 6,
                right: rect.right - 4, bottom: rect.bottom - 6,
            };
            DrawTextW(dc, &mut ws, &mut r, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);
        }
    }
    if let Some(old) = old_font {
        SelectObject(dc, old);
    }
}

/// 在指定 DC 上画单项的高亮圆角背景 + 文字（不清背景，用于 VK_UP/DOWN 直接绘制）
///
/// # Safety
/// - `dc` 必须是有效的 HDC
/// - `s` 必须是有效的 AppState 引用，且 `s.filtered_indices` 中的索引须在 `s.entries` 范围内
pub unsafe fn draw_item_hl_text(dc: HDC, s: &AppState, list_index: usize, rect: &RECT, selected: bool) {
    // 高亮圆角
    let rcr = (s.round_corner / 2).max(1);
    let color = if selected { s.accent_color } else { s.theme_color };
    let argb = 0xFF000000 | color;
    fill_round_rect(dc, rect.left + 2, rect.top + 2,
        rect.right - rect.left - 4, rect.bottom - rect.top - 4,
        rcr, argb);

    // 文字
    draw_entry_text(dc, s, list_index, rect, selected);
}

/// 在指定 DC 上完整绘制一个筛选列表项（带背景填充，用于 WM_PAINT 全量重绘）
///
/// # Safety
/// - `hdc` 必须是有效的 HDC
/// - `s` 必须是有效的 AppState 引用, 且索引须在范围内
pub unsafe fn draw_filtered_item(hdc: HDC, s: &AppState, list_index: usize, rect: &RECT) {
    let is_sel = list_index == s.sel_index;

    // 背景
    let bg_brush = CreateSolidBrush(colorref(s.theme_color));
    FillRect(hdc, rect, bg_brush);
    let _ = DeleteObject(HGDIOBJ(bg_brush.0));

    // 选中高亮
    if is_sel {
        let rcr = (s.round_corner / 2).max(1);
        let argb = 0xFF000000 | s.accent_color;
        fill_round_rect(hdc, rect.left + 2, rect.top + 2,
            rect.right - rect.left - 4, rect.bottom - rect.top - 4,
            rcr, argb);
    }

    // 文字
    draw_entry_text(hdc, s, list_index, rect, is_sel);
}

// ── 状态栏重绘（VK_UP/DOWN 和 WM_PAINT 共用）──────────────────

/// 重绘状态栏（VK_UP/DOWN 和 WM_PAINT 共用）
///
/// # Safety
/// - `dc` 必须是有效的 HDC
/// - `s` 必须是可变的 AppState 引用
pub unsafe fn redraw_status_bar(dc: HDC, s: &mut AppState, ly: i32, vis: usize) {
    let sh = status_bar_h(s.dpi, s.status_font_size);
    let sy = ly + vis as i32 * s.item_h;
    let sr = RECT { left: PD + 4, top: sy + 2, right: s.width - PD - 4, bottom: sy + sh - 2 };
    let bg_brush = CreateSolidBrush(colorref(s.theme_color));
    FillRect(dc, &sr, bg_brush);
    let _ = DeleteObject(HGDIOBJ(bg_brush.0));
    if s.status_hfont.is_none() {
        s.status_hfont = make_font_with(s.dpi, &s.font_name, s.status_font_size).ok();
    }
    if let Some(ref sf) = s.status_hfont {
        SelectObject(dc, HGDIOBJ(sf.0));
    }
    let pos = if s.sel_index < s.filtered_indices.len() { s.sel_index + 1 } else { 0 };
    let txt = format!("第{}条/共{}条", pos, s.filtered_indices.len());
    let mut ws: Vec<u16> = txt.encode_utf16().collect();
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, colorref(s.text_color));
    let mut sr2 = sr;
    DrawTextW(dc, &mut ws, &mut sr2, DT_RIGHT | DT_VCENTER | DT_SINGLELINE);
    if let Some(ref f) = s.hfont {
        SelectObject(dc, HGDIOBJ(f.0));
    }
}
