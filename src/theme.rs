use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;

pub type Color = (f32, f32, f32);

const fn hex(c: u32) -> Color {
    ((c >> 16 & 0xFF) as f32 / 255.0, (c >> 8 & 0xFF) as f32 / 255.0, (c & 0xFF) as f32 / 255.0)
}

pub struct Theme {
    pub bg_sidebar: Color,
    pub bg_main: Color,
    pub bg_title: Color,
    pub bg_raised: Color,
    pub bg_separator: Color,
    pub bg_hover: Color,
    pub bg_input: Color,
    pub bg_widget: Color,
    pub border: Color,
    pub border_hover: Color,
    pub border_focused: Color,
    pub text_dim: Color,
    pub text_secondary: Color,
    pub text_disabled: Color,
    pub text: Color,
    pub text_bright: Color,
    pub text_white: Color,
    pub accent: Color,
    #[allow(dead_code)]
    pub accent_light: Color,
    pub green: Color,
    pub red: Color,
    pub tab_text: Color,
    pub tab_hover_bg: Color,
    pub placeholder: Color,
}

const DARK: &Theme = &Theme {
    bg_sidebar:    hex(0x0F0F0F),
    bg_main:       hex(0x141414),
    bg_title:      hex(0x1F1F1F),
    bg_raised:     hex(0x1A1A1A),
    bg_separator:  hex(0x242424),
    bg_hover:      hex(0x292929),
    bg_input:      hex(0x2E2E2E),
    bg_widget:     hex(0x333333),
    border:        hex(0x383838),
    border_hover:  hex(0x454545),
    border_focused:hex(0x4D4D4D),
    text_dim:      hex(0x595959),
    text_secondary:hex(0x666666),
    text_disabled: hex(0x737373),
    text:          hex(0x808080),
    text_bright:   hex(0xD9D9D9),
    text_white:    hex(0xFFFFFF),
    accent:        hex(0x4A87CC),
    accent_light:  hex(0x66A1E6),
    green:         hex(0x4DCC4D),
    red:           hex(0xA63326),
    tab_text:      hex(0x8C8C8C),
    tab_hover_bg:  hex(0xBFBFBF),
    placeholder:   hex(0xCCCCCC),
};

pub static T: &Theme = DARK;

pub fn brush(d2d: &ID2D1DeviceContext, c: Color, a: f32) -> Option<ID2D1SolidColorBrush> {
    let cf = D2D1_COLOR_F { r: c.0, g: c.1, b: c.2, a };
    unsafe { d2d.CreateSolidColorBrush(&cf as *const _, None).ok() }
}
