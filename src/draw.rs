// Gua — D2D 自绘函数

use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;

use crate::state::*;

pub unsafe fn d2d_fill_round_rect(d2d: &ID2D1DeviceContext, x: f32, y: f32, w: f32, h: f32, r: f32, brush: &ID2D1Brush) {
    let rounded = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F { left: x, top: y, right: x + w, bottom: y + h },
        radiusX: r,
        radiusY: r,
    };
    d2d.FillRoundedRectangle(&rounded as *const _, brush);
}

pub unsafe fn d2d_draw_text(d2d: &ID2D1DeviceContext, text: &str, format: &IDWriteTextFormat, rect: &D2D_RECT_F, brush: &ID2D1Brush) {
    let ws: Vec<u16> = text.encode_utf16().collect();
    d2d.DrawText(
        &ws,
        format,
        rect as *const _,
        brush,
        D2D1_DRAW_TEXT_OPTIONS(0),
        DWRITE_MEASURING_MODE(0),
    );
}

pub unsafe fn d2d_draw_entry_text(d2d: &ID2D1DeviceContext, s: &AppState, list_index: usize, rect: &D2D_RECT_F, brush: &ID2D1Brush) {
    if let Some(&idx) = s.filtered_indices.get(list_index) {
        if idx < s.entries.len() {
            let e = &s.entries[idx];
            let tag = e.category.as_deref().unwrap_or_else(|| entry_type(&e.value));
            let display = e.description.as_deref().unwrap_or(&e.value);
            let txt = format!("[{}]  {}  →  {}", tag, e.key, display);
            let text_rect = D2D_RECT_F {
                left: rect.left + 8.0,
                top: rect.top + 6.0,
                right: rect.right - 4.0,
                bottom: rect.bottom - 6.0,
            };
            if let Some(ref tf) = s.text_format {
                d2d_draw_text(d2d, &txt, tf, &text_rect, brush);
            }
        }
    }
}

pub unsafe fn d2d_draw_filtered_item(d2d: &ID2D1DeviceContext, s: &AppState, list_index: usize, rect: &D2D_RECT_F) {
    let is_sel = list_index == s.sel_index;
    let rcr = (s.round_corner / 2).max(1) as f32;

    if is_sel {
        if let Some(ref brush) = s.accent_brush {
            d2d_fill_round_rect(d2d, rect.left + 2.0, rect.top + 2.0,
                rect.right - rect.left - 4.0, rect.bottom - rect.top - 4.0, rcr, brush);
        }
    }

    if is_sel {
        if let Some(ref wb) = s.white_brush {
            d2d_draw_entry_text(d2d, s, list_index, rect, wb);
        }
    } else if let Some(ref brush) = s.text_brush {
        d2d_draw_entry_text(d2d, s, list_index, rect, brush);
    }
}

pub unsafe fn d2d_redraw_status_bar(d2d: &ID2D1DeviceContext, s: &AppState, ly: f32, vis: usize) {
    let sh = status_bar_h(s.dpi, s.status_font_size) as f32;
    let sy = ly + vis as f32 * s.item_h as f32;
    let sr = D2D_RECT_F {
        left: PD as f32 + 4.0,
        top: sy + 2.0,
        right: s.width as f32 - PD as f32 - 4.0,
        bottom: sy + sh - 2.0,
    };

    let pos = if s.sel_index < s.filtered_indices.len() { s.sel_index + 1 } else { 0 };
    let txt = format!("第{}条/共{}条", pos, s.filtered_indices.len());
    if let Some(ref tf) = s.status_text_format {
        if let Some(ref brush) = s.text_brush {
            d2d_draw_text(d2d, &txt, tf, &sr, brush);
        }
    }
}
