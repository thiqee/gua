use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Entry {
    pub key: String,
    pub value: String,
}

pub fn load(path: impl AsRef<Path>) -> Vec<Entry> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut entries = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            continue;
        }
        if let Some(pos) = line.find('=') {
            let k = line[..pos].trim();
            let v = line[pos + 1..].trim();
            entries.push(Entry {
                key: k.to_string(),
                value: v.to_string(),
            });
        } else if !line.starts_with('[') {
            eprintln!("config: 忽略无法解析的行: {line}");
        }
    }
    entries
}


