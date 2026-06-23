use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Entry {
    pub key: String,
    /// 原始值（URL / 路径等），不含描述
    pub value: String,
    /// 由 [分类] 区段头自动捕获
    pub category: Option<String>,
    /// 可选的描述文字，取自 value 后最后一个 ` | ` 分隔符
    pub description: Option<String>,
}

/// 在 value 中查找最后一个 ` | `（前后带空格的竖线）
/// 有则拆分为 (实际值, 描述)，无则返回 (原字符串, None)
fn split_description(s: &str) -> (&str, Option<&str>) {
    // 从右侧找，避免值中意外出现的 | 被误切
    if let Some(pos) = s.rfind(" | ") {
        let val = s[..pos].trim_end();
        let desc = s[pos + 3..].trim();
        let desc = if desc.is_empty() { None } else { Some(desc) };
        return (val, desc);
    }
    (s, None)
}

pub fn load(path: impl AsRef<Path>) -> Vec<Entry> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            let _ = fs::write("panic.log", format!("config: 读取配置文件失败: {e}\n"));
            return Vec::new();
        }
    };
    let content = content.trim_start_matches('\u{FEFF}');

    let mut entries = Vec::new();
    let mut current_category: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // 捕获区段头作为分类
        if line.starts_with('[') && line.ends_with(']') {
            let cat = line[1..line.len() - 1].trim();
            current_category = if cat.is_empty() { None } else { Some(cat.to_string()) };
            continue;
        }
        if let Some(pos) = line.find('=') {
            let k = line[..pos].trim();
            let v_raw = line[pos + 1..].trim();
            let (value, description) = split_description(v_raw);
            entries.push(Entry {
                key: k.to_string(),
                value: value.to_string(),
                category: current_category.clone(),
                description: description.map(|d| d.to_string()),
            });
            } else {
            eprintln!("config: 忽略无法解析的行: {line}");
            let _ = std::fs::write("panic.log", format!("config: 忽略无法解析的行: {line}\n"));
        }
    }
    entries
}

/// 从解析后的配置中提取所有 [plugin.xxx] section 的配置
/// 返回: { "vd-hotkeys": { "enabled": "true", "switch_left": "Alt+J", ... }, ... }
pub fn build_plugin_configs(entries: &[Entry]) -> HashMap<String, HashMap<String, String>> {
    let mut map: HashMap<String, HashMap<String, String>> = HashMap::new();
    for e in entries {
        if let Some(cat) = &e.category {
            if let Some(name) = cat.strip_prefix("plugin.") {
                map.entry(name.to_string())
                    .or_default()
                    .insert(e.key.clone(), e.value.clone());
            }
        }
    }
    map
}


