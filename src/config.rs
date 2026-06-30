use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Entry {
    pub key: String,
    pub value: String,
    pub category: Option<String>,
    pub description: Option<String>,
}

/// 配置文件的存放路径: %USERPROFILE%/Gua/config.toml
pub fn config_path() -> PathBuf {
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(home).join("Gua");
    let _ = fs::create_dir_all(&dir);
    dir.join("config.toml")
}

/// 解析引号包裹的值: `"hello"` → `hello`, `hello` → `hello`
fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// 已知的字段名（多行记录专用）
const FIELD_NAMES: [&str; 3] = ["key", "value", "description"];

/// 从内容中解析一行或累积多行，返回解析出的 Entry（短格式直接返回，长格式需累积）
fn parse_line(line: &str, cur_cat: &Option<String>, pending: &mut Option<Entry>) -> Option<Entry> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let eq_pos = line.find('=')?;
    let field = line[..eq_pos].trim();
    let raw_val = line[eq_pos + 1..].trim();

    if FIELD_NAMES.contains(&field) {
        // 多行记录的字段
        let val = unquote(raw_val);
        if pending.is_none() {
            *pending = Some(Entry {
                key: String::new(),
                value: String::new(),
                category: cur_cat.clone(),
                description: None,
            });
        }
        if let Some(ref mut e) = pending {
            match field {
                "key" => e.key = val,
                "value" => e.value = val,
                "description" => e.description = if val.is_empty() { None } else { Some(val) },
                _ => {}
            }
        }
        None
    } else {
        // 短格式: key = "value"
        Some(Entry {
            key: field.to_string(),
            value: unquote(raw_val),
            category: cur_cat.clone(),
            description: None,
        })
    }
}

/// 将累积的多行记录 flush 为 Entry（缺少 key 或 value 时丢弃）
fn flush_pending(pending: &mut Option<Entry>) -> Option<Entry> {
    let e = pending.take()?;
    if e.key.is_empty() || e.value.is_empty() {
        return None;
    }
    Some(e)
}

pub fn load(path: impl AsRef<Path>) -> Vec<Entry> {
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut entries: Vec<Entry> = Vec::new();
    let mut current_category: Option<String> = None;
    let mut pending: Option<Entry> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        // 区段头
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if let Some(e) = flush_pending(&mut pending) {
                entries.push(e);
            }
            let cat = trimmed[1..trimmed.len() - 1].trim();
            current_category = if cat.is_empty() { None } else { Some(cat.to_string()) };
            continue;
        }

        // 空行 / 注释：flush pending
        if trimmed.is_empty() || trimmed.starts_with('#') {
            if let Some(e) = flush_pending(&mut pending) {
                entries.push(e);
            }
            continue;
        }

        // 尝试解析
        if let Some(e) = parse_line(trimmed, &current_category, &mut pending) {
            // 短格式直接产出完整 entry
            entries.push(e);
        }
    }

    // 文件尾 flush
    if let Some(e) = flush_pending(&mut pending) {
        entries.push(e);
    }

    entries
}

/// 原子写入: 先写 .tmp，完成后 rename 覆盖原文件
pub fn save(path: impl AsRef<Path>, entries: &[Entry]) {
    let tmp_path = path.as_ref().with_extension("tmp");
    let mut out = match fs::File::create(&tmp_path) {
        Ok(f) => f,
        Err(e) => {
            let _ = fs::write("panic.log", format!("config: 创建临时文件失败: {e}\n"));
            return;
        }
    };

    // 按分类分组
    let mut groups: Vec<(Option<String>, Vec<&Entry>)> = Vec::new();
    for e in entries {
        let last = groups.last_mut();
        if let Some(&mut (ref cat, ref mut list)) = last {
            if *cat == e.category {
                list.push(e);
                continue;
            }
        }
        groups.push((e.category.clone(), vec![e]));
    }

    for (cat, group) in &groups {
        // 写入分类头
        if let Some(ref name) = cat {
            let _ = writeln!(out, "\n[{}]", name);
        }
        for e in group {
            if e.key.starts_with('_') || e.description.is_none() {
                // 设置项或没有描述 → 短格式
                let _ = writeln!(out, "{} = \"{}\"", e.key, e.value);
            } else {
                // 有描述 → 多行记录
                let _ = writeln!(out);
                let _ = writeln!(out, "key = \"{}\"", e.key);
                let _ = writeln!(out, "value = \"{}\"", e.value);
                if let Some(ref desc) = e.description {
                    let _ = writeln!(out, "description = \"{}\"", desc);
                }
            }
        }
    }

    // 确保落盘
    let _ = out.flush();
    if let Err(e) = out.sync_all() {
        let _ = fs::write("panic.log", format!("config: sync 失败: {e}\n"));
        return;
    }
    drop(out);

    // 原子替换
    if let Err(e) = fs::rename(&tmp_path, path.as_ref()) {
        let _ = fs::write("panic.log", format!("config: rename 失败: {e}\n"));
    }
}

/// 从解析后的配置中提取所有 [plugin.xxx] section 的配置
pub fn build_plugin_configs(entries: &[Entry]) -> HashMap<String, HashMap<String, String>> {
    let mut map: HashMap<String, HashMap<String, String>> = HashMap::new();
    for e in entries {
        if let Some(cat) = &e.category {
            if let Some(name) = cat.strip_prefix("plugin.") {
                map.entry(name.to_string())
                    .or_default()
                    .insert(e.key.trim_start_matches('_').to_string(), e.value.clone());
            }
        }
    }
    map
}
