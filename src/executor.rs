use std::process::Command;

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
        let _ = Command::new("cmd").args(["/c", "start", "", &target]).spawn();
    }
}
