// Gua — 可复用控件库

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;

use crate::theme::*;
use windows_numerics::Vector2;

pub struct D2DRes {
    pub d2d: ID2D1DeviceContext,
    pub dwrite: IDWriteFactory,
}

#[derive(Clone, Copy, PartialEq)]
pub enum WidgetCmd {
    None,
    EntryDel(usize),
    EntryAdd(usize),
    CatToggle(usize),
    ExpandAll,
    CollapseAll,
    CatRename(usize),
    CatDelete(usize),
    FontRefresh,
    FontOpen,
}

pub trait Widget {
    fn draw(&self, res: &D2DRes);
    fn set_bounds(&mut self, r: D2D_RECT_F);
    fn on_click(&mut self, _x: f32, _y: f32) -> bool { false }
    fn on_click_with(&mut self, x: f32, y: f32, _res: &D2DRes) -> bool { self.on_click(x, y) }
    fn on_mouse_down(&mut self, _x: f32, _y: f32) {}
    fn on_mouse_up(&mut self, _x: f32, _y: f32) {}
    fn on_mouse_move(&mut self, x: f32, y: f32);
    #[allow(dead_code)]
    fn on_mouse_leave(&mut self);
    fn on_key_down(&mut self, _vk: u32) -> bool { false }
    fn on_char(&mut self, _ch: u32) -> bool { false }
    fn focused(&self) -> bool { false }
    fn set_focused(&mut self, val: bool);
    fn set_text(&mut self, _text: &str) {}
    fn text(&self) -> &str { "" }
    fn captures_hotkey(&self) -> bool { false }
    fn on_mouse_wheel(&mut self, _delta: f32) -> bool { false }
    fn draw_overlay(&self, _res: &D2DRes) {}
    fn cmd(&self) -> WidgetCmd { WidgetCmd::None }
    fn bounds(&self) -> D2D_RECT_F { D2D_RECT_F::default() }
    fn tick(&mut self) -> bool { false }
    fn settings_key(&self) -> Option<&str> { None }
    fn on_ctrl_key(&mut self, _vk: u32) -> bool { false }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { unimplemented!() }
}

// ── helpers ──

fn make_tf(dwrite: &IDWriteFactory, sz: f32) -> Option<IDWriteTextFormat> {
    unsafe {
        dwrite.CreateTextFormat(
            PCWSTR(crate::state::FONT_FAMILY.as_ptr()), None,
            DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL, sz, PCWSTR(crate::state::FONT_LOCALE.as_ptr()),
        ).ok()
    }
}

fn draw_text(d2d: &ID2D1DeviceContext, text: &str, tf: &IDWriteTextFormat, r: &D2D_RECT_F, brush: &ID2D1Brush) {
    let ws: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        d2d.DrawText(&ws, tf, r as *const _, brush, D2D1_DRAW_TEXT_OPTIONS(0), DWRITE_MEASURING_MODE(0));
    }
}

fn shared_sel_range(sel_start: Option<usize>, sel_end: usize) -> Option<(usize, usize)> {
    sel_start.map(|s| (s.min(sel_end), s.max(sel_end)))
}

fn shared_replace_sel(text: &mut String, cursor_pos: &mut usize, sel_start: &mut Option<usize>, sel_end: &mut usize, new: &str) {
    if let Some((lo, hi)) = shared_sel_range(*sel_start, *sel_end) {
        text.replace_range(lo..hi, new);
        *cursor_pos = lo + new.len();
        *sel_start = None;
    }
}

fn shared_on_ctrl_key(text: &mut String, cursor_pos: &mut usize, sel_start: &mut Option<usize>, sel_end: &mut usize, scroll_hold: &std::cell::Cell<bool>, vk: u32) -> bool {
    match vk {
        0x41 => {
            if text.is_empty() { return true; }
            *sel_start = Some(0);
            *sel_end = text.len();
            *cursor_pos = text.len();
            true
        }
        0x43 => {
            if let Some((lo, hi)) = shared_sel_range(*sel_start, *sel_end) {
                if lo < hi { clipboard_copy(&text[lo..hi]); }
            }
            true
        }
        0x56 => {
            if let Some(paste) = clipboard_paste() {
                if let Some((lo, hi)) = shared_sel_range(*sel_start, *sel_end) {
                    text.replace_range(lo..hi, &paste);
                    *cursor_pos = lo + paste.len();
                    *sel_start = None;
                } else {
                    text.insert_str(*cursor_pos, &paste);
                    *cursor_pos += paste.len();
                }
                scroll_hold.set(false);
            }
            true
        }
        0x58 => {
            if let Some((lo, hi)) = shared_sel_range(*sel_start, *sel_end) {
                if lo < hi { clipboard_copy(&text[lo..hi]); }
                text.replace_range(lo..hi, "");
                *cursor_pos = lo;
                *sel_start = None;
                scroll_hold.set(false);
            }
            true
        }
        _ => false,
    }
}

fn shared_on_key_down(text: &mut String, cursor_pos: &mut usize, sel_start: &mut Option<usize>, sel_end: &mut usize, vk: u32) -> bool {
    match vk {
        0x08 => {
            if let Some((lo, hi)) = shared_sel_range(*sel_start, *sel_end) {
                text.replace_range(lo..hi, "");
                *cursor_pos = lo;
                *sel_start = None;
            } else if *cursor_pos > 0 {
                let prev = text.floor_char_boundary(*cursor_pos - 1);
                text.replace_range(prev..*cursor_pos, "");
                *cursor_pos = prev;
            }
            true
        }
        0x2E => {
            if let Some((lo, hi)) = shared_sel_range(*sel_start, *sel_end) {
                text.replace_range(lo..hi, "");
                *cursor_pos = lo;
                *sel_start = None;
            } else if *cursor_pos < text.len() {
                let next = text.ceil_char_boundary(*cursor_pos + 1);
                text.replace_range(*cursor_pos..next, "");
            }
            true
        }
        0x25 => {
            if let Some((lo, _)) = shared_sel_range(*sel_start, *sel_end) {
                *cursor_pos = lo;
                *sel_start = None;
            } else if *cursor_pos > 0 {
                *cursor_pos = text.floor_char_boundary(*cursor_pos - 1);
            }
            true
        }
        0x27 => {
            if let Some((_, hi)) = shared_sel_range(*sel_start, *sel_end) {
                *cursor_pos = hi;
                *sel_start = None;
            } else if *cursor_pos < text.len() {
                *cursor_pos = text.ceil_char_boundary(*cursor_pos + 1);
            }
            true
        }
        _ => false,
    }
}


fn text_width(dwrite: &IDWriteFactory, tf: &IDWriteTextFormat, text: &str) -> f32 {
    let ws: Vec<u16> = text.encode_utf16().collect();
    if ws.is_empty() { return 0.0; }
    if let Ok(layout) = unsafe { dwrite.CreateTextLayout(&ws, tf, 10000.0, 10000.0) } {
        let mut m = DWRITE_TEXT_METRICS::default();
        if unsafe { layout.GetMetrics(&mut m).is_ok() } {
            return m.widthIncludingTrailingWhitespace;
        }
    }
    0.0
}

/// Convert UTF-16 code unit position to byte position in a Rust string
pub fn utf16_to_byte(text: &str, utf16_pos: usize) -> usize {
    let mut u16_count = 0;
    for (byte_pos, c) in text.char_indices() {
        if u16_count >= utf16_pos { return byte_pos; }
        u16_count += c.len_utf16();
    }
    text.len()
}

/// Convert byte position to UTF-16 code unit position
pub fn byte_to_utf16(text: &str, byte_pos: usize) -> usize {
    let mut u16_sum = 0;
    for (i, c) in text.char_indices() {
        if i >= byte_pos { return u16_sum; }
        u16_sum += c.len_utf16();
    }
    u16_sum
}

fn tf_center(dwrite: &IDWriteFactory, sz: f32) -> Option<IDWriteTextFormat> {
    let tf = make_tf(dwrite, sz)?;
    unsafe {
        let _ = tf.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
        let _ = tf.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
    }
    Some(tf)
}

fn tf_vcenter(dwrite: &IDWriteFactory, sz: f32) -> Option<IDWriteTextFormat> {
    let tf = make_tf(dwrite, sz)?;
    unsafe { let _ = tf.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER); }
    Some(tf)
}

// ── Label ──

pub struct Label {
    r: D2D_RECT_F,
    pub text: String,
}

impl Label {
    pub fn new(text: &str) -> Self { Self { r: D2D_RECT_F::default(), text: text.to_string() } }
}

impl Widget for Label {
    fn set_bounds(&mut self, r: D2D_RECT_F) { self.r = r; }
    fn bounds(&self) -> D2D_RECT_F { self.r }
    fn on_mouse_move(&mut self, _x: f32, _y: f32) {}
    fn on_mouse_leave(&mut self) {}
    fn set_focused(&mut self, _val: bool) {}
    fn draw(&self, res: &D2DRes) {
        if let Some(tf) = tf_vcenter(&res.dwrite, 14.0) {
            if let Some(b) = brush(&res.d2d, T.tab_text, 1.0) {
                draw_text(&res.d2d, &self.text, &tf, &self.r, &b);
            }
        }
    }
}

// ── GroupHeader ──

pub struct GroupHeader {
    r: D2D_RECT_F,
    pub text: String,
}

impl GroupHeader {
    pub fn new(text: &str) -> Self { Self { r: D2D_RECT_F::default(), text: text.to_string() } }
}

impl Widget for GroupHeader {
    fn set_bounds(&mut self, r: D2D_RECT_F) { self.r = r; }
    fn bounds(&self) -> D2D_RECT_F { self.r }
    fn on_mouse_move(&mut self, _x: f32, _y: f32) {}
    fn on_mouse_leave(&mut self) {}
    fn set_focused(&mut self, _val: bool) {}
    fn draw(&self, res: &D2DRes) {
        let text_r = D2D_RECT_F { left: self.r.left, top: self.r.top, right: self.r.right, bottom: self.r.top + 20.0 };
        if let Some(tf) = tf_vcenter(&res.dwrite, 14.0) {
            if let Some(b) = brush(&res.d2d, T.text_bright, 1.0) {
                draw_text(&res.d2d, &self.text, &tf, &text_r, &b);
            }
        }
        let sep_y = self.r.top + 24.0;
        if let Some(b) = brush(&res.d2d, T.bg_input, 1.0) {
            let sep = D2D_RECT_F { left: self.r.left, top: sep_y, right: self.r.right, bottom: sep_y + 1.0 };
            unsafe { res.d2d.FillRectangle(&sep as *const _, &b); }
        }
    }
}

// ── ToggleSwitch ──

pub struct ToggleSwitch {
    r: D2D_RECT_F,
    pub checked: bool,
    hovered: bool,
    pub settings_key: Option<String>,
}

impl ToggleSwitch {
    pub fn new(checked: bool) -> Self {
        Self { r: D2D_RECT_F::default(), checked, hovered: false, settings_key: None }
    }
}

impl Widget for ToggleSwitch {
    fn set_bounds(&mut self, r: D2D_RECT_F) { self.r = r; }
    fn bounds(&self) -> D2D_RECT_F { self.r }
    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.hovered = self.r.right - 40.0 <= x && x <= self.r.right && y >= self.r.top && y <= self.r.bottom;
    }
    fn on_mouse_leave(&mut self) { self.hovered = false; }
    fn set_focused(&mut self, _val: bool) {}
    fn text(&self) -> &str { if self.checked { "true" } else { "false" } }
    fn settings_key(&self) -> Option<&str> { self.settings_key.as_deref() }

    fn on_click(&mut self, x: f32, y: f32) -> bool {
        let track_l = self.r.right - 40.0;
        if !(track_l <= x && x <= self.r.right && y >= self.r.top && y <= self.r.bottom) { return false; }
        self.checked = !self.checked;
        true
    }

    fn draw(&self, res: &D2DRes) {
        let track_w = 40.0; let track_h = 22.0; let thumb_d = 18.0;
        let cy = (self.r.top + self.r.bottom) / 2.0;
        let track_l = self.r.right - track_w;
        let track_t = cy - track_h / 2.0;
        let track_r = self.r.right;
        let track_b = cy + track_h / 2.0;
        let track_rect = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F { left: track_l, top: track_t, right: track_r, bottom: track_b },
            radiusX: track_h / 2.0, radiusY: track_h / 2.0,
        };
        if self.checked {
            if let Some(b) = brush(&res.d2d, T.accent, 1.0) {
                unsafe { res.d2d.FillRoundedRectangle(&track_rect as *const _, &b); }
            }
            let thumb_cx = track_r - thumb_d / 2.0 - 2.0;
            let thumb_rect = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F { left: thumb_cx - thumb_d / 2.0, top: cy - thumb_d / 2.0, right: thumb_cx + thumb_d / 2.0, bottom: cy + thumb_d / 2.0 },
                radiusX: thumb_d / 2.0, radiusY: thumb_d / 2.0,
            };
            if let Some(b) = brush(&res.d2d, T.text_white, 1.0) {
                unsafe { res.d2d.FillRoundedRectangle(&thumb_rect as *const _, &b); }
            }
        } else {
            if let Some(b) = brush(&res.d2d, T.border_hover, 1.0) {
                unsafe { res.d2d.FillRoundedRectangle(&track_rect as *const _, &b); }
            }
            if let Some(b) = brush(&res.d2d, T.text_secondary, 1.0) {
                unsafe { let _ = res.d2d.DrawRoundedRectangle(&track_rect as *const _, &b, 1.5, None as Option<&ID2D1StrokeStyle>); }
            }
            let thumb_cx = track_l + thumb_d / 2.0 + 2.0;
            let thumb_rect = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F { left: thumb_cx - thumb_d / 2.0, top: cy - thumb_d / 2.0, right: thumb_cx + thumb_d / 2.0, bottom: cy + thumb_d / 2.0 },
                radiusX: thumb_d / 2.0, radiusY: thumb_d / 2.0,
            };
            if let Some(b) = brush(&res.d2d, T.tab_text, 1.0) {
                unsafe { res.d2d.FillRoundedRectangle(&thumb_rect as *const _, &b); }
            }
        }
        if self.hovered && !self.checked {
            if let Some(b) = brush(&res.d2d, (0.4, 0.6, 0.8), 0.15) {
                unsafe { res.d2d.FillRoundedRectangle(&track_rect as *const _, &b); }
            }
        }
    }
}

// ── TextInput ──

pub struct TextInput {
    r: D2D_RECT_F,
    pub text: String,
    pub placeholder: String,
    focused: bool,
    cursor_pos: usize,
    hovered: bool,
    pub center: bool,
    pub select_on_focus: bool,
    scroll_x: std::cell::Cell<f32>,
    scroll_hold: std::cell::Cell<bool>,
    mouse_down: bool,
    sel_start: Option<usize>,
    sel_end: usize,
    dwrite_factory: Option<IDWriteFactory>,
    pub settings_key: Option<String>,
    undo_stack: Vec<(String, usize)>,
}

impl TextInput {
    pub fn new(text: &str) -> Self {
        Self { r: D2D_RECT_F::default(), text: text.to_string(), placeholder: String::new(), focused: false, cursor_pos: text.len(), hovered: false, center: false, select_on_focus: true, scroll_x: std::cell::Cell::new(0.0), scroll_hold: std::cell::Cell::new(true), mouse_down: false, sel_start: None, sel_end: 0, dwrite_factory: None, settings_key: None, undo_stack: Vec::new() }
    }
    fn push_undo(&mut self) {
        self.undo_stack.push((self.text.clone(), self.cursor_pos));
        if self.undo_stack.len() > 30 {
            self.undo_stack.remove(0);
        }
    }
    fn sel_range(&self) -> Option<(usize, usize)> {
        shared_sel_range(self.sel_start, self.sel_end)
    }
    fn replace_sel(&mut self, new: &str) {
        shared_replace_sel(&mut self.text, &mut self.cursor_pos, &mut self.sel_start, &mut self.sel_end, new);
    }
    fn sync_scroll(&self) {
        let use_center = if self.center {
            self.dwrite_factory.as_ref().and_then(|dwf| {
                make_tf(dwf, 14.0).map(|tf| text_width(dwf, &tf, &self.text) <= (self.r.right - self.r.left))
            }).unwrap_or(true)
        } else { false };
        if use_center || self.text.is_empty() { return; }
        if let Some(ref dwf) = self.dwrite_factory {
            if let Some(tf) = tf_vcenter_nowrap(dwf, 14.0) {
                let ws: Vec<u16> = self.text.encode_utf16().collect();
                let box_w = (self.r.right - self.r.left - 16.0).max(1.0);
                if let Ok(layout) = unsafe { dwf.CreateTextLayout(&ws, &tf, box_w, 10000.0) } {
                    let sx = self.scroll_x.get();
                    let far_u16 = byte_to_utf16(&self.text, self.cursor_pos) as u32;
                    let mut fpx = 0.0f32; let mut fpy = 0.0f32;
                    let mut fhit = DWRITE_HIT_TEST_METRICS::default();
                    let _ = unsafe { layout.HitTestTextPosition(far_u16, false, &mut fpx, &mut fpy, &mut fhit) };
                    let cushion = 10.0;
                    if fpx < sx + cushion { self.scroll_x.set((fpx - cushion).max(0.0)); }
                    else if fpx - sx > box_w - cushion { self.scroll_x.set((fpx - box_w + cushion).max(0.0)); }
                }
            }
        }
    }
}

impl Widget for TextInput {
    fn set_bounds(&mut self, r: D2D_RECT_F) { self.r = r; }
    fn bounds(&self) -> D2D_RECT_F { self.r }
    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.hovered = x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom;
        if self.mouse_down && self.focused {
            if let Some(ref dwf) = self.dwrite_factory {
                let use_center = if self.center {
                    make_tf(dwf, 14.0).map(|tf| text_width(dwf, &tf, &self.text) <= (self.r.right - self.r.left)).unwrap_or(true)
                } else { false };
                let mut sx = self.scroll_x.get();
                let box_w = if use_center { self.r.right - self.r.left } else { (self.r.right - self.r.left - 16.0).max(1.0) };
                // Horizontal edge auto-scroll (only when scrolling does something)
                if !use_center {
                    let tw = make_tf(dwf, 14.0).map(|tf| text_width(dwf, &tf, &self.text)).unwrap_or(0.0);
                    let edge = 16.0;
                    let can_scroll_left = sx > 0.0;
                    let can_scroll_right = tw > sx + box_w;
                    if can_scroll_left && x < self.r.left + edge {
                        sx = (sx - 4.0).max(0.0);
                    } else if can_scroll_right && x > self.r.right - edge {
                        sx = sx + 4.0;
                    }
                    self.scroll_x.set(sx);
                }
                let rel_x = if use_center { x - self.r.left } else { x - (self.r.left + 8.0) + sx };
                let tf_fmt = if use_center { tf_center_nowrap(dwf, 14.0) } else { tf_vcenter_nowrap(dwf, 14.0) };
                if let Some(tf) = tf_fmt {
                    let ws: Vec<u16> = self.text.encode_utf16().collect();
                    if let Ok(layout) = unsafe { dwf.CreateTextLayout(&ws, &tf, box_w, 10000.0) } {
                        let mut is_trailing = BOOL::default();
                        let mut is_inside = BOOL::default();
                        let mut hit = DWRITE_HIT_TEST_METRICS::default();
                        let _ = unsafe { layout.HitTestPoint(rel_x, 0.0, &mut is_trailing, &mut is_inside, &mut hit) };
                        let mut utf16_pos = hit.textPosition as usize;
                        if is_trailing.as_bool() { utf16_pos += hit.length as usize; }
                        self.sel_end = utf16_to_byte(&self.text, utf16_pos);
                    }
                }
            }
        }
    }

    fn on_mouse_leave(&mut self) { self.hovered = false; }

    fn on_mouse_down(&mut self, x: f32, y: f32) {
        self.scroll_hold.set(false);
        if x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom {
            self.mouse_down = true;
            if self.sel_start.map_or(true, |s| s == self.sel_end) {
                self.sel_start = Some(self.cursor_pos);
                self.sel_end = self.cursor_pos;
            }
        }
    }

    fn on_mouse_up(&mut self, _x: f32, _y: f32) {
        if self.mouse_down {
            self.mouse_down = false;
            self.scroll_hold.set(true);
            if let Some(start) = self.sel_start {
                if start == self.sel_end {
                    self.sel_start = None;
                }
            }
        }
    }

    fn on_click_with(&mut self, x: f32, y: f32, res: &D2DRes) -> bool {
        self.scroll_hold.set(false);
        if !(x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom) { return false; }
        self.dwrite_factory = Some(res.dwrite.clone());
        self.sel_start = None;
        if !self.focused {
            self.focused = true;
            if self.select_on_focus && !self.text.is_empty() {
                self.sel_start = Some(0);
                self.sel_end = self.text.len();
                self.cursor_pos = self.text.len();
                self.scroll_x.set(0.0);
            }
        }
        let use_center = if self.center {
            make_tf(&res.dwrite, 14.0).map(|tf| text_width(&res.dwrite, &tf, &self.text) <= (self.r.right - self.r.left)).unwrap_or(true)
        } else { false };
        let ws: Vec<u16> = self.text.encode_utf16().collect();
        let box_w = if use_center { self.r.right - self.r.left } else { (self.r.right - self.r.left - 16.0).max(1.0) };
        let sx = self.scroll_x.get();
        let rel_x = if use_center { x - self.r.left } else { x - (self.r.left + 8.0) + sx };
        if let Some(tf) = if use_center { tf_center_nowrap(&res.dwrite, 14.0) } else { tf_vcenter_nowrap(&res.dwrite, 14.0) } {
            if let Ok(layout) = unsafe { res.dwrite.CreateTextLayout(&ws, &tf, box_w, 10000.0) } {
                let mut is_trailing = BOOL::default();
                let mut is_inside = BOOL::default();
                let mut hit = DWRITE_HIT_TEST_METRICS::default();
                let _ = unsafe { layout.HitTestPoint(rel_x, 0.0, &mut is_trailing, &mut is_inside, &mut hit) };
                let mut utf16_pos = hit.textPosition as usize;
                if is_trailing.as_bool() { utf16_pos += hit.length as usize; }
                self.cursor_pos = utf16_to_byte(&self.text, utf16_pos);
            } else {
                self.cursor_pos = self.text.len();
            }
        } else {
            self.cursor_pos = self.text.len();
        }
        true
    }

    fn set_focused(&mut self, val: bool) {
        self.focused = val;
        if !val { self.scroll_hold.set(true); self.scroll_x.set(0.0); self.cursor_pos = 0; self.sel_start = None; self.mouse_down = false; }
    }
    fn focused(&self) -> bool { self.focused }
    fn text(&self) -> &str { &self.text }
    fn settings_key(&self) -> Option<&str> { self.settings_key.as_deref() }

    fn on_ctrl_key(&mut self, vk: u32) -> bool {
        match vk {
            0x5A => {
                if let Some((prev_text, prev_cursor)) = self.undo_stack.pop() {
                    self.text = prev_text;
                    self.cursor_pos = prev_cursor.min(self.text.len());
                    self.sel_start = None;
                    self.sel_end = 0;
                    self.scroll_x.set(0.0);
                    self.scroll_hold.set(false);
                }
                return true;
            }
            0x56 | 0x58 => {
                self.push_undo();
            }
            _ => {}
        }
        let r = shared_on_ctrl_key(&mut self.text, &mut self.cursor_pos, &mut self.sel_start, &mut self.sel_end, &self.scroll_hold, vk);
        if r && vk == 0x41 { self.scroll_x.set(0.0); }
        r
    }

    fn on_key_down(&mut self, vk: u32) -> bool {
        self.scroll_hold.set(false);
        match vk {
            0x08 | 0x2E => self.push_undo(),
            _ => {}
        }
        let r = shared_on_key_down(&mut self.text, &mut self.cursor_pos, &mut self.sel_start, &mut self.sel_end, vk);
        match vk {
            0x08 | 0x2E => { if r && self.text.is_empty() { self.scroll_x.set(0.0); } r }
            0x25 | 0x27 => { if r { self.sync_scroll(); } r }
            _ => r,
        }
    }

    fn on_char(&mut self, ch: u32) -> bool {
        self.scroll_hold.set(false);
        if let Some(c) = char::from_u32(ch) {
            if !c.is_control() {
                self.push_undo();
                if self.sel_start.is_some() {
                    self.replace_sel(&c.to_string());
                } else {
                    self.text.insert(self.cursor_pos, c);
                    self.cursor_pos += c.len_utf8();
                }
                return true;
            }
        }
        false
    }

    fn draw(&self, res: &D2DRes) {
        let inp_rr = D2D1_ROUNDED_RECT { rect: self.r, radiusX: 6.0, radiusY: 6.0 };
        let bg = if self.focused { T.bg_input } else if self.hovered { T.bg_hover } else { T.bg_separator };
        if let Some(b) = brush(&res.d2d, bg, 1.0) {
            unsafe { res.d2d.FillRoundedRectangle(&inp_rr as *const _, &b); }
        }
        let bc = if self.hovered && !self.focused { T.border_focused } else { T.border };
        if let Some(b) = brush(&res.d2d, bc, 1.0) {
            unsafe { let _ = res.d2d.DrawRoundedRectangle(&inp_rr as *const _, &b, 1.0, None as Option<&ID2D1StrokeStyle>); }
        }

        let sx = self.scroll_x.get();
        let text_r = D2D_RECT_F { left: self.r.left + 8.0 - sx, top: self.r.top, right: self.r.right - 8.0 - sx, bottom: self.r.bottom };
        let clip_r = D2D_RECT_F { left: self.r.left + 6.0, top: self.r.top, right: self.r.right - 6.0, bottom: self.r.bottom };
        unsafe { res.d2d.PushAxisAlignedClip(&clip_r as *const _, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE); }

        // If center mode text is wider than the box, treat as left-aligned
        let use_center = if self.center {
            let fits = make_tf(&res.dwrite, 14.0).map(|tf| text_width(&res.dwrite, &tf, &self.text) <= (self.r.right - self.r.left)).unwrap_or(true);
            fits
        } else { false };

        // ── text drawing (independent of layout) ──
        if self.focused && self.sel_start == Some(0) && self.sel_end == self.text.len() && !self.text.is_empty() {
            if let Some(tf) = if use_center { tf_center_nowrap(&res.dwrite, 14.0) } else { tf_vcenter_nowrap(&res.dwrite, 14.0) } {
                let tw = text_width(&res.dwrite, &tf, &self.text);
                let box_w = self.r.right - self.r.left;
                let (hl_l, hl_w) = if use_center {
                    let text_start = self.r.left + (box_w - tw) / 2.0;
                    (text_start, tw)
                } else {
                    (self.r.left + 8.0, tw)
                };
                if let Some(b) = brush(&res.d2d, T.accent, 1.0) {
                    unsafe { res.d2d.FillRectangle(&D2D_RECT_F { left: hl_l.max(self.r.left + 4.0), top: self.r.top + 2.0, right: (hl_l + hl_w + 4.0).min(self.r.right - 4.0), bottom: self.r.bottom - 2.0 } as *const _, &b); }
                }
                if let Some(b) = brush(&res.d2d, T.text_white, 1.0) {
                    if use_center { draw_text(&res.d2d, &self.text, &tf, &D2D_RECT_F { left: self.r.left, top: self.r.top, right: self.r.right, bottom: self.r.bottom }, &b); }
                    else { draw_text(&res.d2d, &self.text, &tf, &text_r, &b); }
                }
            }
        } else if self.text.is_empty() && !self.focused && !self.placeholder.is_empty() {
            if let Some(tf) = tf_vcenter_nowrap(&res.dwrite, 14.0) {
                if let Some(b) = brush(&res.d2d, T.text_dim, 1.0) { draw_text(&res.d2d, &self.placeholder, &tf, &text_r, &b); }
            }
        } else if use_center {
            if let Some(tf) = tf_center_nowrap(&res.dwrite, 14.0) {
                let full_r = D2D_RECT_F { left: self.r.left, top: self.r.top, right: self.r.right, bottom: self.r.bottom };
                if let Some(b) = brush(&res.d2d, T.text_bright, 1.0) { draw_text(&res.d2d, &self.text, &tf, &full_r, &b); }
            }
        } else {
            if let Some(tf) = tf_vcenter_nowrap(&res.dwrite, 14.0) {
                if let Some(b) = brush(&res.d2d, T.text_bright, 1.0) { draw_text(&res.d2d, &self.text, &tf, &text_r, &b); }
            }
        }

        // ── selection + cursor + auto-scroll (via TextLayout, independent of text drawing) ──
        if let Some(tf) = if use_center { tf_center_nowrap(&res.dwrite, 14.0) } else { tf_vcenter_nowrap(&res.dwrite, 14.0) } {
            let ws: Vec<u16> = self.text.encode_utf16().collect();
            let box_w = if use_center { self.r.right - self.r.left } else { (self.r.right - self.r.left - 16.0).max(1.0) };
            if let Ok(layout) = unsafe { res.dwrite.CreateTextLayout(&ws, &tf, box_w, 10000.0) } {
                // Selection highlight
                if let Some((lo, hi)) = self.sel_range() {
                    if lo != hi {
                        let mut px1 = 0.0f32; let mut py1 = 0.0f32;
                        let mut px2 = 0.0f32; let mut py2 = 0.0f32;
                        let mut h1 = DWRITE_HIT_TEST_METRICS::default();
                        let mut h2 = DWRITE_HIT_TEST_METRICS::default();
                        let _ = unsafe { layout.HitTestTextPosition(byte_to_utf16(&self.text, lo) as u32, false, &mut px1, &mut py1, &mut h1) };
                        let _ = unsafe { layout.HitTestTextPosition(byte_to_utf16(&self.text, hi) as u32, false, &mut px2, &mut py2, &mut h2) };
                        let sel_l = if use_center { self.r.left + px1.min(px2) } else { self.r.left + 8.0 + px1.min(px2) - sx };
                        let sel_r = if use_center { self.r.left + px1.max(px2) } else { self.r.left + 8.0 + px1.max(px2) - sx };
                        if let Some(b) = brush(&res.d2d, T.accent, 0.30) {
                            unsafe { res.d2d.FillRectangle(&D2D_RECT_F { left: sel_l, top: self.r.top + 2.0, right: sel_r, bottom: self.r.bottom - 2.0 } as *const _, &b); }
                        }
                    }
                }
                // Cursor
                if self.focused && self.sel_start.is_none() {
                    let mut px = 0.0f32; let mut py = 0.0f32;
                    let mut hit = DWRITE_HIT_TEST_METRICS::default();
                    let _ = unsafe { layout.HitTestTextPosition(byte_to_utf16(&self.text, self.cursor_pos) as u32, false, &mut px, &mut py, &mut hit) };
                    let cx = if use_center { self.r.left + px } else { self.r.left + 8.0 + px - sx };
                    let cy = self.r.top + 4.0;
                    let ch = self.r.bottom - self.r.top - 8.0;
                    if let Some(b) = brush(&res.d2d, T.placeholder, 1.0) {
                        unsafe { res.d2d.FillRectangle(&D2D_RECT_F { left: cx, top: cy, right: cx + 1.5, bottom: cy + ch } as *const _, &b); }
                    }
                }
                // Auto-scroll (only for left-aligned, only when focused)
                if !use_center && self.focused {
                    if !self.scroll_hold.get() || self.mouse_down {
                        let far = if self.mouse_down { self.sel_end } else { self.cursor_pos };
                        let box_w2 = self.r.right - self.r.left - 16.0;
                        if far > 0 {
                            let far_u16 = byte_to_utf16(&self.text, far) as u32;
                            let mut fpx = 0.0f32; let mut fpy = 0.0f32;
                            let mut fhit = DWRITE_HIT_TEST_METRICS::default();
                            let _ = unsafe { layout.HitTestTextPosition(far_u16, false, &mut fpx, &mut fpy, &mut fhit) };
                            let cushion = 10.0;
                            if fpx < sx + cushion { self.scroll_x.set((fpx - cushion).max(0.0)); }
                            else if fpx - sx > box_w2 - cushion { self.scroll_x.set((fpx - box_w2 + cushion).max(0.0)); }
                        } else {
                            self.scroll_x.set(0.0);
                        }
                    }
                }
            }
        }
        unsafe { res.d2d.PopAxisAlignedClip(); }
    }
}

// ── IconButton ──

pub struct IconButton {
    r: D2D_RECT_F,
    pub icon: String,
    hovered: bool,
    pub cmd: WidgetCmd,
    pub bordered: bool,
}

impl IconButton {
    pub fn new(icon: &str) -> Self {
        Self { r: D2D_RECT_F::default(), icon: icon.to_string(), hovered: false, cmd: WidgetCmd::None, bordered: false }
    }
}

impl Widget for IconButton {
    fn cmd(&self) -> WidgetCmd { self.cmd }
    fn set_bounds(&mut self, r: D2D_RECT_F) { self.r = r; }
    fn bounds(&self) -> D2D_RECT_F { self.r }
    fn on_mouse_move(&mut self, x: f32, y: f32) { self.hovered = x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom; }
    fn on_mouse_leave(&mut self) { self.hovered = false; }
    fn set_focused(&mut self, _val: bool) {}
    fn on_click(&mut self, x: f32, y: f32) -> bool {
        x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom
    }

    fn draw(&self, res: &D2DRes) {
        if self.bordered {
            let bc = if self.hovered { T.accent } else { T.text_secondary };
            let rr = D2D1_ROUNDED_RECT { rect: self.r, radiusX: 4.0, radiusY: 4.0 };
            if let Some(b) = brush(&res.d2d, bc, 1.0) {
                unsafe { let _ = res.d2d.DrawRoundedRectangle(&rr as *const _, &b, 1.0, None as Option<&ID2D1StrokeStyle>); }
            }
        }
        let c = if self.hovered { T.text_bright } else { T.tab_text };
        if let Some(b) = brush(&res.d2d, c, 1.0) {
            if let Some(tf) = tf_center(&res.dwrite, 13.0) {
                draw_text(&res.d2d, &self.icon, &tf, &self.r, &b);
            }
        }
    }
}

// ── ThreeDotsButton ──

pub struct ThreeDotsButton {
    r: D2D_RECT_F,
    pub items: Vec<String>,
    pub open: bool,
    pub selected: Option<usize>,
    hovered: i32,
    cidx: usize,
}

impl ThreeDotsButton {
    pub fn new(items: &[&str], cidx: usize) -> Self {
        Self {
            r: D2D_RECT_F::default(),
            items: items.iter().map(|s| s.to_string()).collect(),
            open: false,
            selected: None,
            hovered: -1,
            cidx,
        }
    }
}

impl Widget for ThreeDotsButton {
    fn cmd(&self) -> WidgetCmd {
        match self.selected {
            Some(0) => WidgetCmd::CatRename(self.cidx),
            Some(1) => WidgetCmd::CatDelete(self.cidx),
            _ => WidgetCmd::None,
        }
    }
    fn set_bounds(&mut self, r: D2D_RECT_F) { self.r = r; }
    fn bounds(&self) -> D2D_RECT_F { self.r }
    fn set_focused(&mut self, _val: bool) {}
    fn focused(&self) -> bool { self.open }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.hovered = -1;
        if !self.open || self.items.is_empty() { return; }
        let popup_l = self.r.right - 136.0;
        let popup_t = self.r.top + 34.0;
        let item_h = 28.0;
        let popup_h = self.items.len() as f32 * item_h + 8.0;
        if x >= popup_l && x <= popup_l + 120.0 && y >= popup_t && y <= popup_t + popup_h {
            let mi = ((y - popup_t - 4.0) / item_h) as i32;
            self.hovered = mi.min(self.items.len() as i32 - 1);
        }
    }
    fn on_mouse_leave(&mut self) { self.hovered = -1; }

    fn on_click_with(&mut self, x: f32, y: f32, _res: &D2DRes) -> bool {
        let in_btn = x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom;
        if self.open {
            let popup_l = self.r.right - 136.0;
            let popup_t = self.r.top + 34.0;
            let item_h = 28.0;
            let popup_h = self.items.len() as f32 * item_h + 8.0;
            if x >= popup_l && x <= popup_l + 120.0 && y >= popup_t && y <= popup_t + popup_h {
                let mi = ((y - popup_t - 4.0) / item_h) as usize;
                if mi < self.items.len() {
                    self.selected = Some(mi);
                    self.open = false;
                    return true;
                }
            }
            self.open = false;
            return false;
        }
        if in_btn {
            self.open = true;
            self.selected = None;
            return true;
        }
        false
    }

    fn draw(&self, res: &D2DRes) {
        let c = if self.open || self.hovered == -1 { T.text_bright } else { T.tab_text };
        if let Some(b) = brush(&res.d2d, c, 1.0) {
            if let Some(tf) = tf_center(&res.dwrite, 13.0) {
                draw_text(&res.d2d, "⋮", &tf, &self.r, &b);
            }
        }
    }

    fn draw_overlay(&self, res: &D2DRes) {
        if !self.open || self.items.is_empty() { return; }
        let popup_l = self.r.right - 136.0;
        let popup_t = self.r.top + 34.0;
        let popup_r = D2D_RECT_F { left: popup_l, top: popup_t, right: popup_l + 120.0, bottom: popup_t + self.items.len() as f32 * 28.0 + 8.0 };
        let rr = D2D1_ROUNDED_RECT { rect: popup_r, radiusX: 6.0, radiusY: 6.0 };
        if let Some(b) = brush(&res.d2d, T.bg_title, 1.0) {
            unsafe { res.d2d.FillRoundedRectangle(&rr as *const _, &b); }
        }
        if let Some(b) = brush(&res.d2d, T.bg_widget, 1.0) {
            unsafe { let _ = res.d2d.DrawRoundedRectangle(&rr as *const _, &b, 1.0, None as Option<&ID2D1StrokeStyle>); }
        }
        for (mi, label) in self.items.iter().enumerate() {
            let item_y = popup_t + 4.0 + mi as f32 * 28.0;
            let item_r = D2D_RECT_F { left: popup_l + 4.0, top: item_y, right: popup_l + 116.0, bottom: item_y + 26.0 };
            if mi as i32 == self.hovered {
                if let Some(b) = brush(&res.d2d, T.bg_widget, 1.0) {
                    let irr = D2D1_ROUNDED_RECT { rect: item_r, radiusX: 4.0, radiusY: 4.0 };
                    unsafe { res.d2d.FillRoundedRectangle(&irr as *const _, &b); }
                }
            }
            if let Some(tf) = tf_center(&res.dwrite, 13.0) {
                if let Some(b) = brush(&res.d2d, T.placeholder, 1.0) {
                    draw_text(&res.d2d, label, &tf, &item_r, &b);
                }
            }
        }
    }
}

// ── RefreshButton ──

use std::time::Instant;

pub struct RefreshButton {
    r: D2D_RECT_F,
    hovered: bool,
    state: u8, // 0=idle, 1=spinning(dots), 2=done(✓)
    start: Option<Instant>,
    ticks: u32,
    cmd: WidgetCmd,
}

impl RefreshButton {
    pub fn new() -> Self {
        Self { r: D2D_RECT_F::default(), hovered: false, state: 0, start: None, ticks: 0, cmd: WidgetCmd::FontRefresh }
    }
}

impl Widget for RefreshButton {
    fn cmd(&self) -> WidgetCmd { self.cmd }
    fn set_bounds(&mut self, r: D2D_RECT_F) { self.r = r; }
    fn bounds(&self) -> D2D_RECT_F { self.r }
    fn on_mouse_move(&mut self, x: f32, y: f32) { self.hovered = x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom; }
    fn on_mouse_leave(&mut self) { self.hovered = false; }
    fn set_focused(&mut self, _val: bool) {}
    fn on_click(&mut self, x: f32, y: f32) -> bool {
        if !(x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom) { return false; }
        if self.state == 0 {
            self.state = 1;
            self.start = Some(Instant::now());
        }
        true
    }

    fn tick(&mut self) -> bool {
        match self.state {
            1 => {
                self.ticks += 1;
                if self.ticks >= 3 {
                    self.state = 2;
                    self.start = Some(Instant::now());
                }
                true
            }
            2 => {
                let elapsed = self.start.map(|t| t.elapsed()).unwrap_or_default();
                if elapsed >= std::time::Duration::from_millis(500) {
                    self.state = 0;
                    self.start = None;
                    self.ticks = 0;
                    false
                } else { true }
            }
            _ => false
        }
    }

    fn draw(&self, res: &D2DRes) {
        let inner = D2D_RECT_F {
            left: self.r.left + 2.0,
            top: self.r.top + 2.0,
            right: self.r.right - 2.0,
            bottom: self.r.bottom - 2.0,
        };

        // border for idle / spinning
        if self.state != 2 {
            let bc = if self.hovered { T.accent } else { T.text_secondary };
            let rr = D2D1_ROUNDED_RECT { rect: self.r, radiusX: 4.0, radiusY: 4.0 };
            if let Some(b) = brush(&res.d2d, bc, 1.0) {
                unsafe { let _ = res.d2d.DrawRoundedRectangle(&rr as *const _, &b, 1.0, None as Option<&ID2D1StrokeStyle>); }
            }
        }

        if let Some(tf) = tf_center(&res.dwrite, 12.0) {
            if self.state == 1 {
                let dots = [".", "..", "..."];
                let idx = self.ticks.min(2) as usize;
                let tc = if (self.ticks % 2) == 0 { T.tab_text } else { T.text_bright };
                if let Some(b) = brush(&res.d2d, tc, 1.0) {
                    draw_text(&res.d2d, dots[idx], &tf, &inner, &b);
                }
            } else if self.state == 2 {
                if let Some(b) = brush(&res.d2d, T.green, 1.0) {
                    draw_text(&res.d2d, "✓", &tf, &inner, &b);
                }
            } else {
                let tc = if self.hovered { T.text_bright } else { T.tab_text };
                if let Some(b) = brush(&res.d2d, tc, 1.0) {
                    draw_text(&res.d2d, "刷新", &tf, &inner, &b);
                }
            }
        }
    }
}

// ── ClickLabel ──

pub struct ClickLabel {
    r: D2D_RECT_F,
    pub text: String,
    hovered: bool,
    pub cmd: WidgetCmd,
}

impl ClickLabel {
    pub fn new(text: &str) -> Self {
        Self { r: D2D_RECT_F::default(), text: text.to_string(), hovered: false, cmd: WidgetCmd::None }
    }
}

impl Widget for ClickLabel {
    fn cmd(&self) -> WidgetCmd { self.cmd }
    fn set_bounds(&mut self, r: D2D_RECT_F) { self.r = r; }
    fn bounds(&self) -> D2D_RECT_F { self.r }
    fn on_mouse_move(&mut self, x: f32, y: f32) { self.hovered = x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom; }
    fn on_mouse_leave(&mut self) { self.hovered = false; }
    fn set_focused(&mut self, _val: bool) {}
    fn on_click(&mut self, x: f32, y: f32) -> bool {
        x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom
    }

    fn draw(&self, res: &D2DRes) {
        let c = if self.hovered { T.text_bright } else { T.tab_text };
        if let Some(b) = brush(&res.d2d, c, 1.0) {
            if let Some(tf) = tf_vcenter(&res.dwrite, 14.0) {
                draw_text(&res.d2d, &self.text, &tf, &self.r, &b);
            }
        }
    }
}

// ── MultilineTextInput ──

pub struct MultilineTextInput {
    r: D2D_RECT_F,
    pub text: String,
    focused: bool,
    scroll_y: std::cell::Cell<f32>,
    content_h: std::cell::Cell<f32>,
    scroll_hold: std::cell::Cell<bool>,
    hovered: bool,
    cursor_pos: usize,
    mouse_down: bool,
    sel_start: Option<usize>,
    sel_end: usize,
    dwrite_factory: Option<IDWriteFactory>,
    pub settings_key: Option<String>,
    undo_stack: Vec<(String, usize)>,
}

impl MultilineTextInput {
    fn sel_range(&self) -> Option<(usize, usize)> {
        shared_sel_range(self.sel_start, self.sel_end)
    }
    fn replace_sel(&mut self, new: &str) {
        shared_replace_sel(&mut self.text, &mut self.cursor_pos, &mut self.sel_start, &mut self.sel_end, new);
    }
    fn push_undo(&mut self) {
        self.undo_stack.push((self.text.clone(), self.cursor_pos));
        if self.undo_stack.len() > 30 {
            self.undo_stack.remove(0);
        }
    }
    pub fn new(text: &str) -> Self {
        Self { r: D2D_RECT_F::default(), text: text.to_string(), focused: false, scroll_y: std::cell::Cell::new(0.0), content_h: std::cell::Cell::new(0.0), scroll_hold: std::cell::Cell::new(true), hovered: false, cursor_pos: text.len(), mouse_down: false, sel_start: None, sel_end: 0, dwrite_factory: None, settings_key: None, undo_stack: Vec::new() }
    }
}

impl Widget for MultilineTextInput {
    fn set_bounds(&mut self, r: D2D_RECT_F) { self.r = r; }
    fn bounds(&self) -> D2D_RECT_F { self.r }
    fn text(&self) -> &str { &self.text }
    fn settings_key(&self) -> Option<&str> { self.settings_key.as_deref() }

    fn on_ctrl_key(&mut self, vk: u32) -> bool {
        match vk {
            0x5A => {
                if let Some((prev_text, prev_cursor)) = self.undo_stack.pop() {
                    self.text = prev_text;
                    self.cursor_pos = prev_cursor.min(self.text.len());
                    self.sel_start = None;
                    self.sel_end = 0;
                    self.scroll_hold.set(false);
                }
                return true;
            }
            0x56 | 0x58 => {
                self.push_undo();
            }
            _ => {}
        }
        shared_on_ctrl_key(&mut self.text, &mut self.cursor_pos, &mut self.sel_start, &mut self.sel_end, &self.scroll_hold, vk)
    }

    fn on_mouse_down(&mut self, x: f32, y: f32) {
        self.scroll_hold.set(false);
        if x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom {
            self.mouse_down = true;
            self.sel_start = Some(self.cursor_pos);
            self.sel_end = self.cursor_pos;
        }
    }
    fn on_mouse_up(&mut self, _x: f32, _y: f32) {
        if self.mouse_down {
            self.mouse_down = false;
            self.scroll_hold.set(true);
            if let Some(start) = self.sel_start {
                if start == self.sel_end {
                    self.sel_start = None;
                }
            }
        }
    }
    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.hovered = x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom;
        if self.mouse_down && self.focused {
            let rel_x = x - (self.r.left + 4.0);
            let mut sy = self.scroll_y.get();
            // Edge auto-scroll during drag selection
            let edge = 16.0;
            if y < self.r.top + edge {
                sy = (sy - 4.0).max(0.0);
            } else if y > self.r.bottom - edge {
                let vis_h = self.r.bottom - self.r.top - 4.0;
                sy = (sy + 4.0).min((self.content_h.get() - vis_h).max(0.0));
            }
            self.scroll_y.set(sy);
            let rel_y = y - (self.r.top + 2.0) + sy;
            if let Some(ref dwf) = self.dwrite_factory {
                if let Some(tf) = make_tf(dwf, 14.0) {
                    unsafe { let _ = tf.SetWordWrapping(DWRITE_WORD_WRAPPING_CHARACTER); }
        let box_w = self.r.right - self.r.left - 12.0;
                    let ws: Vec<u16> = self.text.encode_utf16().collect();
                    if let Ok(layout) = unsafe { dwf.CreateTextLayout(&ws, &tf, box_w.max(1.0), 10000.0) } {
                        let mut is_trailing = BOOL::default();
                        let mut is_inside = BOOL::default();
                        let mut hit = DWRITE_HIT_TEST_METRICS::default();
                        let _ = unsafe { layout.HitTestPoint(rel_x.clamp(0.0, box_w), rel_y.max(0.0), &mut is_trailing, &mut is_inside, &mut hit) };
                        let mut utf16_pos = hit.textPosition as usize;
                        if is_trailing.as_bool() { utf16_pos += hit.length as usize; }
                        self.sel_end = utf16_to_byte(&self.text, utf16_pos);
                    }
                }
            }
        }
    }
    fn on_mouse_leave(&mut self) { self.hovered = false; }

    fn on_click(&mut self, x: f32, y: f32) -> bool {
        if !(x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom) { return false; }
        self.focused = true;
        let rel = x - (self.r.left + 4.0);
        let est = 7.0;
        self.cursor_pos = self.text.floor_char_boundary(((rel / est) as usize).min(self.text.len()));
        true
    }

    fn on_click_with(&mut self, x: f32, y: f32, res: &D2DRes) -> bool {
        self.scroll_hold.set(false);
        if !(x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom) { return false; }
        self.dwrite_factory = Some(res.dwrite.clone());
        self.sel_start = None;
        self.focused = true;
        if let Some(tf) = make_tf(&res.dwrite, 14.0) {
            unsafe { let _ = tf.SetWordWrapping(DWRITE_WORD_WRAPPING_CHARACTER); }
            let box_w = self.r.right - self.r.left - 12.0;
            let sy = self.scroll_y.get();
            let rel_x = x - (self.r.left + 4.0);
            let rel_y = y - (self.r.top + 2.0) + sy;
            let ws: Vec<u16> = self.text.encode_utf16().collect();
            if let Ok(layout) = unsafe { res.dwrite.CreateTextLayout(&ws, &tf, box_w.max(1.0), 10000.0) } {
                let mut is_trailing = BOOL::default();
                let mut is_inside = BOOL::default();
                let mut hit = DWRITE_HIT_TEST_METRICS::default();
                let _ = unsafe { layout.HitTestPoint(rel_x.clamp(0.0, box_w), rel_y.max(0.0), &mut is_trailing, &mut is_inside, &mut hit) };
                let mut utf16_pos = hit.textPosition as usize;
                if is_trailing.as_bool() { utf16_pos += hit.length as usize; }
                self.cursor_pos = utf16_to_byte(&self.text, utf16_pos);
            } else {
                self.cursor_pos = self.text.len();
            }
        } else {
            self.cursor_pos = self.text.len();
        }
        true
    }

    fn set_focused(&mut self, val: bool) { self.focused = val; if !val { self.scroll_hold.set(true); self.cursor_pos = 0; self.sel_start = None; self.mouse_down = false; } }
    fn focused(&self) -> bool { self.focused }

    fn on_mouse_wheel(&mut self, delta: f32) -> bool {
        self.scroll_hold.set(true);
        let vis_h = self.r.bottom - self.r.top - 4.0;
        if self.text.is_empty() { return true; }
        let content_h = self.content_h.get();
        let max_s = if content_h > 0.0 { (content_h - vis_h).max(0.0) } else { std::f32::MAX };
        let s = (self.scroll_y.get() - delta).clamp(0.0, max_s);
        self.scroll_y.set(s);
        true
    }
    fn on_key_down(&mut self, vk: u32) -> bool {
        self.scroll_hold.set(false);
        match vk {
            0x08 | 0x2E => self.push_undo(),
            _ => {}
        }
        if shared_on_key_down(&mut self.text, &mut self.cursor_pos, &mut self.sel_start, &mut self.sel_end, vk) { return true; }
        match vk {
            0x26 => {
                self.sel_start = None;
                if let Some(ref dwf) = self.dwrite_factory {
                    if let Some(tf) = make_tf(dwf, 14.0) {
                        let box_w = (self.r.right - self.r.left - 12.0).max(1.0);
                        let ws: Vec<u16> = self.text.encode_utf16().collect();
                        if let Ok(layout) = unsafe { dwf.CreateTextLayout(&ws, &tf, box_w, 10000.0) } {
                            let u16_cursor = byte_to_utf16(&self.text, self.cursor_pos) as u32;
                            let mut px = 0.0f32; let mut py = 0.0f32;
                            let mut hit = DWRITE_HIT_TEST_METRICS::default();
                            let _ = unsafe { layout.HitTestTextPosition(u16_cursor, false, &mut px, &mut py, &mut hit) };
                            let mut is_trailing = BOOL::default();
                            let mut is_inside = BOOL::default();
                            let mut new_hit = DWRITE_HIT_TEST_METRICS::default();
                            let _ = unsafe { layout.HitTestPoint(px, (py - 1.0).max(0.0), &mut is_trailing, &mut is_inside, &mut new_hit) };
                            let mut utf16_pos = new_hit.textPosition as usize;
                            if is_trailing.as_bool() { utf16_pos += new_hit.length as usize; }
                            self.cursor_pos = utf16_to_byte(&self.text, utf16_pos);
                        }
                    }
                } else {
                    let s = (self.scroll_y.get() - 20.0).max(0.0);
                    self.scroll_y.set(s);
                }
                true
            }
            0x28 => {
                self.sel_start = None;
                if let Some(ref dwf) = self.dwrite_factory {
                    if let Some(tf) = make_tf(dwf, 14.0) {
                        let box_w = (self.r.right - self.r.left - 12.0).max(1.0);
                        let ws: Vec<u16> = self.text.encode_utf16().collect();
                        if let Ok(layout) = unsafe { dwf.CreateTextLayout(&ws, &tf, box_w, 10000.0) } {
                            let u16_cursor = byte_to_utf16(&self.text, self.cursor_pos) as u32;
                            let mut px = 0.0f32; let mut py = 0.0f32;
                            let mut hit = DWRITE_HIT_TEST_METRICS::default();
                            let _ = unsafe { layout.HitTestTextPosition(u16_cursor, false, &mut px, &mut py, &mut hit) };
                            let mut is_trailing = BOOL::default();
                            let mut is_inside = BOOL::default();
                            let mut new_hit = DWRITE_HIT_TEST_METRICS::default();
                            let _ = unsafe { layout.HitTestPoint(px, py + hit.height + 1.0, &mut is_trailing, &mut is_inside, &mut new_hit) };
                            let mut utf16_pos = new_hit.textPosition as usize;
                            if is_trailing.as_bool() { utf16_pos += new_hit.length as usize; }
                            self.cursor_pos = utf16_to_byte(&self.text, utf16_pos);
                        }
                    }
                } else {
                    let vis_h = self.r.bottom - self.r.top - 4.0;
                    let ch = self.text.len() as f32 * 10.0;
                    let s = (self.scroll_y.get() + 20.0).min((ch - vis_h).max(0.0));
                    self.scroll_y.set(s);
                }
                true
            }
            _ => false,
        }
    }

    fn on_char(&mut self, ch: u32) -> bool {
        self.scroll_hold.set(false);
        if let Some(c) = char::from_u32(ch) {
            if c == '\r' {
                self.push_undo();
                if self.sel_start.is_some() { self.replace_sel("\n"); }
                else { self.text.insert(self.cursor_pos, '\n'); self.cursor_pos += 1; }
                return true;
            }
            if !c.is_control() {
                self.push_undo();
                if self.sel_start.is_some() { self.replace_sel(&c.to_string()); }
                else { self.text.insert(self.cursor_pos, c); self.cursor_pos += c.len_utf8(); }
                return true;
            }
        }
        false
    }

    fn draw(&self, res: &D2DRes) {
        let inp_rr = D2D1_ROUNDED_RECT { rect: self.r, radiusX: 6.0, radiusY: 6.0 };
        let bg = if self.focused { T.bg_input } else if self.hovered { T.bg_hover } else { T.bg_separator };
        if let Some(b) = brush(&res.d2d, bg, 1.0) {
            unsafe { res.d2d.FillRoundedRectangle(&inp_rr as *const _, &b); }
        }
        let c = if self.focused { T.accent } else if self.hovered { T.border_focused } else { T.border };
        if let Some(b) = brush(&res.d2d, c, 1.0) {
            unsafe { let _ = res.d2d.DrawRoundedRectangle(&inp_rr as *const _, &b, 1.0, None as Option<&ID2D1StrokeStyle>); }
        }

        let box_l = self.r.left + 4.0;
        let box_t = self.r.top + 2.0;
        let box_w = self.r.right - self.r.left - 12.0;
        let vis_h = self.r.bottom - self.r.top - 4.0;

        if let Some(tf) = make_tf(&res.dwrite, 14.0) {
            unsafe { let _ = tf.SetWordWrapping(DWRITE_WORD_WRAPPING_CHARACTER); }
            let ws: Vec<u16> = self.text.encode_utf16().collect();
            if let Ok(layout) = unsafe { res.dwrite.CreateTextLayout(&ws, &tf, box_w.max(1.0), 10000.0) } {
                let mut m = DWRITE_TEXT_METRICS::default();
                let _ = unsafe { layout.GetMetrics(&mut m) };
                let content_h = m.height;

                let s = self.scroll_y.get().clamp(0.0, (content_h - vis_h).max(0.0));
                self.scroll_y.set(s);
                self.content_h.set(content_h);

                let clip_r = D2D_RECT_F { left: self.r.left + 2.0, top: self.r.top, right: self.r.right - 8.0, bottom: self.r.bottom };
                unsafe { res.d2d.PushAxisAlignedClip(&clip_r as *const _, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE); }

                let sy = self.scroll_y.get();
                let origin = Vector2 { X: box_l, Y: box_t - sy };
                if let Some(b) = brush(&res.d2d, T.text_bright, 1.0) {
                    unsafe { res.d2d.DrawTextLayout(origin, &layout, &b, D2D1_DRAW_TEXT_OPTIONS(0)); }
                }

                // Draw selection highlight via DirectWrite HitTestTextRange
                if let Some((lo, hi)) = self.sel_range() {
                    if lo != hi {
                        let lo_u16 = byte_to_utf16(&self.text, lo) as u32;
                        let hi_u16 = byte_to_utf16(&self.text, hi) as u32;
                        let len_u16 = hi_u16 - lo_u16;
                        let mut metrics = vec![DWRITE_HIT_TEST_METRICS::default(); 256];
                        let mut actual = 0u32;
                        let _ = unsafe { layout.HitTestTextRange(lo_u16, len_u16, 0.0, 0.0, Some(metrics.as_mut_slice()), &mut actual) };
                        if let Some(b) = brush(&res.d2d, T.accent, 0.30) {
                            for i in 0..actual as usize {
                                let m = &metrics[i];
                                let sel_l = box_l + m.left;
                                let sel_t = box_t - sy + m.top;
                                let sel_r = sel_l + m.width;
                                let sel_b = sel_t + m.height;
                                if sel_r > sel_l {
                                    unsafe { res.d2d.FillRectangle(&D2D_RECT_F { left: sel_l, top: sel_t, right: sel_r, bottom: sel_b } as *const _, &b); }
                                }
                            }
                        }
                    }
                }

                if self.focused {
                    let mut u16_count = 0u32;
                    for c in self.text[..self.cursor_pos].chars() {
                        let mut buf = [0u16; 2];
                        let encoded = c.encode_utf16(&mut buf);
                        u16_count += encoded.len() as u32;
                    }
                    let mut px = 0.0f32; let mut py = 0.0f32;
                    let mut hit = DWRITE_HIT_TEST_METRICS::default();
                    let _ = unsafe { layout.HitTestTextPosition(u16_count, false, &mut px, &mut py, &mut hit) };
                    let cx = box_l + px;
                    let cy = box_t - sy + py;
                    if let Some(b) = brush(&res.d2d, T.placeholder, 1.0) {
                        unsafe { res.d2d.FillRectangle(&D2D_RECT_F { left: cx, top: cy, right: cx + 1.5, bottom: cy + hit.height } as *const _, &b); }
                    }
                    // Auto-scroll to keep cursor/selection visible
                    if !self.scroll_hold.get() || self.mouse_down {
                        let (tpy, th) = if self.mouse_down {
                            let far_u16 = byte_to_utf16(&self.text, self.sel_end) as u32;
                            let mut tpx = 0.0f32; let mut tpy2 = 0.0f32;
                            let mut thit = DWRITE_HIT_TEST_METRICS::default();
                            let _ = unsafe { layout.HitTestTextPosition(far_u16, false, &mut tpx, &mut tpy2, &mut thit) };
                            (tpy2, thit.height)
                        } else {
                            (py, hit.height)
                        };
                        let margin = 12.0;
                        if tpy < sy + margin {
                            self.scroll_y.set((tpy - margin).max(0.0));
                        } else if tpy + th > sy + vis_h - margin {
                            self.scroll_y.set((tpy + th + margin - vis_h).max(0.0));
                        }
                        let s = self.scroll_y.get().clamp(0.0, (content_h - vis_h).max(0.0));
                        self.scroll_y.set(s);
                    }
                }

                unsafe { res.d2d.PopAxisAlignedClip(); }

                // scroll bar
                if content_h > vis_h {
                    let sb_l = self.r.right - 6.0;
                    let sb_r = self.r.right - 2.0;
                    let sb_t = self.r.top + 2.0;
                    let sb_h = self.r.bottom - self.r.top - 4.0;
                    if let Some(b) = brush(&res.d2d, T.bg_widget, 1.0) {
                        unsafe { res.d2d.FillRectangle(&D2D_RECT_F { left: sb_l, top: sb_t, right: sb_r, bottom: sb_t + sb_h } as *const _, &b); }
                    }
                    let thumb_h = (vis_h / content_h) * sb_h;
                    let sy2 = self.scroll_y.get();
                    let thumb_t = sb_t + (sy2 / (content_h - vis_h)) * (sb_h - thumb_h);
                    if let Some(b) = brush(&res.d2d, T.text_secondary, 1.0) {
                        unsafe { res.d2d.FillRectangle(&D2D_RECT_F { left: sb_l, top: thumb_t, right: sb_r, bottom: thumb_t + thumb_h } as *const _, &b); }
                    }
                }
            }
        }
    }
}

// ── Dropdown ──

fn tf_center_nowrap(dwrite: &IDWriteFactory, sz: f32) -> Option<IDWriteTextFormat> {
    let tf = make_tf(dwrite, sz)?;
    unsafe {
        let _ = tf.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
        let _ = tf.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        let _ = tf.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
    }
    Some(tf)
}

fn tf_vcenter_nowrap(dwrite: &IDWriteFactory, sz: f32) -> Option<IDWriteTextFormat> {
    let tf = make_tf(dwrite, sz)?;
    unsafe {
        let _ = tf.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        let _ = tf.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
    }
    Some(tf)
}

pub struct Dropdown {
    r: D2D_RECT_F,
    pub options: Vec<String>,
    pub selected: usize,
    expanded: bool,
    hovered: bool,
    pub focused: bool,
    hovered_idx: usize,
    popup_item_h: f32,
    popup_max_visible: usize,
    popup_w: f32,
    pub settings_key: Option<String>,
}

impl Dropdown {
    pub fn new(options: &[String], current: &str) -> Self {
        let sel = options.iter().position(|o| o == current).unwrap_or(0);
        Self {
            r: D2D_RECT_F::default(),
            options: options.to_vec(),
            selected: sel,
            expanded: false,
            hovered: false,
            focused: false,
            hovered_idx: 0,
            popup_item_h: 32.0,
            popup_max_visible: 6,
            popup_w: 0.0,
            settings_key: None,
        }
    }

    pub fn set_options(&mut self, options: Vec<String>) {
        if options.is_empty() { return; }
        let cur = self.options.get(self.selected).cloned();
        self.options = options;
        self.selected = self.options.iter().position(|o| Some(o) == cur.as_ref()).unwrap_or(0);
        if self.selected >= self.options.len() { self.selected = 0; }
        self.popup_w = 0.0;
    }

    fn calc_popup_w(&mut self, res: &D2DRes) {
        let base_w = self.r.right - self.r.left;
        let mut max_w = base_w;
        if let Some(tf) = make_tf(&res.dwrite, 14.0) {
            for opt in &self.options {
                let tw = text_width(&res.dwrite, &tf, opt);
                let needed = tw + 48.0;
                if needed > max_w { max_w = needed; }
            }
        }
        self.popup_w = max_w;
    }

    fn popup_left(&self) -> f32 { self.r.left }
    fn popup_right(&self) -> f32 { if self.popup_w > 0.0 { self.r.left + self.popup_w } else { self.r.right } }
    fn popup_top(&self) -> f32 { self.r.bottom + 4.0 }
    fn popup_bottom(&self) -> f32 {
        let vis = self.options.len().min(self.popup_max_visible);
        self.popup_top() + vis as f32 * self.popup_item_h + 8.0
    }
}

impl Widget for Dropdown {
    fn set_bounds(&mut self, r: D2D_RECT_F) { self.r = r; }
    fn bounds(&self) -> D2D_RECT_F { self.r }
    fn text(&self) -> &str {
        self.options.get(self.selected).map(|s| s.as_str()).unwrap_or("")
    }
    fn settings_key(&self) -> Option<&str> { self.settings_key.as_deref() }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.hovered = x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom;
        if self.expanded {
            let pr = self.popup_left();
            let pl = self.popup_right();
            if x >= pr && x <= pl && y >= self.popup_top() && y <= self.popup_bottom() {
                let idx = ((y - self.popup_top() - 4.0) / self.popup_item_h) as usize;
                self.hovered_idx = idx.min(self.options.len().saturating_sub(1));
            }
        }
    }

    fn on_mouse_leave(&mut self) { self.hovered = false; }

    fn on_click_with(&mut self, x: f32, y: f32, res: &D2DRes) -> bool {
        if x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom {
            self.expanded = !self.expanded;
            self.focused = self.expanded;
            if self.expanded {
                self.calc_popup_w(res);
                self.hovered_idx = self.selected;
            }
            return true;
        }
        if self.expanded {
            let pr = self.popup_left();
            let pl = self.popup_right();
            if x >= pr && x <= pl && y >= self.popup_top() && y <= self.popup_bottom() {
                let idx = ((y - self.popup_top() - 4.0) / self.popup_item_h) as usize;
                if idx < self.options.len() {
                    self.selected = idx;
                }
                self.expanded = false;
                self.focused = false;
                return true;
            }
        }
        false
    }

    fn set_focused(&mut self, val: bool) {
        if !val {
            self.expanded = false;
            self.focused = false;
        }
    }

    fn focused(&self) -> bool { self.focused || self.expanded }

    fn set_text(&mut self, text: &str) {
        if let Some(idx) = self.options.iter().position(|o| o == text) {
            self.selected = idx;
        }
    }

    fn draw(&self, res: &D2DRes) {
        let inp_rr = D2D1_ROUNDED_RECT { rect: self.r, radiusX: 6.0, radiusY: 6.0 };
        let bg = if self.focused { T.bg_input } else if self.hovered { T.bg_hover } else { T.bg_separator };
        if let Some(b) = brush(&res.d2d, bg, 1.0) {
            unsafe { res.d2d.FillRoundedRectangle(&inp_rr as *const _, &b); }
        }
        let c = if self.focused { T.accent } else if self.hovered { T.border_focused } else { T.border };
        if let Some(b) = brush(&res.d2d, c, 1.0) {
            unsafe { let _ = res.d2d.DrawRoundedRectangle(&inp_rr as *const _, &b, 1.0, None as Option<&ID2D1StrokeStyle>); }
        }

        let display = if self.options.is_empty() { self.options.get(self.selected).map(|s| s.as_str()).unwrap_or("") } else { self.options[self.selected].as_str() };
        let arrow = if self.expanded { "▲" } else { "▼" };

        let text_r = D2D_RECT_F { left: self.r.left + 8.0, top: self.r.top, right: self.r.right - 28.0, bottom: self.r.bottom };
        let clip_r = D2D_RECT_F { left: self.r.left + 6.0, top: self.r.top, right: self.r.right - 30.0, bottom: self.r.bottom };
        unsafe { res.d2d.PushAxisAlignedClip(&clip_r as *const _, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE); }

        if let Some(tf) = tf_vcenter_nowrap(&res.dwrite, 14.0) {
            if let Some(b) = brush(&res.d2d, T.text_bright, 1.0) {
                draw_text(&res.d2d, display, &tf, &text_r, &b);
            }
        }
        unsafe { res.d2d.PopAxisAlignedClip(); }

        let arr_r = D2D_RECT_F { left: self.r.right - 24.0, top: self.r.top, right: self.r.right - 6.0, bottom: self.r.bottom };
        if let Some(tf) = tf_vcenter(&res.dwrite, 12.0) {
            if let Some(b) = brush(&res.d2d, T.accent, 1.0) {
                draw_text(&res.d2d, arrow, &tf, &arr_r, &b);
            }
        }
    }

    fn draw_overlay(&self, res: &D2DRes) {
        if !self.expanded || self.options.is_empty() { return; }

        let pr = self.popup_left();
        let pl = self.popup_right();
        let pt = self.popup_top();
        let pb = self.popup_bottom();
        let popup_r = D2D_RECT_F { left: pr, top: pt, right: pl, bottom: pb };
        let rr = D2D1_ROUNDED_RECT { rect: popup_r, radiusX: 6.0, radiusY: 6.0 };

        if let Some(b) = brush(&res.d2d, T.bg_raised, 1.0) {
            unsafe { res.d2d.FillRoundedRectangle(&rr as *const _, &b); }
        }
        if let Some(b) = brush(&res.d2d, T.bg_input, 1.0) {
            unsafe { let _ = res.d2d.DrawRoundedRectangle(&rr as *const _, &b, 1.0, None as Option<&ID2D1StrokeStyle>); }
        }

        let vis_count = self.options.len().min(self.popup_max_visible);
        for i in 0..vis_count {
            let item_y = pt + 4.0 + i as f32 * self.popup_item_h;
            let item_r = D2D_RECT_F { left: pr + 2.0, top: item_y, right: pl - 2.0, bottom: item_y + self.popup_item_h };
            if i == self.hovered_idx && self.hovered {
                if let Some(b) = brush(&res.d2d, T.bg_hover, 1.0) {
                    let irr = D2D1_ROUNDED_RECT { rect: item_r, radiusX: 4.0, radiusY: 4.0 };
                    unsafe { res.d2d.FillRoundedRectangle(&irr as *const _, &b); }
                }
            }

            let text_item_r = D2D_RECT_F { left: item_r.left + 8.0, top: item_r.top, right: item_r.right - 24.0, bottom: item_r.bottom };
            if let Some(tf) = tf_vcenter(&res.dwrite, 14.0) {
                if let Some(b) = brush(&res.d2d, T.tab_hover_bg, 1.0) {
                    draw_text(&res.d2d, &self.options[i], &tf, &text_item_r, &b);
                }
            }

            if i == self.selected {
                let check_r = D2D_RECT_F { left: item_r.right - 20.0, top: item_r.top, right: item_r.right - 4.0, bottom: item_r.bottom };
                if let Some(tf) = tf_vcenter(&res.dwrite, 13.0) {
                    if let Some(b) = brush(&res.d2d, T.accent, 1.0) {
                        draw_text(&res.d2d, "✓", &tf, &check_r, &b);
                    }
                }
            }
        }
    }
}

// ── KeyBindingInput ──

pub struct KeyBindingInput {
    r: D2D_RECT_F,
    pub text: String,
    focused: bool,
    hovered: bool,
    pub settings_key: Option<String>,
}

impl KeyBindingInput {
    pub fn new(text: &str) -> Self { Self { r: D2D_RECT_F::default(), text: text.to_string(), focused: false, hovered: false, settings_key: None } }
}

impl Widget for KeyBindingInput {
    fn set_bounds(&mut self, r: D2D_RECT_F) { self.r = r; }
    fn bounds(&self) -> D2D_RECT_F { self.r }
    fn text(&self) -> &str { &self.text }
    fn settings_key(&self) -> Option<&str> { self.settings_key.as_deref() }
    fn on_mouse_move(&mut self, x: f32, y: f32) { self.hovered = x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom; }
    fn on_mouse_leave(&mut self) { self.hovered = false; }

    fn on_click(&mut self, x: f32, y: f32) -> bool {
        if !(x >= self.r.left && x <= self.r.right && y >= self.r.top && y <= self.r.bottom) { return false; }
        self.focused = true;
        true
    }

    fn set_focused(&mut self, val: bool) { self.focused = val; }
    fn focused(&self) -> bool { self.focused }
    fn set_text(&mut self, text: &str) { self.text = text.to_string(); }
    fn captures_hotkey(&self) -> bool { true }

    fn draw(&self, res: &D2DRes) {
        let bg = if self.focused { T.bg_input } else if self.hovered { T.bg_hover } else { T.bg_separator };
        if let Some(b) = brush(&res.d2d, bg, 1.0) {
            let rr = D2D1_ROUNDED_RECT { rect: self.r, radiusX: 6.0, radiusY: 6.0 };
            unsafe { res.d2d.FillRoundedRectangle(&rr as *const _, &b); }
        }
        let lc = if self.focused { T.accent } else if self.hovered { T.border_focused } else { T.border };
        if let Some(b) = brush(&res.d2d, lc, 1.0) {
            let rr = D2D1_ROUNDED_RECT { rect: self.r, radiusX: 6.0, radiusY: 6.0 };
            unsafe { let _ = res.d2d.DrawRoundedRectangle(&rr as *const _, &b, 1.0, None as Option<&ID2D1StrokeStyle>); }
        }

        let display = if self.focused { "按下快捷键...".to_string() } else { self.text.clone() };
        let text_r = D2D_RECT_F { left: self.r.left + 8.0, top: self.r.top, right: self.r.right - 30.0, bottom: self.r.bottom };
        let clip_r = D2D_RECT_F { left: self.r.left + 6.0, top: self.r.top, right: self.r.right - 32.0, bottom: self.r.bottom };
        unsafe { res.d2d.PushAxisAlignedClip(&clip_r as *const _, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE); }

        let tc = if self.focused { T.text_disabled } else { T.text_bright };
        if let Some(tf) = tf_center(&res.dwrite, 14.0) {
            if let Some(b) = brush(&res.d2d, tc, 1.0) {
                draw_text(&res.d2d, &display, &tf, &text_r, &b);
            }
        }
        unsafe { res.d2d.PopAxisAlignedClip(); }

        let hint_r = D2D_RECT_F { left: self.r.right - 26.0, top: self.r.top, right: self.r.right - 6.0, bottom: self.r.bottom };
        if let Some(tf) = tf_vcenter(&res.dwrite, 13.0) {
            let accent_r = if self.focused { T.accent.0 } else { T.text_dim.0 };
            if let Some(b) = brush(&res.d2d, (accent_r, T.accent.1, T.accent.2), 1.0) {
                draw_text(&res.d2d, "🖊", &tf, &hint_r, &b);
            }
        }
    }
}

// ── 剪贴板辅助 ────────────────────────────────────────────────

#[link(name = "user32")]
extern "system" {
    fn OpenClipboard(hWnd: HWND) -> BOOL;
    fn CloseClipboard() -> BOOL;
    fn EmptyClipboard() -> BOOL;
    fn SetClipboardData(uFormat: u32, hMem: HANDLE) -> HANDLE;
    fn GetClipboardData(uFormat: u32) -> HANDLE;
}

#[link(name = "kernel32")]
extern "system" {
    fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> HANDLE;
    fn GlobalLock(hMem: HANDLE) -> *mut std::ffi::c_void;
    fn GlobalUnlock(hMem: HANDLE) -> BOOL;
    fn GlobalFree(hMem: HANDLE) -> HANDLE;
}

const CF_UNICODETEXT: u32 = 13;
const GMEM_MOVEABLE: u32 = 0x0002;

pub fn clipboard_copy(text: &str) -> bool {
    unsafe {
        if OpenClipboard(HWND::default()) == BOOL(0) { return false; }
        let _ = EmptyClipboard();
        let ws: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        let bytes = ws.len() * 2;
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if h == HANDLE::default() { let _ = CloseClipboard(); return false; }
        let lock = GlobalLock(h);
        if lock.is_null() { let _ = GlobalFree(h); let _ = CloseClipboard(); return false; }
        std::ptr::copy_nonoverlapping(ws.as_ptr() as *const u8, lock as *mut u8, bytes);
        let _ = GlobalUnlock(h);
        let _ = SetClipboardData(CF_UNICODETEXT, h);
        let _ = CloseClipboard();
        true
    }
}

pub fn clipboard_paste() -> Option<String> {
    unsafe {
        if OpenClipboard(HWND::default()) == BOOL(0) { return None; }
        let h = GetClipboardData(CF_UNICODETEXT);
        if h == HANDLE::default() { let _ = CloseClipboard(); return None; }
        let lock = GlobalLock(h);
        if lock.is_null() { let _ = CloseClipboard(); return None; }
        let mut len = 0;
        while *((lock as *const u16).add(len)) != 0 { len += 1; }
        let slice = std::slice::from_raw_parts(lock as *const u16, len);
        let s = String::from_utf16_lossy(slice);
        let _ = GlobalUnlock(h);
        let _ = CloseClipboard();
        Some(s)
    }
}
