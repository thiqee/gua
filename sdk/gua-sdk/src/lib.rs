// Gua Plugin SDK — Rust 绑定
// 插件开发者实现 GuaPlugin trait，然后用 gua_plugin_export! 宏导出

use std::ptr;

// ── C ABI 类型（与 gua-api.h 一致） ──────────────────────────

/// Gua 提供给插件的 API 表
#[repr(C)]
pub struct GuaApi {
    pub api_version: u32,
    pub struct_size: u32,
    pub register_hotkey: Option<
        unsafe extern "C" fn(mods: u32, vk: u32, user_id: i32) -> i32,
    >,
    pub unregister_hotkey: Option<unsafe extern "C" fn(user_id: i32)>,
    pub get_config: Option<
        unsafe extern "C" fn(key: *const i8, buf: *mut i8, buf_size: i32) -> i32,
    >,
    pub set_timer: Option<
        unsafe extern "C" fn(interval_ms: u32, user_id: i32) -> i32,
    >,
    pub kill_timer: Option<unsafe extern "C" fn(user_id: i32)>,
    pub log: Option<unsafe extern "C" fn(level: i32, msg: *const i8)>,
    pub hwnd: u64,
}

/// 插件填回给 Gua 的回调表
#[repr(C)]
pub struct PluginVtable {
    pub vtable_size: u32,
    pub name: *const i8,
    pub version: *const i8,
    pub init: Option<unsafe extern "C" fn() -> i32>,
    pub on_hotkey: Option<unsafe extern "C" fn(user_id: i32)>,
    pub on_tick: Option<unsafe extern "C" fn(user_id: i32)>,
    pub on_config_reload: Option<unsafe extern "C" fn()>,
    pub cleanup: Option<unsafe extern "C" fn()>,
    pub on_wndproc: Option<unsafe extern "C" fn(msg: u32, wp: u64, lp: i64) -> i32>,
}

// ── 插件 trait ───────────────────────────────────────────────

/// 插件开发者实现此 trait
pub trait GuaPlugin {
    /// 插件名称（UTF-8，静态字符串）
    const NAME: &'static str;
    /// 插件版本
    const VERSION: &'static str;

    /// 初始化，返回 0 表示成功
    fn init(&self, api: &'static GuaApi) -> i32;

    /// 热键被触发
    fn on_hotkey(&self, _user_id: i32) {}

    /// 定时器触发
    fn on_tick(&self, _user_id: i32) {}

    /// 配置热重载通知
    fn on_config_reload(&self) {}

    /// 插件卸载前清理
    fn cleanup(&self) {}

    /// 窗口消息钩子（可选），返回 true 表示已处理
    fn on_wndproc(&self, _msg: u32, _wp: u64, _lp: i64) -> bool {
        false
    }
}

// ── 导出宏 ──────────────────────────────────────────────────

/// 生成插件的 C 导出入口 `gua_plugin_load`
///
/// 用法：
/// ```ignore
/// struct MyPlugin;
///
/// impl GuaPlugin for MyPlugin {
///     const NAME: &'static str = "my-plugin";
///     const VERSION: &'static str = "0.1.0";
///     fn init(&self, api: &'static GuaApi) -> i32 { 0 }
/// }
///
/// gua_plugin_export!(MyPlugin);
/// ```
#[macro_export]
macro_rules! gua_plugin_export {
    ($plugin_type:ty) => {
        static PLUGIN: $plugin_type = $plugin_type {};
        static mut GUA_API: Option<&'static $crate::GuaApi> = None;

        #[no_mangle]
        pub unsafe extern "C" fn gua_plugin_load(
            api: *const $crate::GuaApi,
            vtable: *mut $crate::PluginVtable,
        ) -> i32 {
            if api.is_null() || vtable.is_null() {
                return -1;
            }
            let api_ref = &*api;
            GUA_API = Some(api_ref);

            let name_bytes = concat!($crate::GuaPlugin::NAME, "\0");
            let version_bytes = concat!($crate::GuaPlugin::VERSION, "\0");

            let v = &mut *vtable;
            v.vtable_size = std::mem::size_of::<$crate::PluginVtable>() as u32;
            v.name = name_bytes.as_ptr() as *const i8;
            v.version = version_bytes.as_ptr() as *const i8;
            v.init = Some(init_wrapper::<$plugin_type>);
            v.on_hotkey = Some(on_hotkey_wrapper::<$plugin_type>);
            v.on_tick = Some(on_tick_wrapper::<$plugin_type>);
            v.on_config_reload = Some(on_config_reload_wrapper::<$plugin_type>);
            v.cleanup = Some(cleanup_wrapper::<$plugin_type>);
            v.on_wndproc = Some(on_wndproc_wrapper::<$plugin_type>);
            0
        }

        unsafe extern "C" fn init_wrapper<T: $crate::GuaPlugin>() -> i32 {
            let api = GUA_API.unwrap();
            PLUGIN.init(api)
        }

        unsafe extern "C" fn on_hotkey_wrapper<T: $crate::GuaPlugin>(user_id: i32) {
            PLUGIN.on_hotkey(user_id);
        }

        unsafe extern "C" fn on_tick_wrapper<T: $crate::GuaPlugin>(user_id: i32) {
            PLUGIN.on_tick(user_id);
        }

        unsafe extern "C" fn on_config_reload_wrapper<T: $crate::GuaPlugin>() {
            PLUGIN.on_config_reload();
        }

        unsafe extern "C" fn cleanup_wrapper<T: $crate::GuaPlugin>() {
            PLUGIN.cleanup();
        }

        unsafe extern "C" fn on_wndproc_wrapper<T: $crate::GuaPlugin>(
            msg: u32, wp: u64, lp: i64,
        ) -> i32 {
            if PLUGIN.on_wndproc(msg, wp, lp) { 1 } else { 0 }
        }
    };
}

// ── 辅助函数 ─────────────────────────────────────────────────

impl GuaApi {
    /// 读取配置项
    pub fn get_config(&self, key: &str) -> Option<String> {
        let func = self.get_config?;
        let key_c = std::ffi::CString::new(key).ok()?;
        let len = unsafe { func(key_c.as_ptr(), ptr::null_mut(), 0) };
        if len < 0 {
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        let written = unsafe { func(key_c.as_ptr(), buf.as_mut_ptr() as *mut i8, len) };
        if written < 0 {
            return None;
        }
        // buf 末尾有 \0，取有效部分
        let bytes = &buf[..written as usize];
        Some(String::from_utf8_lossy(bytes).to_string())
    }

    /// 注册热键
    pub fn register_hotkey(&self, mods: u32, vk: u32, user_id: i32) -> i32 {
        match self.register_hotkey {
            Some(f) => unsafe { f(mods, vk, user_id) },
            None => -1,
        }
    }

    /// 注销热键
    pub fn unregister_hotkey(&self, user_id: i32) {
        if let Some(f) = self.unregister_hotkey {
            unsafe { f(user_id) }
        }
    }

    /// 日志输出
    pub fn log(&self, level: i32, msg: &str) {
        if let Some(f) = self.log {
            if let Ok(c) = std::ffi::CString::new(msg) {
                unsafe { f(level, c.as_ptr()) }
            }
        }
    }
}
