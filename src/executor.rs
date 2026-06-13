use std::process::Command;
use std::ptr;

use windows::core::{w, PCWSTR};
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
    let target = if !query.is_empty() && (val.starts_with("http://") || val.starts_with("https://")) {
        format!("{}{}", val, url_encode(query))
    } else {
        val.to_string()
    };
    if target.starts_with("http://") || target.starts_with("https://") {
        let _ = Command::new("cmd").args(["/c", "start", &target]).spawn();
    } else if target.ends_with(".exe") {
        let _ = Command::new(&target).spawn();
    } else {
        let t = to_w(&target);
        unsafe {
            let _ = ShellExecuteW(None, w!("open"), pcwstr(&t), PCWSTR(ptr::null()), PCWSTR(ptr::null()), SW_SHOWNORMAL);
        }
    }
}
