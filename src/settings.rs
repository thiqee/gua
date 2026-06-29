// Gua — 设置面板（鸿蒙设计风格）

use std::collections::HashMap;
use std::ptr;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::config;
use crate::state::*;
use crate::widget::*;

#[link(name = "user32")]
extern "system" {
    fn GetAsyncKeyState(vKey: i32) -> i16;
}

#[link(name = "user32")]
extern "system" {
    fn SetCapture(hWnd: HWND) -> HWND;
    fn ReleaseCapture() -> BOOL;
    fn ImmGetContext(hwnd: HWND) -> isize;
    fn ImmSetCompositionWindow(himc: isize, lpCompForm: *const COMPOSITIONFORM) -> BOOL;
    fn ImmGetCompositionStringW(himc: isize, dwIndex: u32, lpBuf: *mut std::ffi::c_void, dwBufLen: u32) -> u32;
    fn ImmReleaseContext(hwnd: HWND, himc: isize) -> BOOL;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateRoundRectRgn(x1: i32, y1: i32, x2: i32, y2: i32, w: i32, h: i32) -> HRGN;
    fn SetWindowRgn(h: HWND, hRgn: HRGN, bRedraw: BOOL) -> i32;
}

const S_W: i32 = 780;
const S_H: i32 = 640;
const TITLE_H: f32 = 30.0;
const BOTTOM_H: f32 = 52.0;
const SIDEBAR_W: f32 = 140.0;
const CONTENT_L: f32 = 140.0;
const CONTENT_PAD: f32 = 24.0;
const ACCENT: (f32, f32, f32) = (0.29, 0.53, 0.80);

#[allow(dead_code)]
pub struct SettingsWin {
    pub hwnd: HWND,
    swap_chain: IDXGISwapChain1,
    d2d_context: ID2D1DeviceContext,
    target: Option<ID2D1Bitmap1>,
    widgets: Vec<Box<dyn Widget>>,
    cards: Vec<D2D_RECT_F>,
    cat: usize,
    sel_cat: usize,
    scroll_y: f32,
    content_h: f32,
    focused_idx: Option<usize>,
    capturing_hotkey: bool,
    mod_held: [bool; 4],
    close_hovered: bool,
    save_hovered: bool,
    // 识别码 tab state
    codes_search: String,
    codes_version: usize,
    cat_expanded: Vec<bool>,
    scroll_dragging: bool,
    scroll_drag_start_y: f32,
    composing: String,
}

#[allow(static_mut_refs)]
static mut SETTINGS: Option<SettingsWin> = None;

unsafe fn main_state() -> *mut AppState {
    let hwnd = HWND(MAIN_HWND as *mut std::ffi::c_void);
    if hwnd.0.is_null() { return ptr::null_mut(); }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    ptr as *mut AppState
}

fn card_hdr(name: &str, w: &mut Vec<Box<dyn Widget>>, y: &mut f32, cx: f32, card_r: f32) {
    w.push(Box::new(GroupHeader::new(name)));
    w.last_mut().unwrap().set_bounds(D2D_RECT_F { left: cx, top: *y, right: card_r, bottom: *y + 28.0 });
    *y += 36.0;
}

fn card_bg(cards: &mut Vec<D2D_RECT_F>, y: &mut f32, card_l: f32, card_r: f32, ct: f32) {
    cards.push(D2D_RECT_F { left: card_l, top: ct, right: card_r, bottom: *y });
}

fn build_widgets(cat: usize, cards: &mut Vec<D2D_RECT_F>, content_h: &mut f32) -> Vec<Box<dyn Widget>> {
    cards.clear();
    let mut w: Vec<Box<dyn Widget>> = Vec::new();

    let cx = CONTENT_L + CONTENT_PAD;
    let cw = (S_W as f32) - CONTENT_L - CONTENT_PAD - CONTENT_PAD;
    let card_l = cx;
    let card_r = cx + cw;
    let inner_l = cx + 14.0;
    let inner_w = cw - 28.0;
    let mut y = TITLE_H + CONTENT_PAD;

    let inp_l = 96.0;
    let inp_w = 152.0;
    let wide_l = 116.0;
    let tog_l = inner_l + inner_w - 48.0;

    match cat {
        0 => {
            // ── 快捷键 ──
            card_hdr("快捷键", &mut w, &mut y, cx, card_r);
            let ct = y;
            let mut kb = KeyBindingInput::new("Alt+Space");
            kb.set_bounds(D2D_RECT_F { left: inner_l, top: y + 10.0, right: inner_l + inner_w, bottom: y + 44.0 });
            w.push(Box::new(kb));
            y += 56.0;
            card_bg(cards, &mut y, card_l, card_r, ct);
            y += 12.0;

            // ── 体验设置 ──
            card_hdr("体验设置", &mut w, &mut y, cx, card_r);
            let ct = y;
            for (i, (label, checked)) in [("失去焦点自动隐藏", true), ("模糊匹配", true), ("拼音搜索", true)].iter().enumerate() {
                let row_y = y + 10.0 + i as f32 * 38.0;
                let mut lbl = Label::new(label);
                lbl.set_bounds(D2D_RECT_F { left: inner_l, top: row_y, right: inner_l + inner_w - 56.0, bottom: row_y + 28.0 });
                w.push(Box::new(lbl));
                let mut sw = ToggleSwitch::new(*checked);
                sw.set_bounds(D2D_RECT_F { left: tog_l, top: row_y, right: tog_l + 48.0, bottom: row_y + 28.0 });
                w.push(Box::new(sw));
            }
            y += 10.0 + 3.0 * 38.0 + 12.0;
            card_bg(cards, &mut y, card_l, card_r, ct);
            y += 12.0;

            // ── 黑名单程序 ──
            card_hdr("黑名单程序", &mut w, &mut y, cx, card_r);
            let ct = y;
            let mut bl_inp = MultilineTextInput::new("notepad.exe, calc.exe");
            bl_inp.set_bounds(D2D_RECT_F { left: inner_l, top: y + 10.0, right: inner_l + inner_w, bottom: y + 110.0 });
            w.push(Box::new(bl_inp));
            y += 10.0 + 100.0 + 12.0;
            card_bg(cards, &mut y, card_l, card_r, ct);
            y += 12.0;

            // ── 多音字追加读音 ──
            card_hdr("多音字追加读音", &mut w, &mut y, cx, card_r);
            let ct = y;
            let mut py_inp = MultilineTextInput::new("茄=qie, 了=le");
            py_inp.set_bounds(D2D_RECT_F { left: inner_l, top: y + 10.0, right: inner_l + inner_w, bottom: y + 110.0 });
            w.push(Box::new(py_inp));
            y += 10.0 + 100.0 + 12.0;
            card_bg(cards, &mut y, card_l, card_r, ct);
            y += 12.0;
        }

        1 => {
            // ── 颜色 ──
            card_hdr("颜色", &mut w, &mut y, cx, card_r);
            let ct = y;
            for (i, (label, val)) in [("背景色", "#1E1E1E"), ("输入框色", "#2A2A2A"), ("高亮色", "#4A6FA5"), ("文字色", "#CCCCCC")].iter().enumerate() {
                let row_y = y + 10.0 + i as f32 * 38.0;
                let mut lbl = Label::new(label);
                lbl.set_bounds(D2D_RECT_F { left: inner_l, top: row_y, right: inner_l + inp_l - 8.0, bottom: row_y + 28.0 });
                w.push(Box::new(lbl));
                let mut inp = TextInput::new(val);
                inp.center = true;
                inp.set_bounds(D2D_RECT_F { left: inner_l + inp_l, top: row_y, right: inner_l + inp_l + inp_w, bottom: row_y + 28.0 });
                w.push(Box::new(inp));
            }
            y += 10.0 + 4.0 * 38.0 + 12.0;
            card_bg(cards, &mut y, card_l, card_r, ct);
            y += 12.0;

            // ── 字体 ──
            card_hdr("字体", &mut w, &mut y, cx, card_r);
            let ct = y;
            let font_names = scan_font_families();
            let s_main = unsafe { main_state() };
            let current_font = if !s_main.is_null() { unsafe { (*s_main).font_name.clone() } } else { "Segoe UI".to_string() };
            let font_options = if font_names.is_empty() {
                vec![current_font.clone()]
            } else {
                let mut opts = font_names;
                if !opts.contains(&current_font) { opts.insert(0, current_font.clone()); }
                opts
            };
            let row_y0 = y + 10.0;
            let mut lbl0 = Label::new("字体选择");
            lbl0.set_bounds(D2D_RECT_F { left: inner_l, top: row_y0, right: inner_l + inp_l - 8.0, bottom: row_y0 + 28.0 });
            w.push(Box::new(lbl0));
            let mut dd = Dropdown::new(&font_options, &current_font);
            dd.set_bounds(D2D_RECT_F { left: inner_l + inp_l, top: row_y0, right: inner_l + inp_l + inp_w, bottom: row_y0 + 28.0 });
            w.push(Box::new(dd));
            for (i, (label, val)) in [("字号", "18"), ("状态栏字号", "12")].iter().enumerate() {
                let row_y = y + 10.0 + (i + 1) as f32 * 38.0;
                let mut lbl = Label::new(label);
                lbl.set_bounds(D2D_RECT_F { left: inner_l, top: row_y, right: inner_l + inp_l - 8.0, bottom: row_y + 28.0 });
                w.push(Box::new(lbl));
                let mut inp = TextInput::new(val);
                inp.center = true;
                inp.set_bounds(D2D_RECT_F { left: inner_l + inp_l, top: row_y, right: inner_l + inp_l + inp_w, bottom: row_y + 28.0 });
                w.push(Box::new(inp));
            }
            y += 10.0 + 3.0 * 38.0 + 12.0;
            card_bg(cards, &mut y, card_l, card_r, ct);
            y += 12.0;

            // ── 布局 ──
            card_hdr("布局", &mut w, &mut y, cx, card_r);
            let ct = y;
            for (i, (label, val)) in [("透明度", "255"), ("圆角大小", "12"), ("水平位置 (%)", "50"), ("垂直位置 (%)", "40"), ("宽度", "500"), ("最大显示限制", "8")].iter().enumerate() {
                let row_y = y + 10.0 + i as f32 * 38.0;
                let mut lbl = Label::new(label);
                lbl.set_bounds(D2D_RECT_F { left: inner_l, top: row_y, right: inner_l + inp_l - 8.0, bottom: row_y + 28.0 });
                w.push(Box::new(lbl));
                let mut inp = TextInput::new(val);
                inp.center = true;
                inp.set_bounds(D2D_RECT_F { left: inner_l + inp_l, top: row_y, right: inner_l + inp_l + inp_w, bottom: row_y + 28.0 });
                w.push(Box::new(inp));
            }
            y += 10.0 + 6.0 * 38.0 + 12.0;
            card_bg(cards, &mut y, card_l, card_r, ct);
            y += 12.0;
        }

        _ => {
            // 识别码 tab is built separately via build_codes_tab
            // Return empty - will be rebuilt in WM_PAINT
            *content_h = y;
            return w;
        }
    }

    *content_h = y;
    w
}

unsafe fn sync_codes_entries(s: &SettingsWin) {
    let s_main = main_state();
    if s_main.is_null() || s.cat != 2 { return; }
    let state = &mut *s_main;
    for wi in 0..s.widgets.len() {
        if let WidgetCmd::EntryDel(gi) = s.widgets[wi].cmd() {
            if wi >= 3 && gi < state.entries.len() {
                let key = s.widgets[wi - 3].text().to_string();
                let val = s.widgets[wi - 2].text().to_string();
                let desc = s.widgets[wi - 1].text().to_string();
                state.entries[gi].key = key;
                state.entries[gi].value = val;
                state.entries[gi].description = if desc.is_empty() { None } else { Some(desc) };
            }
        }
    }
}

unsafe fn build_codes_tab(
    cards: &mut Vec<D2D_RECT_F>,
    content_h: &mut f32,
    search: &str,
    cat_expanded: &mut Vec<bool>,
) -> Vec<Box<dyn Widget>> {
    let cx = CONTENT_L + CONTENT_PAD;
    let cw = (S_W as f32) - CONTENT_L - CONTENT_PAD - CONTENT_PAD;
    let card_l = cx;
    let card_r = cx + cw;
    let inner_l = cx + 14.0;
    let inner_w = cw - 28.0;

    cards.clear();
    let mut w: Vec<Box<dyn Widget>> = Vec::new();
    let mut y = TITLE_H + CONTENT_PAD;

    // Search box + expand/collapse all buttons
    let mut lbl_search = Label::new("搜索：");
    lbl_search.set_bounds(D2D_RECT_F { left: inner_l, top: y, right: inner_l + 50.0, bottom: y + 28.0 });
    w.push(Box::new(lbl_search));
    let mut search_inp = TextInput::new(search);
    search_inp.select_on_focus = false;
    search_inp.set_bounds(D2D_RECT_F { left: inner_l + 54.0, top: y, right: inner_l + inner_w - 170.0, bottom: y + 28.0 });
    w.push(Box::new(search_inp));

    let mut exp_all = IconButton::new("全部展开");
    exp_all.bordered = true;
    exp_all.cmd = WidgetCmd::ExpandAll;
    exp_all.set_bounds(D2D_RECT_F { left: inner_l + inner_w - 166.0, top: y, right: inner_l + inner_w - 84.0, bottom: y + 28.0 });
    w.push(Box::new(exp_all));

    let mut col_all = IconButton::new("全部折叠");
    col_all.bordered = true;
    col_all.cmd = WidgetCmd::CollapseAll;
    col_all.set_bounds(D2D_RECT_F { left: inner_l + inner_w - 80.0, top: y, right: inner_l + inner_w, bottom: y + 28.0 });
    w.push(Box::new(col_all));
    y += 42.0;

    let s_main = main_state();
    if s_main.is_null() { *content_h = y; return w; }
    let entries = &(*s_main).entries;
    let filtered: Vec<&config::Entry> = entries.iter().filter(|e| !e.key.starts_with('_')).collect();

    let search_lower = search.to_lowercase();
    let search_active = !search.is_empty();

    // Group entries by category with global indices
    let mut cat_map: Vec<(String, Vec<(usize, &config::Entry)>)> = Vec::new();
    for (gi, e) in filtered.iter().enumerate() {
        let cat_name = e.category.as_deref().unwrap_or("未分类").to_string();
        if let Some(pos) = cat_map.iter().position(|(n, _)| *n == cat_name) {
            cat_map[pos].1.push((gi, e));
        } else {
            cat_map.push((cat_name, vec![(gi, e)]));
        }
    }
    if let Some(pos) = cat_map.iter().position(|(n, _)| n == "未分类") {
        let uncat = cat_map.remove(pos);
        cat_map.push(uncat);
    }
    // Restore expand state from AppState
    let sm = main_state();
    let saved_state = if !sm.is_null() { unsafe { (*sm).codes_cat_state.clone() } } else { Vec::new() };
    cat_expanded.clear();
    for i in 0..cat_map.len() {
        cat_expanded.push(if i < saved_state.len() { saved_state[i] } else { true });
    }

    let row_h = 28.0;
    let col_key_w = 90.0;
    let col_val_w = 230.0;
    let del_w = 24.0;
    let col_desc_w = inner_w - col_key_w - col_val_w - del_w - 20.0;
    let menu_btn_w = 44.0;

    for (ci, (cat_name, cat_entries)) in cat_map.iter().enumerate() {
        let ct = y;

        // Category header: [▼ ▲] + name + [⋮]
        let arr = if ci < cat_expanded.len() && cat_expanded[ci] { "▼" } else { "▶" };
        let mut arr_lbl = ClickLabel::new(arr);
        arr_lbl.cmd = WidgetCmd::CatToggle(ci);
        arr_lbl.set_bounds(D2D_RECT_F { left: inner_l, top: y, right: inner_l + 20.0, bottom: y + row_h });
        w.push(Box::new(arr_lbl));

        let mut name_lbl = ClickLabel::new(cat_name);
        name_lbl.cmd = WidgetCmd::CatToggle(ci);
        name_lbl.set_bounds(D2D_RECT_F { left: inner_l + 24.0, top: y, right: inner_l + inner_w - menu_btn_w - 104.0, bottom: y + row_h });
        w.push(Box::new(name_lbl));

        let mut add_btn = IconButton::new("＋添加识别码");
        add_btn.cmd = WidgetCmd::EntryAdd(ci);
        add_btn.set_bounds(D2D_RECT_F { left: inner_l + inner_w - menu_btn_w - 100.0, top: y, right: inner_l + inner_w - menu_btn_w - 4.0, bottom: y + row_h });
        w.push(Box::new(add_btn));

        let mut menu_btn = ThreeDotsButton::new(&["重命名分类", "删除分类"], ci);
        menu_btn.set_bounds(D2D_RECT_F { left: inner_l + inner_w - menu_btn_w, top: y, right: inner_l + inner_w, bottom: y + row_h });
        w.push(Box::new(menu_btn));
        y += row_h + 4.0;

        cards.push(D2D_RECT_F { left: card_l, top: ct, right: card_r, bottom: y });

        if ci < cat_expanded.len() && (cat_expanded[ci] || search_active) {
            let visible: Vec<(usize, &config::Entry)> = if search_active {
                let sm = main_state();
                let (fuzzy, pinyin, overrides) = if !sm.is_null() {
                    let s = unsafe { &*sm };
                    (s.fuzzy_enabled, s.pinyin_enabled, &s.pinyin_overrides)
                } else { (false, false, &HashMap::new()) };
                cat_entries.iter().filter(|(_, e)| {
                    let k = e.key.to_lowercase();
                    let v = e.value.to_lowercase();
                    let d = e.description.as_deref().unwrap_or("").to_lowercase();
                    if k.contains(&search_lower) || v.contains(&search_lower) || d.contains(&search_lower) {
                        return true;
                    }
                    if pinyin || fuzzy {
                        if let Some(lv) = match_level(search, &e.key, false, fuzzy, pinyin, overrides) {
                            if lv > 0 { return true; }
                        }
                    }
                    false
                }).cloned().collect()
            } else {
                cat_entries.clone()
            };

            for &(global_idx, e) in &visible {
                let row_top = y;

                let mut key_inp = TextInput::new(&e.key);
                key_inp.set_bounds(D2D_RECT_F { left: inner_l + 4.0, top: row_top, right: inner_l + col_key_w, bottom: row_top + row_h });
                w.push(Box::new(key_inp));

                let mut val_inp = TextInput::new(&e.value);
                val_inp.set_bounds(D2D_RECT_F { left: inner_l + col_key_w + 8.0, top: row_top, right: inner_l + col_key_w + col_val_w, bottom: row_top + row_h });
                w.push(Box::new(val_inp));

                let desc = e.description.as_deref().unwrap_or("");
                let mut desc_inp = TextInput::new(desc);
                desc_inp.set_bounds(D2D_RECT_F { left: inner_l + col_key_w + col_val_w + 12.0, top: row_top, right: inner_l + inner_w - del_w - 4.0, bottom: row_top + row_h });
                w.push(Box::new(desc_inp));

                let mut del_btn = IconButton::new("✕");
                del_btn.cmd = WidgetCmd::EntryDel(global_idx);
                del_btn.set_bounds(D2D_RECT_F { left: inner_l + inner_w - del_w, top: row_top, right: inner_l + inner_w, bottom: row_top + row_h });
                w.push(Box::new(del_btn));

                y += row_h + 4.0;
            }

        }

        if let Some(last) = cards.last_mut() { last.bottom = y; }
        y += 4.0;
    }

    if cat_map.is_empty() {
        let txt = if search_active { "无匹配的识别码" } else { "暂无识别码" };
        let mut lbl = Label::new(txt);
        lbl.set_bounds(D2D_RECT_F { left: inner_l, top: y, right: inner_l + inner_w, bottom: y + 24.0 });
        w.push(Box::new(lbl));
        y += 30.0;
    }

    *content_h = y;
    w
}

pub unsafe fn open_settings(h: HWND, r: &GuaRenderer) {
    if let Some(ref s) = SETTINGS {
        let _ = ShowWindow(s.hwnd, SW_SHOW);
        let _ = SetForegroundWindow(s.hwnd);
        return;
    }

    let inst = GetModuleHandleW(None).unwrap();
    let cn = to_w("Gua_Settings");
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(settings_proc),
        hInstance: inst.into(),
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
        hbrBackground: HBRUSH(ptr::null_mut()),
        lpszClassName: PCWSTR(cn.as_ptr()),
        ..Default::default()
    };
    RegisterClassW(&wc);

    let hwnd_s = CreateWindowExW(
        WINDOW_EX_STYLE::default(), PCWSTR(cn.as_ptr()), w!("Gua 设置"),
        WS_POPUP,
        0, 0, S_W, S_H, None, None, Some(inst.into()), None,
    ).unwrap();

    let hrgn = CreateRoundRectRgn(0, 0, S_W + 1, S_H + 1, 12, 12);
    if !hrgn.0.is_null() { SetWindowRgn(hwnd_s, hrgn, true.into()); }

    let mon = MonitorFromWindow(hwnd_s, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if GetMonitorInfoW(mon, &mut mi).as_bool() {
        let x = mi.rcWork.left + (mi.rcWork.right - mi.rcWork.left - S_W) / 2;
        let y = mi.rcWork.top + (mi.rcWork.bottom - mi.rcWork.top - S_H) / 2;
        let _ = SetWindowPos(hwnd_s, Some(HWND_TOP), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
    }

    let dxgi_device: IDXGIDevice = r.d3d_device.cast().unwrap();
    let adapter = dxgi_device.GetAdapter().unwrap();
    let factory: IDXGIFactory2 = adapter.GetParent().unwrap();

    let desc = DXGI_SWAP_CHAIN_DESC1 {
        Width: S_W as u32, Height: S_H as u32, Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        Stereo: false.into(), SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT, BufferCount: 2,
        Scaling: DXGI_SCALING_NONE, SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
        AlphaMode: DXGI_ALPHA_MODE_IGNORE, Flags: 0u32,
    };
    let sc = factory.CreateSwapChainForHwnd(&r.d3d_device, hwnd_s, &desc, None, None).unwrap();

    let d2d = r.d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE).unwrap();
    let back: IDXGISurface = sc.GetBuffer(0).unwrap();
    let props = D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
        dpiX: 96.0, dpiY: 96.0,
        bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
        colorContext: std::mem::ManuallyDrop::new(None),
    };
    let target = d2d.CreateBitmapFromDxgiSurface(&back, Some(&props)).unwrap();
    d2d.SetTarget(&target);

    let win = SettingsWin {
        hwnd: hwnd_s, swap_chain: sc, d2d_context: d2d,
        target: Some(target), widgets: Vec::new(), cards: Vec::new(),
        cat: 0, sel_cat: 99, scroll_y: 0.0, content_h: 0.0,
        focused_idx: None, capturing_hotkey: false,
        mod_held: [false; 4],
        close_hovered: false, save_hovered: false,
        codes_search: String::new(), codes_version: 0,
        cat_expanded: Vec::new(),
        scroll_dragging: false, scroll_drag_start_y: 0.0,
        composing: String::new(),
    };
    SETTINGS = Some(win);
    let _ = ShowWindow(hwnd_s, SW_SHOW);
}

fn format_hotkey_string(vk: u32, mod_held: &[bool; 4]) -> String {
    let has_mod = mod_held[0] || mod_held[1] || mod_held[2] || mod_held[3];
    let mods: [bool; 4] = if has_mod {
        *mod_held
    } else {
        // fallback: GetAsyncKeyState
        unsafe {
            [
                GetAsyncKeyState(0x11) < 0,
                GetAsyncKeyState(0x12) < 0,
                GetAsyncKeyState(0x10) < 0,
                GetAsyncKeyState(0x5B) < 0 || GetAsyncKeyState(0x5C) < 0,
            ]
        }
    };
    let mod_names = ["Ctrl", "Alt", "Shift", "Win"];
    let mut parts: Vec<&str> = Vec::new();
    for (i, &m) in mods.iter().enumerate() { if m { parts.push(mod_names[i]); } }
    if parts.is_empty() { return String::new(); }
    let key_name = match vk {
        0x20 => "Space", 0x0D => "Enter", 0x09 => "Tab", 0x08 => "Backspace", 0x1B => "Escape",
        0x2E => "Delete", 0x2D => "Insert", 0x24 => "Home", 0x23 => "End",
        0x21 => "PageUp", 0x22 => "PageDown",
        0x25 => "Left", 0x26 => "Up", 0x27 => "Right", 0x28 => "Down",
        0x6E => { parts.push("Separator"); return parts.join("+"); }
        0x6F => { parts.push("/"); return parts.join("+"); }
        0x70..=0x87 => { let n = vk - 0x6F; return format!("{}F{}", parts.join("+"), n); }
        0x41..=0x5A => { let c = (vk as u8 - 0x41 + b'A') as char; parts.push(Box::leak(Box::new(c.to_string())).as_str()); return parts.join("+"); }
        0x30..=0x39 => { let c = (vk as u8 - 0x30 + b'0') as char; parts.push(Box::leak(Box::new(c.to_string())).as_str()); return parts.join("+"); }
        0x6A => { parts.push("*"); return parts.join("+"); }
        0x6B => { parts.push("+"); return parts.join("+"); }
        0x6D => { parts.push("-"); return parts.join("+"); }
        0xBC => { parts.push(","); return parts.join("+"); }
        0xBE => { parts.push("."); return parts.join("+"); }
        0xBA => { parts.push(";"); return parts.join("+"); }
        0xBD => { parts.push("-"); return parts.join("+"); }
        0xBB => { parts.push("="); return parts.join("+"); }
        0xDB => { parts.push("["); return parts.join("+"); }
        0xDD => { parts.push("]"); return parts.join("+"); }
        0xDC => { parts.push("\\"); return parts.join("+"); }
        0xBF => { parts.push("/"); return parts.join("+"); }
        0xC0 => { parts.push("`"); return parts.join("+"); }
        0xDE => { parts.push("'"); return parts.join("+"); }
        _ => { return String::new(); }
    };
    parts.push(key_name);
    parts.join("+")
}

unsafe fn set_capturing(s: &mut SettingsWin, capturing: bool) {
    if s.capturing_hotkey == capturing { return; }
    s.capturing_hotkey = capturing;
    let main_hwnd = HWND(MAIN_HWND as *mut std::ffi::c_void);
    if main_hwnd.0.is_null() { return; }
    if capturing {
        let _ = UnregisterHotKey(main_hwnd, HOTKEY_ID);
    } else {
        let s_main = main_state();
        if !s_main.is_null() {
            let ms = &*s_main;
            let _ = RegisterHotKey(main_hwnd, HOTKEY_ID, ms.mod_keys, ms.hotkey_vk);
        }
    }
}

unsafe fn clear_focus(s: &mut SettingsWin) {
    if let Some(idx) = s.focused_idx.take() {
        if idx < s.widgets.len() { s.widgets[idx].set_focused(false); }
    }
    set_capturing(s, false);
}

pub unsafe extern "system" fn settings_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CLOSE => {
            SETTINGS = None;
            let _ = DestroyWindow(h);
            return LRESULT(0);
        }

        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            BeginPaint(h, &mut ps);
            let s = match &mut SETTINGS { Some(s) => s, None => { let _ = EndPaint(h, &ps); return LRESULT(0); } };

            // Rebuild widgets if needed
            let mut need_rebuild = s.cat != s.sel_cat;
            if !need_rebuild && s.cat == 2 {
                let cur_search = s.widgets.get(1).map(|w| w.text().to_string()).unwrap_or_default();
                if cur_search != s.codes_search { need_rebuild = true; s.codes_search = cur_search; }
                if s.codes_version > 0 { need_rebuild = true; s.codes_version = 0; }
            }
            if need_rebuild {
                if s.cat == 2 { sync_codes_entries(s); }
                let was_cat_switch = s.sel_cat != s.cat;
                s.sel_cat = s.cat;
                if was_cat_switch { s.scroll_y = 0.0; }
                s.focused_idx = None;
                set_capturing(s, false);
                s.mod_held = [false; 4];
                if s.cat == 2 {
                    let widgets = build_codes_tab(&mut s.cards, &mut s.content_h, &s.codes_search, &mut s.cat_expanded);
                    s.widgets = widgets;
                } else {
                    s.widgets = build_widgets(s.cat, &mut s.cards, &mut s.content_h);
                }
            }

            let _ = s.d2d_context.BeginDraw();
            let _ = s.d2d_context.Clear(Some(&D2D1_COLOR_F { r: 0.08, g: 0.08, b: 0.08, a: 1.0 } as *const _));

            let s_main = main_state();
            let dwf = if !s_main.is_null() { gua_renderer(&*s_main).map(|r| r.dwrite_factory.clone()) } else { None };
            let (ar, ag, ab) = if !s_main.is_null() { let s = &*s_main; let c = color_to_d2d(s.accent_color, 1.0); (c.r, c.g, c.b) } else { ACCENT };

            // ── 标题栏 ──
            if let Some(b) = mk_brush_(&s.d2d_context, 0.05, 0.05, 0.05, 1.0) {
                s.d2d_context.FillRectangle(&D2D_RECT_F { left: 0.0, top: 0.0, right: S_W as f32, bottom: TITLE_H } as *const _, &b);
            }
            if let Some(b) = mk_brush_(&s.d2d_context, 0.12, 0.12, 0.12, 1.0) {
                s.d2d_context.FillRectangle(&D2D_RECT_F { left: 0.0, top: TITLE_H - 1.0, right: S_W as f32, bottom: TITLE_H } as *const _, &b);
            }
            if let Some(ref dwf) = dwf {
                let f = to_w("Microsoft YaHei"); let l = to_w("en-us");
                if let Ok(tf) = dwf.CreateTextFormat(PCWSTR(f.as_ptr()), None, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 13.0, PCWSTR(l.as_ptr())) {
                    let _ = tf.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                    if let Some(b) = mk_brush_(&s.d2d_context, 0.45, 0.45, 0.45, 1.0) {
                        s.d2d_context.DrawText(&to_w("Gua 设置"), &tf, &D2D_RECT_F { left: 16.0, top: 0.0, right: 120.0, bottom: TITLE_H } as *const _, &b, D2D1_DRAW_TEXT_OPTIONS(0), DWRITE_MEASURING_MODE(0));
                    }
                }
            }

            // ── 侧边栏 ──
            if let Some(b) = mk_brush_(&s.d2d_context, 0.06, 0.06, 0.06, 1.0) {
                s.d2d_context.FillRectangle(&D2D_RECT_F { left: 0.0, top: TITLE_H, right: SIDEBAR_W, bottom: S_H as f32 - BOTTOM_H } as *const _, &b);
            }
            if let Some(b) = mk_brush_(&s.d2d_context, 0.14, 0.14, 0.14, 1.0) {
                s.d2d_context.FillRectangle(&D2D_RECT_F { left: SIDEBAR_W - 1.0, top: TITLE_H, right: SIDEBAR_W, bottom: S_H as f32 - BOTTOM_H } as *const _, &b);
            }
            let names = ["通用", "外观", "识别码"];
            for (i, name) in names.iter().enumerate() {
                let btn_top = TITLE_H + 12.0 + i as f32 * 46.0;
                let btn = D2D_RECT_F { left: 12.0, top: btn_top, right: SIDEBAR_W - 12.0, bottom: btn_top + 38.0 };
                let sel = i == s.cat;
                if sel {
                    if let Some(b) = mk_brush_(&s.d2d_context, ar, ag, ab, 1.0) {
                        s.d2d_context.FillRectangle(&D2D_RECT_F { left: 4.0, top: btn_top + 4.0, right: 7.0, bottom: btn_top + 34.0 } as *const _, &b);
                    }
                    if let Some(b) = mk_brush_(&s.d2d_context, ar, ag, ab, 0.15) {
                        s.d2d_context.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: btn, radiusX: 6.0, radiusY: 6.0 } as *const _, &b);
                    }
                }
                let (rr, gg, bb) = if sel { (0.85, 0.85, 0.85) } else { (0.50, 0.50, 0.50) };
                if let Some(b) = mk_brush_(&s.d2d_context, rr, gg, bb, 1.0) {
                    if let Some(ref dwf) = dwf {
                        let f2 = to_w("Microsoft YaHei"); let l2 = to_w("en-us");
                        if let Ok(tf) = dwf.CreateTextFormat(PCWSTR(f2.as_ptr()), None, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 14.0, PCWSTR(l2.as_ptr())) {
                            let _ = tf.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
                            let _ = tf.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                            s.d2d_context.DrawText(&to_w(name), &tf, &btn as *const _, &b, D2D1_DRAW_TEXT_OPTIONS(0), DWRITE_MEASURING_MODE(0));
                        }
                    }
                }
            }

            // ── 内容区 ──
            s.d2d_context.PushAxisAlignedClip(&D2D_RECT_F { left: CONTENT_L, top: TITLE_H, right: S_W as f32, bottom: S_H as f32 - BOTTOM_H } as *const _, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);

            #[repr(C)]
            struct Mtx { _11: f32, _12: f32, _21: f32, _22: f32, _31: f32, _32: f32 }
            let mtx = Mtx { _11: 1.0, _12: 0.0, _21: 0.0, _22: 1.0, _31: 0.0, _32: -s.scroll_y };
            s.d2d_context.SetTransform(&mtx as *const _ as *const _);

            for card in &s.cards {
                if let Some(b) = mk_brush_(&s.d2d_context, 0.14, 0.14, 0.14, 1.0) {
                    s.d2d_context.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: *card, radiusX: 8.0, radiusY: 8.0 } as *const _, &b);
                }
            }
            if let Some(dwrite) = dwf.clone() {
                let res = D2DRes { d2d: s.d2d_context.clone(), dwrite };
                for widget in &s.widgets { widget.draw(&res); }
                for widget in &s.widgets { widget.draw_overlay(&res); }
            }

            let ident = Mtx { _11: 1.0, _12: 0.0, _21: 0.0, _22: 1.0, _31: 0.0, _32: 0.0 };
            s.d2d_context.SetTransform(&ident as *const _ as *const _);
            s.d2d_context.PopAxisAlignedClip();

            // ── 滚动条 ──
            let track_l = S_W as f32 - 16.0;
            let track_t = TITLE_H + 4.0;
            let track_h = S_H as f32 - BOTTOM_H - TITLE_H - 8.0;
            if let Some(b) = mk_brush_(&s.d2d_context, 0.10, 0.10, 0.10, 1.0) {
                s.d2d_context.FillRectangle(&D2D_RECT_F { left: track_l, top: track_t, right: track_l + 8.0, bottom: track_t + track_h } as *const _, &b);
            }
            let max_scroll = (s.content_h - (S_H as f32 - TITLE_H - BOTTOM_H)).max(0.0);
            if max_scroll > 0.0 {
                let thumb_h = (track_h - 10.0) * (track_h / (track_h + max_scroll));
                let thumb_t = track_t + 5.0 + (s.scroll_y / max_scroll) * (track_h - 10.0 - thumb_h);
                if let Some(b) = mk_brush_(&s.d2d_context, 0.30, 0.30, 0.30, 1.0) {
                    s.d2d_context.FillRoundedRectangle(&D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F { left: track_l, top: thumb_t, right: track_l + 8.0, bottom: thumb_t + thumb_h },
                        radiusX: 3.0, radiusY: 3.0,
                    } as *const _, &b);
                }
            }

            // ── 底部操作栏 ──
            if let Some(b) = mk_brush_(&s.d2d_context, 0.06, 0.06, 0.06, 1.0) {
                s.d2d_context.FillRectangle(&D2D_RECT_F { left: 0.0, top: S_H as f32 - BOTTOM_H, right: S_W as f32, bottom: S_H as f32 } as *const _, &b);
            }
            if let Some(b) = mk_brush_(&s.d2d_context, 0.12, 0.12, 0.12, 1.0) {
                s.d2d_context.FillRectangle(&D2D_RECT_F { left: 0.0, top: S_H as f32 - BOTTOM_H, right: S_W as f32, bottom: S_H as f32 - BOTTOM_H + 1.0 } as *const _, &b);
            }

            let bty = S_H as f32 - BOTTOM_H + 10.0;
            let bby = S_H as f32 - 10.0;
            let close_l = S_W as f32 - 20.0 - 80.0 * 2.0 - 8.0;
            let save_l = S_W as f32 - 20.0 - 80.0;

            let cbr = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: close_l, top: bty, right: close_l + 80.0, bottom: bby }, radiusX: 6.0, radiusY: 6.0 };
            let (cr, cg, cb) = if s.close_hovered { (0.65, 0.20, 0.15) } else { (0.35, 0.35, 0.35) };
            if let Some(b) = mk_brush_(&s.d2d_context, cr, cg, cb, 1.0) { s.d2d_context.FillRoundedRectangle(&cbr as *const _, &b); }
            if let Some(ref dwf) = dwf {
                let f3 = to_w("Microsoft YaHei"); let l3 = to_w("en-us");
                if let Ok(tf) = dwf.CreateTextFormat(PCWSTR(f3.as_ptr()), None, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 12.0, PCWSTR(l3.as_ptr())) {
                    let _ = tf.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
                    let _ = tf.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                    if let Some(b) = mk_brush_(&s.d2d_context, 1.0, 1.0, 1.0, 1.0) {
                        s.d2d_context.DrawText(&to_w("关闭"), &tf, &cbr.rect as *const _, &b, D2D1_DRAW_TEXT_OPTIONS(0), DWRITE_MEASURING_MODE(0));
                    }
                }
            }

            let sbr_ = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: save_l, top: bty, right: save_l + 80.0, bottom: bby }, radiusX: 6.0, radiusY: 6.0 };
            let (sar, sag, sab) = if s.save_hovered { ((ar + 0.1).min(1.0), (ag + 0.1).min(1.0), (ab + 0.1).min(1.0)) } else { (ar, ag, ab) };
            if let Some(b) = mk_brush_(&s.d2d_context, sar, sag, sab, 1.0) { s.d2d_context.FillRoundedRectangle(&sbr_ as *const _, &b); }
            if let Some(ref dwf) = dwf {
                let f4 = to_w("Microsoft YaHei"); let l4 = to_w("en-us");
                if let Ok(tf) = dwf.CreateTextFormat(PCWSTR(f4.as_ptr()), None, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 12.0, PCWSTR(l4.as_ptr())) {
                    let _ = tf.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
                    let _ = tf.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                    if let Some(b) = mk_brush_(&s.d2d_context, 1.0, 1.0, 1.0, 1.0) {
                        s.d2d_context.DrawText(&to_w("保存"), &tf, &sbr_.rect as *const _, &b, D2D1_DRAW_TEXT_OPTIONS(0), DWRITE_MEASURING_MODE(0));
                    }
                }
            }

            let _ = s.d2d_context.EndDraw(None, None);
            let _ = s.swap_chain.Present(0, DXGI_PRESENT(0));
            let _ = EndPaint(h, &ps);
            return LRESULT(0);
        }

        WM_LBUTTONDOWN => {
            let x = (lp.0 as u32 & 0xFFFF) as i32 as f32;
            let y = ((lp.0 as u32 >> 16) & 0xFFFF) as i32 as f32;
            if let Some(s) = &mut SETTINGS {
                let bty = S_H as f32 - BOTTOM_H + 10.0;
                let bby = S_H as f32 - 10.0;
                let close_l = S_W as f32 - 20.0 - 80.0 * 2.0 - 8.0;
                let save_l = S_W as f32 - 20.0 - 80.0;

                if x >= close_l && x <= close_l + 80.0 && y >= bty && y <= bby {
                    if s.cat == 2 { sync_codes_entries(s); }
                    SETTINGS = None;
                    let _ = DestroyWindow(h);
                    return LRESULT(0);
                }
                if x >= save_l && x <= save_l + 80.0 && y >= bty && y <= bby {
                    return LRESULT(0);
                }

                // ── 滚动条拖拽 ──
                let track_l = S_W as f32 - 14.0;
                let track_t = TITLE_H + 4.0;
                let track_h = S_H as f32 - BOTTOM_H - TITLE_H - 8.0;
                let max_scroll = (s.content_h - (S_H as f32 - TITLE_H - BOTTOM_H)).max(0.0);
                if max_scroll > 0.0 && x >= track_l && x <= track_l + 8.0 && y >= track_t && y <= track_t + track_h {
                    let thumb_h = (track_h - 10.0) * (track_h / (track_h + max_scroll));
                    let thumb_t = track_t + 5.0 + (s.scroll_y / max_scroll) * (track_h - 10.0 - thumb_h);
                    if y >= thumb_t && y <= thumb_t + thumb_h {
                        s.scroll_dragging = true;
                        s.scroll_drag_start_y = y;
                        let _ = SetCapture(h);
                    } else {
                        let ratio = ((y - track_t - 5.0 - thumb_h / 2.0) / (track_h - 10.0 - thumb_h)).clamp(0.0, 1.0);
                        s.scroll_y = ratio * max_scroll;
                        let _ = InvalidateRect(Some(h), None, true);
                    }
                    return LRESULT(0);
                }

                if x < SIDEBAR_W {
                    for i in 0..3 {
                        let btn_top = TITLE_H + 12.0 + i as f32 * 46.0;
                        let btn = D2D_RECT_F { left: 12.0, top: btn_top, right: SIDEBAR_W - 12.0, bottom: btn_top + 38.0 };
                        if x >= btn.left && x <= btn.right && y >= btn.top && y <= btn.bottom {
                            if s.cat == 2 { sync_codes_entries(s); }
                            clear_focus(s);
                            s.cat = i;
                            let _ = InvalidateRect(Some(h), None, true);
                            break;
                        }
                    }
                } else {
                    let _ = SetForegroundWindow(h);
                    let adj_y = y + s.scroll_y;
                    let old_idx = s.focused_idx;
                    set_capturing(s, false);
                    let s_main_click = main_state();
                    let click_res = if !s_main_click.is_null() { gua_renderer(&*s_main_click).map(|r| D2DRes { d2d: s.d2d_context.clone(), dwrite: r.dwrite_factory.clone() }) } else { None };
                    let mut handled = false;
                    let mut handled_idx = 0usize;
                    let mut captures = false;
                    for (i, w) in s.widgets.iter_mut().enumerate() {
                        let ok = if let Some(ref res) = click_res { w.on_click_with(x, adj_y, res) } else { w.on_click(x, adj_y) };
                        if ok {
                            handled_idx = i;
                            if w.focused() {
                                s.focused_idx = Some(i);
                                captures = w.captures_hotkey();
                                if old_idx != Some(i) {
                                    if let Some(oi) = old_idx { if oi < s.widgets.len() { s.widgets[oi].set_focused(false); } }
                                }
                            }
                            handled = true;
                            break;
                        }
                    }
                    if handled {
                        if handled_idx < s.widgets.len() { s.widgets[handled_idx].on_mouse_down(x, adj_y); }
                        set_capturing(s, captures);
                        if s.cat == 2 && handled_idx < s.widgets.len() {
                            match s.widgets[handled_idx].cmd() {
                                WidgetCmd::EntryDel(global_idx) => {
                                    sync_codes_entries(s);
                                    let s_main = main_state();
                                    if !s_main.is_null() && global_idx < (*s_main).entries.len() {
                                        (*s_main).entries.remove(global_idx);
                                        s.codes_version += 1;
                                    }
                                    let _ = InvalidateRect(Some(h), None, true);
                                }
                                WidgetCmd::EntryAdd(ci) => {
                                    sync_codes_entries(s);
                                    let s_main = main_state();
                                    if !s_main.is_null() {
                                        let state = &mut *s_main;
                                        let cat_name = {
                                            let mut cats: Vec<String> = Vec::new();
                                            for e in &state.entries {
                                                if e.key.starts_with('_') { continue; }
                                                let n = e.category.as_deref().unwrap_or("未分类").to_string();
                                                if !cats.contains(&n) { cats.push(n); }
                                            }
                                            if let Some(p) = cats.iter().position(|n| n == "未分类") {
                                                let u = cats.remove(p); cats.push(u);
                                            }
                                            cats.get(ci).cloned().unwrap_or("未分类".to_string())
                                        };
                                        state.entries.push(config::Entry {
                                            key: "新识别码".to_string(),
                                            value: String::new(),
                                            category: Some(cat_name),
                                            description: None,
                                        });
                                        s.codes_version += 1;
                                    }
                                    let _ = InvalidateRect(Some(h), None, true);
                                }
                                WidgetCmd::CatToggle(ci) => {
                                    sync_codes_entries(s);
                                    if ci < s.cat_expanded.len() {
                                        s.cat_expanded[ci] = !s.cat_expanded[ci];
                                        let sm = main_state();
                                        if !sm.is_null() { unsafe { (*sm).codes_cat_state = s.cat_expanded.clone(); } }
                                        s.codes_version += 1;
                                        let _ = InvalidateRect(Some(h), None, true);
                                    }
                                }
                                WidgetCmd::ExpandAll => {
                                    for e in &mut s.cat_expanded { *e = true; }
                                    let sm = main_state();
                                    if !sm.is_null() { unsafe { (*sm).codes_cat_state = s.cat_expanded.clone(); } }
                                    s.codes_version += 1;
                                    let _ = InvalidateRect(Some(h), None, true);
                                }
                                WidgetCmd::CollapseAll => {
                                    for e in &mut s.cat_expanded { *e = false; }
                                    let sm = main_state();
                                    if !sm.is_null() { unsafe { (*sm).codes_cat_state = s.cat_expanded.clone(); } }
                                    s.codes_version += 1;
                                    let _ = InvalidateRect(Some(h), None, true);
                                }
                                WidgetCmd::CatRename(ci) => {
                                    // TODO: rename category dialog
                                    let _ = InvalidateRect(Some(h), None, true);
                                }
                                WidgetCmd::CatDelete(ci) => {
                                    sync_codes_entries(s);
                                    let s_main = main_state();
                                    if !s_main.is_null() {
                                        let state = &mut *s_main;
                                        let cat_name = {
                                            let mut m: Vec<String> = Vec::new();
                                            for e in &state.entries {
                                                if e.key.starts_with('_') { continue; }
                                                let n = e.category.as_deref().unwrap_or("未分类").to_string();
                                                if !m.contains(&n) { m.push(n); }
                                            }
                                            if let Some(p) = m.iter().position(|n| n == "未分类") {
                                                let u = m.remove(p); m.push(u);
                                            }
                                            m.get(ci).cloned().unwrap_or_default()
                                        };
                                        state.entries.retain(|e| {
                                            if e.key.starts_with('_') { return true; }
                                            e.category.as_deref().unwrap_or("未分类") != cat_name
                                        });
                                        s.codes_version += 1;
                                    }
                                    let _ = InvalidateRect(Some(h), None, true);
                                }
                                _ => {}
                            }
                        }
                        // Non-focusable click → clear focus
                        if !s.widgets[handled_idx].focused() {
                            if let Some(oi) = old_idx {
                                if oi < s.widgets.len() { s.widgets[oi].set_focused(false); }
                            }
                            s.focused_idx = None;
                        }
                        let _ = InvalidateRect(Some(h), None, true);
                    }
                    if !handled {
                        clear_focus(s);
                    }
                }
            }
            return LRESULT(0);
        }

        WM_LBUTTONUP => {
            let x = (lp.0 as u32 & 0xFFFF) as i32 as f32;
            let y = ((lp.0 as u32 >> 16) & 0xFFFF) as i32 as f32;
            if let Some(s) = &mut SETTINGS {
                s.scroll_dragging = false;
                let _ = ReleaseCapture();
                let adj_y = y + s.scroll_y;
                for w in &mut s.widgets { w.on_mouse_up(x, adj_y); }
                let _ = InvalidateRect(Some(h), None, true);
            }
            return LRESULT(0);
        }

        WM_MOUSEMOVE => {
            let x = (lp.0 as u32 & 0xFFFF) as i32 as f32;
            let y = ((lp.0 as u32 >> 16) & 0xFFFF) as i32 as f32;
            if let Some(s) = &mut SETTINGS {
                if s.scroll_dragging {
                    let dy = y - s.scroll_drag_start_y;
                    s.scroll_drag_start_y = y;
                    let max_scroll = (s.content_h - (S_H as f32 - TITLE_H - BOTTOM_H)).max(0.0);
                    if max_scroll > 0.0 {
                        let track_h = S_H as f32 - BOTTOM_H - TITLE_H - 8.0;
                        let thumb_h = (track_h - 10.0) * (track_h / (track_h + max_scroll));
                        let move_ratio = dy / (track_h - 10.0 - thumb_h);
                        s.scroll_y = (s.scroll_y + move_ratio * max_scroll).clamp(0.0, max_scroll);
                        let _ = InvalidateRect(Some(h), None, true);
                    }
                    return LRESULT(0);
                }
                let adj_y = y + s.scroll_y;
                let bw2 = 80.0;
                let bty = S_H as f32 - BOTTOM_H + 10.0;
                let bby = S_H as f32 - 10.0;
                let cl = S_W as f32 - 20.0 - bw2 * 2.0 - 8.0;
                let sl = S_W as f32 - 20.0 - bw2;
                let close_hit = x >= cl && x <= cl + bw2 && y >= bty && y <= bby;
                let save_hit = x >= sl && x <= sl + bw2 && y >= bty && y <= bby;
                if close_hit != s.close_hovered || save_hit != s.save_hovered {
                    s.close_hovered = close_hit;
                    s.save_hovered = save_hit;
                    let _ = InvalidateRect(Some(h), None, true);
                }
                for w in &mut s.widgets { w.on_mouse_move(x, adj_y); }
                let _ = RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
            }
            return LRESULT(0);
        }

        WM_MOUSEWHEEL => {
            if let Some(s) = &mut SETTINGS {
                let delta = (wp.0 as u32 >> 16) as i16;
                let step = delta as f32 / 120.0 * 24.0;
                // 先发给聚焦的多行输入框
                let mut handled = false;
                if let Some(idx) = s.focused_idx {
                    if idx < s.widgets.len() {
                        handled = s.widgets[idx].on_mouse_wheel(step);
                    }
                }
                if !handled {
                    s.scroll_y = (s.scroll_y - step).clamp(0.0, (s.content_h - (S_H as f32 - TITLE_H - BOTTOM_H - CONTENT_PAD)).max(0.0));
                }
                let _ = InvalidateRect(Some(h), None, true);
            }
            return LRESULT(0);
        }

        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let vk = wp.0 as u32;
            if let Some(s) = &mut SETTINGS {
                match vk {
                    0x10 => { s.mod_held[2] = true; }
                    0x11 => { s.mod_held[0] = true; }
                    0x12 => { s.mod_held[1] = true; }
                    0x5B | 0x5C => { s.mod_held[3] = true; }
                    _ => {}
                }
                if s.capturing_hotkey {
                    match vk {
                        0x1B => { clear_focus(s); let _ = InvalidateRect(Some(h), None, true); }
                        0x10 | 0x11 | 0x12 | 0x5B | 0x5C => {}
                        _ => {
                            if s.mod_held.iter().any(|&m| m) {
                                let hotkey_str = format_hotkey_string(vk, &s.mod_held);
                                if !hotkey_str.is_empty() {
                                    let idx = s.focused_idx;
                                    clear_focus(s);
                                    if let Some(idx) = idx { if idx < s.widgets.len() { s.widgets[idx].set_text(&hotkey_str); } }
                                    let _ = InvalidateRect(Some(h), None, true);
                                }
                            }
                        }
                    }
                } else if vk == 0x1B {
                    clear_focus(s);
                    let _ = InvalidateRect(Some(h), None, true);
                } else if let Some(idx) = s.focused_idx {
                    if idx < s.widgets.len() {
                        if s.widgets[idx].on_key_down(vk) { let _ = InvalidateRect(Some(h), None, true); }
                    }
                }
            }
            return LRESULT(0);
        }

        WM_KEYUP | WM_SYSKEYUP => {
            let vk = wp.0 as u32;
            if let Some(s) = &mut SETTINGS {
                match vk {
                    0x10 => { s.mod_held[2] = false; }
                    0x11 => { s.mod_held[0] = false; }
                    0x12 => { s.mod_held[1] = false; }
                    0x5B | 0x5C => { s.mod_held[3] = false; }
                    _ => {}
                }
            }
            return LRESULT(0);
        }

        WM_CHAR => {
            let ch = wp.0 as u32;
            if let Some(s) = &mut SETTINGS {
                if let Some(idx) = s.focused_idx {
                    if idx < s.widgets.len() {
                        if s.widgets[idx].on_char(ch) { let _ = InvalidateRect(Some(h), None, true); }
                    }
                }
            }
            return LRESULT(0);
        }

        WM_IME_SETCONTEXT => {
            return DefWindowProcW(h, msg, wp, LPARAM(lp.0 & !(ISC_SHOWUICOMPOSITIONWINDOW as isize)));
        }

        WM_IME_STARTCOMPOSITION => {
            if let Some(s) = &mut SETTINGS {
                let himc = ImmGetContext(h);
                if himc != 0 {
                    let r = if let Some(idx) = s.focused_idx {
                        if idx < s.widgets.len() { s.widgets[idx].bounds() } else { D2D_RECT_F::default() }
                    } else { D2D_RECT_F::default() };
                    let cf = COMPOSITIONFORM {
                        dwStyle: CFS_FORCE_POSITION,
                        ptCurrentPos: POINT { x: r.left as i32 + 8, y: r.bottom as i32 + 4 },
                        rcArea: RECT::default(),
                    };
                    let _ = ImmSetCompositionWindow(himc, &cf);
                    let _ = ImmReleaseContext(h, himc);
                }
            }
            return LRESULT(0);
        }

        WM_IME_COMPOSITION => {
            if let Some(s) = &mut SETTINGS {
                let himc = ImmGetContext(h);
                if himc != 0 {
                    let r = if let Some(idx) = s.focused_idx {
                        if idx < s.widgets.len() { s.widgets[idx].bounds() } else { D2D_RECT_F::default() }
                    } else { D2D_RECT_F::default() };
                    let cf = COMPOSITIONFORM {
                        dwStyle: CFS_FORCE_POSITION,
                        ptCurrentPos: POINT { x: r.left as i32, y: r.bottom as i32 },
                        rcArea: RECT::default(),
                    };
                    let _ = ImmSetCompositionWindow(himc, &cf);

                    if lp.0 as usize & GCS_RESULTSTR as usize != 0 {
                        if let Some(idx) = s.focused_idx {
                            if idx < s.widgets.len() {
                                let len = ImmGetCompositionStringW(himc, GCS_RESULTSTR, ptr::null_mut(), 0);
                                if len > 0 {
                                    let mut buf = vec![0u16; (len as usize) / 2 + 1];
                                    let _ = ImmGetCompositionStringW(himc, GCS_RESULTSTR, buf.as_mut_ptr() as *mut std::ffi::c_void, len);
                                    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                                    let result = String::from_utf16_lossy(&buf[..end]);
                                    for c in result.chars() {
                                        s.widgets[idx].on_char(c as u32);
                                    }
                                }
                            }
                        }
                        s.composing.clear();
                    }
                    let _ = ImmReleaseContext(h, himc);
                    let _ = InvalidateRect(Some(h), None, true);
                }
            }
            return LRESULT(0);
        }

        WM_IME_ENDCOMPOSITION => {
            if let Some(s) = &mut SETTINGS {
                s.composing.clear();
                let _ = InvalidateRect(Some(h), None, true);
            }
            return LRESULT(0);
        }

        WM_NCHITTEST => {
            let x_screen = (lp.0 as u32 & 0xFFFF) as i32;
            let y_screen = ((lp.0 as u32 >> 16) & 0xFFFF) as i32;
            let mut rc = RECT::default();
            let _ = GetWindowRect(h, &mut rc);
            let rel_x = x_screen - rc.left;
            let rel_y = y_screen - rc.top;
            if rel_y >= 0 && rel_y < TITLE_H as i32 && rel_x >= SIDEBAR_W as i32 {
                return LRESULT(HTCAPTION as isize);
            }
            return LRESULT(HTCLIENT as isize);
        }

        WM_DESTROY => { SETTINGS = None; return LRESULT(0); }
        _ => {}
    }
    DefWindowProcW(h, msg, wp, lp)
}

pub unsafe fn is_open() -> bool { SETTINGS.is_some() }

pub unsafe fn close_settings() {
    if let Some(ref s) = SETTINGS {
        let _ = DestroyWindow(s.hwnd);
    }
    SETTINGS = None;
}

unsafe fn mk_brush_(d2d: &ID2D1DeviceContext, r: f32, g: f32, b: f32, a: f32) -> Option<ID2D1SolidColorBrush> {
    let c = D2D1_COLOR_F { r, g, b, a };
    d2d.CreateSolidColorBrush(&c as *const _, None).ok()
}
