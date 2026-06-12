use std::process::Command;

pub fn execute(_key: &str, val: &str, query: &str) {
    let target = if !query.is_empty() && (val.starts_with("http://") || val.starts_with("https://")) {
        let encoded: String = query.replace(' ', "%20");
        format!("{}{}", val, encoded)
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
