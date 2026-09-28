// Gua — 开机自启动：写 HKCU\Software\Microsoft\Windows\CurrentVersion\Run

use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteValueW, RegSetValueExW,
};

use crate::plugin::plog;
use crate::state::to_w;

/// 注册表 Run 子键
const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// 注册表值名（任务管理器「启动」页显示的名称）
const VALUE_NAME: &str = "Gua";

/// 当前进程可执行文件的完整路径
fn exe_path() -> Option<String> {
    let mut buf = [0u16; 1024];
    let n = unsafe { GetModuleFileNameW(None, &mut buf) };
    if n == 0 || n as usize >= buf.len() {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..n as usize]))
}

/// 开启/关闭开机自启动，返回 Err 表示注册表操作失败
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let sub = to_w(RUN_SUBKEY);
    let name = to_w(VALUE_NAME);

    unsafe {
        let mut hkey = HKEY(std::ptr::null_mut());
        let rc = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut hkey,
            None,
        );
        if rc != ERROR_SUCCESS {
            return Err(format!("打开注册表 Run 项失败，错误码 {}", rc.0));
        }

        let mut fail_msg: Option<String> = None;
        if enabled {
            match exe_path() {
                Some(path) => {
                    // 值形如 "E:\path\gua.exe"，带引号以兼容含空格的路径
                    let value = to_w(&format!("\"{path}\""));
                    let bytes = std::slice::from_raw_parts(
                        value.as_ptr() as *const u8,
                        value.len() * std::mem::size_of::<u16>(),
                    );
                    let rc =
                        RegSetValueExW(hkey, PCWSTR(name.as_ptr()), None, REG_SZ, Some(bytes));
                    if rc != ERROR_SUCCESS {
                        fail_msg = Some(format!("写入注册表失败，错误码 {}", rc.0));
                    }
                }
                None => fail_msg = Some("无法获取程序路径".to_string()),
            }
        } else {
            let rc = RegDeleteValueW(hkey, PCWSTR(name.as_ptr()));
            // 值本来就不存在时，视为「关闭」成功
            if rc != ERROR_SUCCESS && rc != ERROR_FILE_NOT_FOUND {
                fail_msg = Some(format!("删除注册表项失败，错误码 {}", rc.0));
            }
        }

        let _ = RegCloseKey(hkey);

        match fail_msg {
            Some(m) => Err(m),
            None => Ok(()),
        }
    }
}

/// 按配置同步自启动项（失败只记日志，不打断主流程）
pub fn apply(enabled: bool) {
    if let Err(e) = set_enabled(enabled) {
        plog(&format!("autostart: {e}"));
    }
}
