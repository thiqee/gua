// Gua — 插件加载器
// 扫描 plugins/ 目录，LoadLibrary 加载 DLL，通过 C ABI 管理插件生命周期

use std::cell::{Cell, UnsafeCell};
use std::collections::HashMap;
use std::ffi::CStr;
use std::fs;
use std::io::Write;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::config;
use crate::state::to_w;

fn plog(msg: &str) {
    let path = config::config_dir().join("panic.log");
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "plugin: {msg}");
    }
}

// ── Win32 extern ──────────────────────────────────────────────

type HMODULE = *mut std::ffi::c_void;
type FARPROC = Option<unsafe extern "system" fn() -> isize>;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(lpLibFileName: *const u16) -> HMODULE;
    fn GetProcAddress(hModule: HMODULE, lpProcName: *const u8) -> FARPROC;
    fn FreeLibrary(hLibModule: HMODULE) -> i32;
}

use crate::state::{RegisterHotKey, UnregisterHotKey};

// ── 常量 ──────────────────────────────────────────────────────

const PLUGIN_HOTKEY_BASE: i32 = 1000;
const MAX_HOTKEYS_PER_PLUGIN: i32 = 64;

// ── C ABI 类型定义 ────────────────────────────────────────────

/// Gua 提供给插件的 API 表（版本 1）
/// 新字段只追加在末尾，不修改已有字段
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
    /// level: 0=info, 1=warn, 2=error; msg 指针只在调用期间有效，Gua 内部立即复制
    pub log: Option<unsafe extern "C" fn(level: i32, msg: *const i8)>,
    /// Gua 主窗口 HWND（插件可发消息或做 Win32 操作）
    pub hwnd: u64,
}

/// 插件填回给 Gua 的回调表（版本 1）
/// 新字段只追加在末尾，不修改已有字段
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
    /// 返回 true 表示该消息已被插件处理
    pub on_wndproc: Option<unsafe extern "C" fn(msg: u32, wp: u64, lp: i64) -> i32>,
}

type GuaPluginLoadFn = unsafe extern "C" fn(api: *const GuaApi, vtable: *mut PluginVtable) -> i32;

// ── 内部状态 ──────────────────────────────────────────────────

struct LoadedPlugin {
    lib: HMODULE,
    vtable: PluginVtable,
    name: String,
    /// user_id → internal_hotkey_id
    user_to_internal: HashMap<i32, i32>,
    /// internal_hotkey_id → user_id
    internal_to_user: HashMap<i32, i32>,
}

use windows::Win32::Foundation::HWND;

/// 单线程 UnsafeCell 包装（无运行时借用检查）
/// 安全前提：全局状态只在一个线程（主线程消息循环）中访问
///
/// # Safety
/// Sync 是无条件实现的，因为 ST 的所有公开访问方法（r/w）都是 unsafe，
/// 调用方必须自行保证线程安全。T 中通常包含 !Send/!Sync 的裸指针，
/// 但在单线程架构下这是安全的。
struct ST<T>(UnsafeCell<T>);
unsafe impl<T> Sync for ST<T> {}

impl<T> ST<T> {
    const fn new(val: T) -> Self { ST(UnsafeCell::new(val)) }
    /// 获取不可变引用（单线程安全，无并发写入）
    unsafe fn r(&self) -> &T { &*self.0.get() }
    /// 获取可变引用（单线程安全，无并发读取）
    unsafe fn w(&self) -> &mut T { &mut *self.0.get() }
}

// 全局状态（单线程，只在主线程访问）
static PLUGINS: ST<Vec<LoadedPlugin>> = ST::new(Vec::new());
static PLUGIN_CONFIGS: ST<Option<HashMap<String, HashMap<String, String>>>> = ST::new(None);
static TIMER_MAP: ST<Option<HashMap<i32, (usize, i32)>>> = ST::new(None);
static GUA_HWND: AtomicUsize = AtomicUsize::new(0);

#[allow(static_mut_refs)]
static mut GUA_API: GuaApi = GuaApi {
    api_version: 1,
    struct_size: std::mem::size_of::<GuaApi>() as u32,
    register_hotkey: Some(register_hotkey_impl),
    unregister_hotkey: Some(unregister_hotkey_impl),
    get_config: Some(get_config_impl),
    set_timer: Some(set_timer_impl),
    kill_timer: Some(kill_timer_impl),
    log: Some(log_impl),
    hwnd: 0,
};

fn gua_hwnd() -> HWND {
    HWND(GUA_HWND.load(Ordering::Relaxed) as *mut std::ffi::c_void)
}

thread_local! {
    static CURRENT_PLUGIN_IDX: Cell<usize> = Cell::new(usize::MAX);
}

// ── GuaApi 实现 ───────────────────────────────────────────────

unsafe extern "C" fn register_hotkey_impl(mods: u32, vk: u32, user_id: i32) -> i32 {
    let idx = CURRENT_PLUGIN_IDX.get();
    if idx == usize::MAX {
        plog("register_hotkey: 不在插件上下文中");
        return -1;
    }
    if user_id < 0 || user_id >= MAX_HOTKEYS_PER_PLUGIN {
        plog(&format!("register_hotkey: user_id {} 超出范围", user_id));
        return -1;
    }
    {
        let plugins = PLUGINS.r();
        if plugins[idx].user_to_internal.contains_key(&user_id) {
            return user_id;
        }
    }
    let internal_id = PLUGIN_HOTKEY_BASE + idx as i32 * MAX_HOTKEYS_PER_PLUGIN + user_id;
    if !RegisterHotKey(gua_hwnd(), internal_id, mods, vk).as_bool() {
        plog(&format!("register_hotkey: RegisterHotKey 失败 mods={} vk={}", mods, vk));
        return -1;
    }
    plog(&format!("register_hotkey: 成功 mods={} vk={} user_id={} internal_id={}", mods, vk, user_id, internal_id));
    let plugins = PLUGINS.w();
    plugins[idx].user_to_internal.insert(user_id, internal_id);
    plugins[idx].internal_to_user.insert(internal_id, user_id);
    user_id
}

unsafe extern "C" fn unregister_hotkey_impl(user_id: i32) {
    let idx = CURRENT_PLUGIN_IDX.get();
    if idx == usize::MAX {
        return;
    }
    let plugins = PLUGINS.w();
    if let Some(&internal_id) = plugins[idx].user_to_internal.get(&user_id) {
        let _ = UnregisterHotKey(gua_hwnd(), internal_id).as_bool();
        plugins[idx].user_to_internal.remove(&user_id);
        plugins[idx].internal_to_user.remove(&internal_id);
    }
}

unsafe extern "C" fn get_config_impl(key: *const i8, buf: *mut i8, buf_size: i32) -> i32 {
    let idx = CURRENT_PLUGIN_IDX.get();
    if idx == usize::MAX || key.is_null() {
        return -1;
    }
    let plugin_name = PLUGINS.r()[idx].name.clone();
    let key_str = match CStr::from_ptr(key).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let configs = PLUGIN_CONFIGS.r();
    let configs = match configs.as_ref() {
        Some(c) => c,
        None => return -1,
    };
    let val = configs
        .get(&plugin_name)
        .and_then(|cfg| cfg.get(key_str))
        .map(|s| s.as_str());
    let val = match val {
        Some(v) => v,
        None => return -1,
    };
    let bytes = val.as_bytes();
    let len = bytes.len();
    if buf.is_null() {
        return (len + 1) as i32;
    }
    let needed = len + 1;
    if (buf_size as usize) < needed {
        return -2;
    }
    ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, len);
    *buf.add(len) = 0;
    len as i32
}

unsafe extern "C" fn set_timer_impl(interval_ms: u32, user_id: i32) -> i32 {
    let idx = CURRENT_PLUGIN_IDX.get();
    if idx == usize::MAX || user_id < 0 || user_id >= MAX_HOTKEYS_PER_PLUGIN {
        return -1;
    }
    let timer_id = PLUGIN_HOTKEY_BASE + idx as i32 * MAX_HOTKEYS_PER_PLUGIN + 256 + user_id;
    if SetTimer(gua_hwnd(), timer_id as usize, interval_ms, None) == 0 {
        return -1;
    }
    if let Some(ref mut map) = *TIMER_MAP.w() {
        map.insert(timer_id, (idx, user_id));
    }
    user_id
}

unsafe extern "C" fn kill_timer_impl(user_id: i32) {
    let idx = CURRENT_PLUGIN_IDX.get();
    if idx == usize::MAX || user_id < 0 || user_id >= MAX_HOTKEYS_PER_PLUGIN {
        return;
    }
    let timer_id = PLUGIN_HOTKEY_BASE + idx as i32 * MAX_HOTKEYS_PER_PLUGIN + 256 + user_id;
    let _ = KillTimer(gua_hwnd(), timer_id as usize);
    if let Some(ref mut map) = *TIMER_MAP.w() {
        map.remove(&timer_id);
    }
}

#[link(name = "user32")]
extern "system" {
    fn SetTimer(hwnd: HWND, nIDEvent: usize, uElapse: u32, lpTimerFunc: Option<unsafe extern "system" fn()>) -> usize;
    fn KillTimer(hwnd: HWND, uIDEvent: usize) -> i32;
}

unsafe extern "C" fn log_impl(level: i32, msg: *const i8) {
    if msg.is_null() {
        return;
    }
    let s = CStr::from_ptr(msg).to_string_lossy();
    let prefix = match level {
        0 => "[plugin info]",
        1 => "[plugin warn]",
        2 => "[plugin error]",
        _ => "[plugin]",
    };
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("plugin.log")
        .and_then(|mut f| std::io::Write::write_fmt(
            &mut f,
            format_args!("{} {}\n", prefix, s),
        ));
}

// ── 公共 API ──────────────────────────────────────────────────

/// 加载所有插件（在窗口创建后、消息循环前调用）
///
/// # Safety
/// - `hwnd` 必须是有效的窗口句柄
/// - 需在消息循环启动前调用，且只调用一次
pub unsafe fn load_all(
    hwnd: HWND,
    plugin_configs: &HashMap<String, HashMap<String, String>>,
) {
    GUA_HWND.store(hwnd.0 as usize, Ordering::Relaxed);
    unload_current_plugins();
    *PLUGIN_CONFIGS.w() = Some(plugin_configs.clone());
    *TIMER_MAP.w() = Some(HashMap::new());
    *PLUGINS.w() = Vec::new();
    GUA_API.hwnd = hwnd.0 as u64;

    let plugin_dir = config::config_dir().join("plugins");
    if !plugin_dir.is_dir() {
        return;
    }

    let mut entries: Vec<_> = match std::fs::read_dir(&plugin_dir) {
        Ok(iter) => iter.flatten().collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name());

    let configs = PLUGIN_CONFIGS.r();
    let configs = configs.as_ref().unwrap();

    for entry in &entries {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("dll") {
            continue;
        }
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if file_stem.is_empty() {
            continue;
        }

        plog(&format!("发现DLL: {}", file_stem));
        let full_path = to_w(&path.to_string_lossy());
        let lib = LoadLibraryW(full_path.as_ptr());
        if lib.is_null() {
            plog(&format!("{}: LoadLibraryW 失败", file_stem));
            continue;
        }
        plog(&format!("{}: LoadLibraryW 成功", file_stem));

        let load_fn_name = b"gua_plugin_load\0";
        let proc = GetProcAddress(lib, load_fn_name.as_ptr());
        if proc.is_none() {
            plog(&format!("{}: 缺少 gua_plugin_load 导出", file_stem));
            let _ = FreeLibrary(lib);
            continue;
        }
        plog(&format!("{}: 找到 gua_plugin_load", file_stem));
        let load_fn: GuaPluginLoadFn = std::mem::transmute(proc.unwrap());

        // GuaApi 指针（静态分配，零泄漏）
        let gua_api = &GUA_API as *const GuaApi;

        let mut vtable = PluginVtable {
            vtable_size: 0,
            name: ptr::null(),
            version: ptr::null(),
            init: None,
            on_hotkey: None,
            on_tick: None,
            on_config_reload: None,
            cleanup: None,
            on_wndproc: None,
        };

        let idx = PLUGINS.r().len();
        CURRENT_PLUGIN_IDX.set(idx);
        plog(&format!("{}: 调用 gua_plugin_load...", file_stem));

        let ret = load_fn(gua_api as *const GuaApi, &mut vtable as *mut PluginVtable);

        CURRENT_PLUGIN_IDX.set(usize::MAX);
        plog(&format!("{}: gua_plugin_load 返回 ret={}", file_stem, ret));

        if ret != 0 || vtable.name.is_null() {
            plog(&format!("{}: 初始化失败 (ret={})", file_stem, ret));
            let _ = FreeLibrary(lib);
            continue;
        }

        let name_str = CStr::from_ptr(vtable.name).to_string_lossy().to_string();
        plog(&format!("{}: 名称=\"{}\" has_init={}", file_stem, name_str, vtable.init.is_some()));
        if vtable.vtable_size > std::mem::size_of::<PluginVtable>() as u32 {
            vtable.vtable_size = std::mem::size_of::<PluginVtable>() as u32;
        }

        // 用 vtable 中的 name 检查 enabled 开关（config section 名应与插件声明的 name 一致）
        if configs
            .get(&name_str)
            .and_then(|c| c.get("enabled"))
            .map_or(false, |v| v == "false")
        {
            plog(&format!("{} 已禁用", name_str));
            let _ = FreeLibrary(lib);
            CURRENT_PLUGIN_IDX.set(usize::MAX);
            continue;
        }

        PLUGINS.w().push(LoadedPlugin {
            lib,
            vtable,
            name: name_str.clone(),
            user_to_internal: HashMap::new(),
            internal_to_user: HashMap::new(),
        });

        CURRENT_PLUGIN_IDX.set(usize::MAX);

        // 调用 init
        CURRENT_PLUGIN_IDX.set(idx);
        plog(&format!("{}: 调用 init...", name_str));
        let init_result = std::panic::catch_unwind(|| {
            let init_fn = PLUGINS.r()[idx].vtable.init;
            if let Some(f) = init_fn {
                f()
            } else {
                0
            }
        });
        CURRENT_PLUGIN_IDX.set(usize::MAX);

        match init_result {
            Ok(0) => { plog(&format!("{}: init 成功", name_str)); }
            Ok(r) => {
                plog(&format!("{}: init 返回非零 {}", name_str, r));
                cleanup_plugin(idx);
                continue;
            }
            Err(_) => {
                plog(&format!("{}: init 发生 panic", name_str));
                cleanup_plugin(idx);
                continue;
            }
        }
    }

    let count = PLUGINS.r().len();
    if count > 0 {
        plog(&format!("已加载 {} 个插件", count));
    }
}

/// 卸载所有插件（在消息循环退出后、GdiplusShutdown 前调用）
///
/// # Safety
/// - 需在窗口销毁后、进程退出前调用，只调用一次
pub unsafe fn unload_all() {
    unload_current_plugins();
    PLUGINS.w().clear();
}

/// 分发热键到对应的插件
///
/// # Safety
/// - `internal_id` 必须来自 `is_plugin_hotkey` 验证的 ID
/// - 需在插件已加载后调用
pub unsafe fn dispatch_hotkey(internal_id: i32) -> bool {
    let plugins = PLUGINS.r();
    for i in 0..plugins.len() {
        if plugins[i].internal_to_user.contains_key(&internal_id) {
            let user_id = plugins[i].internal_to_user[&internal_id];
            plog(&format!("dispatch_hotkey: 插件[{}] user_id={}", i, user_id));
            let f = plugins[i].vtable.on_hotkey;
            CURRENT_PLUGIN_IDX.set(i);
            let _ = std::panic::catch_unwind(|| {
                if let Some(f) = f {
                    f(user_id);
                }
            });
            CURRENT_PLUGIN_IDX.set(usize::MAX);
            return true;
        }
    }
    false
}

/// 通知所有插件配置已重载
///
/// # Safety
/// - 需在插件已加载后调用
pub unsafe fn notify_reload(configs: &HashMap<String, HashMap<String, String>>) {
    *PLUGIN_CONFIGS.w() = Some(configs.clone());
    let count = PLUGINS.r().len();
    for i in 0..count {
        CURRENT_PLUGIN_IDX.set(i);
        let f = PLUGINS.r()[i].vtable.on_config_reload;
        let _ = std::panic::catch_unwind(|| {
            if let Some(f) = f {
                f();
            }
        });
        CURRENT_PLUGIN_IDX.set(usize::MAX);
    }
}

/// 分发窗口消息给插件，返回 true 表示插件已处理
///
/// # Safety
/// - 需在插件已加载后调用
pub unsafe fn dispatch_wndproc(msg: u32, wp: u64, lp: i64) -> bool {
    // WM_TIMER: 通过 TIMER_MAP 查表路由到对应插件的 on_tick
    if msg == 0x0113 /* WM_TIMER */ {
        let timer_id = wp as i32;
        let entry = TIMER_MAP.r().as_ref()
            .and_then(|m| m.get(&timer_id).copied());
        if let Some((idx, user_id)) = entry {
            let plugins = PLUGINS.r();
            if idx < plugins.len() {
                if let Some(f) = plugins[idx].vtable.on_tick {
                    CURRENT_PLUGIN_IDX.set(idx);
                    let _ = std::panic::catch_unwind(|| f(user_id));
                    CURRENT_PLUGIN_IDX.set(usize::MAX);
                    return true;
                }
            }
        }
        return false;
    }

    // 已有逻辑：遍历所有插件调 on_wndproc
    for i in 0..PLUGINS.r().len() {
        if let Some(f) = PLUGINS.r()[i].vtable.on_wndproc {
            CURRENT_PLUGIN_IDX.set(i);
            let handled = std::panic::catch_unwind(|| f(msg, wp, lp));
            CURRENT_PLUGIN_IDX.set(usize::MAX);
            if let Ok(1) = handled {
                return true;
            }
        }
    }
    false
}

/// 判断 hotkey_id 是否属于插件范围
pub fn is_plugin_hotkey(hotkey_id: i32) -> bool {
    hotkey_id >= PLUGIN_HOTKEY_BASE
}

// ── 内部辅助 ──────────────────────────────────────────────────

unsafe fn unload_current_plugins() {
    for i in (0..PLUGINS.r().len()).rev() {
        let cleanup_fn = PLUGINS.r()[i].vtable.cleanup;
        let _ = std::panic::catch_unwind(|| {
            if let Some(f) = cleanup_fn { f(); }
        });
        unregister_all_for_plugin(i);
        let _ = FreeLibrary(PLUGINS.r()[i].lib);
    }
}

unsafe fn cleanup_plugin(idx: usize) {
    unregister_all_for_plugin(idx);
    let lib = PLUGINS.r()[idx].lib;
    let _ = FreeLibrary(lib);
    PLUGINS.w().remove(idx);
}

unsafe fn unregister_all_for_plugin(idx: usize) {
    let ids: Vec<i32> = PLUGINS.r()[idx].internal_to_user.keys().copied().collect();
    for id in &ids {
        let _ = UnregisterHotKey(gua_hwnd(), *id).as_bool();
    }
    let plugins = PLUGINS.w();
    plugins[idx].user_to_internal.clear();
    plugins[idx].internal_to_user.clear();
    if let Some(ref mut map) = *TIMER_MAP.w() {
        map.retain(|_, &mut (i, _)| i != idx);
    }
}

/// 获取 gua.exe 所在目录
fn get_exe_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}
