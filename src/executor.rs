use std::ptr;

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::state::{to_w, pcwstr};

fn url_encode(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            b' ' => result.push_str("%20"),
            _ => result.push_str(&format!("%{:02X}", byte)),
        }
    }
    result
}

pub fn execute(_key: &str, val: &str, query: &str) {
    // 反引号包裹的命令：去掉首尾 `，用 CreateProcessW 执行（自动解析程序+参数）
    if val.len() >= 2 && val.starts_with('`') && val.ends_with('`') {
        let cmd = &val[1..val.len() - 1];
        let mut cmd_line = to_w(cmd);
        unsafe {
            let mut si = STARTUPINFOW::default();
            si.cb = size_of::<STARTUPINFOW>() as u32;
            let mut pi = PROCESS_INFORMATION::default();
            let _ = CreateProcessW(
                PCWSTR::null(),                           // lpApplicationName
                Some(PWSTR(cmd_line.as_mut_ptr())),      // lpCommandLine
                None,                                     // lpProcessAttributes
                None,                                     // lpThreadAttributes
                false,                                    // bInheritHandles
                NORMAL_PRIORITY_CLASS,                           // dwCreationFlags
                None,                                     // lpEnvironment
                PCWSTR::null(),                           // lpCurrentDirectory
                &si as *const STARTUPINFOW,
                &mut pi as *mut PROCESS_INFORMATION,
            );
            let _ = CloseHandle(pi.hThread);
            let _ = CloseHandle(pi.hProcess);
        }
        return;
    }

    // 原有逻辑：搜索引擎
    let target = if !query.is_empty() && (val.starts_with("http://") || val.starts_with("https://")) {
        format!("{}{}", val, url_encode(query))
    } else {
        val.to_string()
    };
    let t = to_w(&target);
    unsafe {
        let _ = ShellExecuteW(None, w!("open"), pcwstr(&t), PCWSTR(ptr::null()), PCWSTR(ptr::null()), SW_SHOWNORMAL);
    }
}
