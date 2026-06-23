// vd-hotkeys — 切到指定虚拟桌面

use gua_sdk::{GuaApi, GuaPlugin};

struct VdHotkeysPlugin;
static mut HK_COUNT: i32 = 0;
static mut GUA_API: Option<&'static GuaApi> = None;

impl GuaPlugin for VdHotkeysPlugin {
    const NAME: &'static str = "vd-hotkeys";
    const VERSION: &'static str = "0.1.0";

    fn init(&self, api: &'static GuaApi) -> i32 {
        for i in 0..unsafe { HK_COUNT } { api.unregister_hotkey(i); }
        unsafe { HK_COUNT = 0; }
        let mut n = 0;
        for i in 0..99 {
            let hs = match api.get_config(&format!("desktop_{}", i)) { Some(v) => v, None => break };
            if let Some((m, v)) = parse_hotkey(&hs) {
                if api.register_hotkey(m, v, i) < 0 { api.log(1, &format!("{} 注册失败", hs)); } else { n += 1; }
            } else { api.log(1, &format!("无法解析 '{}'", hs)); }
        }
        unsafe { HK_COUNT = n; }
        api.log(0, &format!("已注册 {} 个桌面快捷键", n)); 0
    }

    fn on_hotkey(&self, id: i32) { let _ = winvd::switch_desktop(id as u32); }

    fn on_config_reload(&self) {
        if let Some(api) = unsafe { GUA_API } { self.init(api); }
    }
}

#[no_mangle]
pub unsafe extern "C" fn gua_plugin_load(api: *const GuaApi, vtable: *mut gua_sdk::PluginVtable) -> i32 {
    if api.is_null() || vtable.is_null() { return -1; }
    GUA_API = Some(&*api);
    let v = &mut *vtable;
    v.vtable_size = std::mem::size_of::<gua_sdk::PluginVtable>() as u32;
    v.name = "vd-hotkeys\0".as_ptr() as *const i8;
    v.version = "0.1.0\0".as_ptr() as *const i8;
    v.init = Some(init_impl);
    v.on_hotkey = Some(on_hotkey_impl);
    v.on_config_reload = Some(on_config_reload_impl);
    0
}

unsafe extern "C" fn init_impl() -> i32 { VdHotkeysPlugin.init(GUA_API.unwrap()) }
unsafe extern "C" fn on_hotkey_impl(id: i32) { VdHotkeysPlugin.on_hotkey(id); }
unsafe extern "C" fn on_config_reload_impl() { VdHotkeysPlugin.on_config_reload(); }

fn parse_hotkey(s: &str) -> Option<(u32, u32)> {
    let p: Vec<&str> = s.split('+').map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
    if p.len() < 2 || p.len() > 5 { return None; }
    let mut m = 0u32;
    for x in &p[..p.len()-1] { m |= match x.to_lowercase().as_str() {
        "alt" => 1, "ctrl"|"control" => 2, "shift" => 4, "win"|"windows"|"super" => 8, _ => return None,
    }; }
    if m == 0 { return None; }
    let u = p[p.len()-1].to_uppercase(); let b = u.as_bytes();
    if b.len() == 1 { let c = b[0]; if (b'A'..=b'Z').contains(&c) || (b'0'..=b'9').contains(&c) { return Some((m, c as u32)); } }
    if b.len() >= 2 && b[0] == b'F' { if let Ok(n) = u[1..].parse::<u32>() { if (1..=24).contains(&n) { return Some((m, 0x6F + n)); } } }
    Some((m, match u.as_str() {
        "SPACE" => 0x20, "ENTER" => 0x0D, "ESCAPE"|"ESC" => 0x1B, "TAB" => 0x09, "BACKSPACE" => 0x08,
        "DELETE"|"DEL" => 0x2E, "INSERT"|"INS" => 0x2D, "HOME" => 0x24, "END" => 0x23,
        "PAGEUP"|"PGUP" => 0x21, "PAGEDOWN"|"PGDN" => 0x22, "UP" => 0x26, "DOWN" => 0x28,
        "LEFT" => 0x25, "RIGHT" => 0x27, _ => return None,
    }))
}
