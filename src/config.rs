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

/// 配置文件夹: %USERPROFILE%/Gua/
pub fn config_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("Gua")
}
pub fn settings_path() -> PathBuf { config_dir().join("settings.toml") }
pub fn codes_path() -> PathBuf { config_dir().join("codes.toml") }

/// 返回所有 `_xxx` 设置项的默认值（不包含多值键 _pinyin_overrides / _blacklist）
pub fn default_entries() -> Vec<Entry> {
    vec![
        Entry { key: "_font".into(),              value: "Segoe UI".into(),         category: None, description: None },
        Entry { key: "_font_size".into(),          value: "18".into(),              category: None, description: None },
        Entry { key: "_width".into(),             value: "420".into(),              category: None, description: None },
        Entry { key: "_max_results".into(),        value: "8".into(),               category: None, description: None },
        Entry { key: "_round_corner".into(),       value: "12".into(),              category: None, description: None },
        Entry { key: "_opacity".into(),            value: "255".into(),             category: None, description: None },
        Entry { key: "_case_sensitive".into(),     value: "false".into(),           category: None, description: None },
        Entry { key: "_fuzzy_match".into(),        value: "true".into(),            category: None, description: None },
        Entry { key: "_pinyin_search".into(),      value: "true".into(),            category: None, description: None },
        Entry { key: "_hide_on_focus_loss".into(), value: "true".into(),            category: None, description: None },
        Entry { key: "_theme_color".into(),        value: "#1E1E1E".into(),         category: None, description: None },
        Entry { key: "_input_bg_color".into(),     value: "#2A2A2A".into(),         category: None, description: None },
        Entry { key: "_accent_color".into(),       value: "#4A6FA5".into(),         category: None, description: None },
        Entry { key: "_text_color".into(),         value: "#CCCCCC".into(),         category: None, description: None },
        Entry { key: "_status_font_size".into(),   value: "12".into(),              category: None, description: None },
        Entry { key: "_panel_position_x".into(),   value: "50".into(),              category: None, description: None },
        Entry { key: "_panel_position_y".into(),   value: "50".into(),              category: None, description: None },
        Entry { key: "_hotkey".into(),             value: "Alt+Space".into(),       category: None, description: None },
        Entry { key: "_blacklist".into(),          value: "".into(),                category: None, description: None },
        Entry { key: "_pinyin_overrides".into(),   value: "".into(),                category: None, description: None },
    ]
}

#[allow(unused_variables)]
fn panic_log(msg: &str) {
    #[cfg(debug_assertions)] {
        let path = config_dir().join("panic.log");
        let _ = fs::write(&path, msg);
    }
}

/// 创建 config / fonts / plugins 三个目录
fn ensure_dirs() -> bool {
    let base = config_dir();
    for dir in &[&base, &base.join("fonts"), &base.join("plugins")] {
        if let Err(e) = fs::create_dir_all(dir) {
            panic_log(&format!("config: 创建目录失败 {}: {e}\n", dir.display()));
            return false;
        }
    }
    true
}

// ── 底层文件读写 ──

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

const FIELD_NAMES: [&str; 3] = ["key", "value", "description"];

fn parse_line(line: &str, cur_cat: &Option<String>, pending: &mut Option<Entry>) -> Option<Entry> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') { return None; }
    let eq_pos = line.find('=')?;
    let field = line[..eq_pos].trim();
    let raw_val = line[eq_pos + 1..].trim();
    if FIELD_NAMES.contains(&field) {
        let val = unquote(raw_val);
        if pending.is_none() {
            *pending = Some(Entry { key: String::new(), value: String::new(), category: cur_cat.clone(), description: None });
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
        Some(Entry { key: field.to_string(), value: unquote(raw_val), category: cur_cat.clone(), description: None })
    }
}

fn flush_pending(pending: &mut Option<Entry>) -> Option<Entry> {
    let e = pending.take()?;
    if e.key.is_empty() || e.value.is_empty() { None } else { Some(e) }
}

fn load_raw(path: &Path) -> Vec<Entry> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut entries: Vec<Entry> = Vec::new();
    let mut current_category: Option<String> = None;
    let mut pending: Option<Entry> = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if let Some(e) = flush_pending(&mut pending) { entries.push(e); }
            let cat = trimmed[1..trimmed.len() - 1].trim();
            current_category = if cat.is_empty() { None } else { Some(cat.to_string()) };
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            if let Some(e) = flush_pending(&mut pending) { entries.push(e); }
            continue;
        }
        if let Some(e) = parse_line(trimmed, &current_category, &mut pending) {
            entries.push(e);
        }
    }
    if let Some(e) = flush_pending(&mut pending) { entries.push(e); }
    entries
}

fn save_raw(path: &Path, entries: &[Entry]) {
    if !ensure_dirs() { return; }
    let tmp_path = path.with_extension("tmp");
    let mut out = match fs::File::create(&tmp_path) {
        Ok(f) => f,
        Err(e) => { panic_log(&format!("config: 创建临时文件失败: {e}\n")); return; }
    };

    let mut groups: Vec<(Option<String>, Vec<&Entry>)> = Vec::new();
    for e in entries {
        let last = groups.last_mut();
        if let Some(&mut (ref cat, ref mut list)) = last {
            if *cat == e.category { list.push(e); continue; }
        }
        groups.push((e.category.clone(), vec![e]));
    }

    for (cat, group) in &groups {
        if let Some(ref name) = cat { let _ = writeln!(out, "\n[{}]", name); }
        for e in group {
            if e.key.starts_with('_') || e.description.is_none() {
                let _ = writeln!(out, "{} = \"{}\"", e.key, e.value);
            } else {
                let _ = writeln!(out);
                let _ = writeln!(out, "key = \"{}\"", e.key);
                let _ = writeln!(out, "value = \"{}\"", e.value);
                if let Some(ref desc) = e.description { let _ = writeln!(out, "description = \"{}\"", desc); }
            }
        }
    }

    let _ = out.flush();
    if let Err(e) = out.sync_all() { panic_log(&format!("config: sync 失败: {e}\n")); return; }
    drop(out);
    if let Err(e) = fs::rename(&tmp_path, path) { panic_log(&format!("config: rename 失败: {e}\n")); }
}

// ── 设置文件 (settings.toml) ──

/// 读取设置文件。文件不存在时自动用默认值创建；解析后增量补充缺失的默认项。
pub fn load_settings() -> Vec<Entry> {
    ensure_dirs();
    let path = settings_path();

    if !path.is_file() {
        let entries = default_entries();
        save_raw(&path, &entries);
        return entries;
    }

    let mut entries = load_raw(&path);

    let defaults = default_entries();
    let mut changed = false;
    for def in &defaults {
        if !entries.iter().any(|e| e.key == def.key) {
            entries.push(def.clone());
            changed = true;
        }
    }
    if changed {
        save_raw(&path, &entries);
    }

    entries
}

pub fn save_settings(entries: &[Entry]) {
    ensure_dirs();
    save_raw(&settings_path(), entries);
}

// ── 识别码文件 (codes.toml) ──

pub fn load_codes() -> Vec<Entry> {
    ensure_dirs();
    let path = codes_path();
    if !path.is_file() {
        save_raw(&path, &[]);
    }
    load_raw(&path)
}

pub fn save_codes(entries: &[Entry]) {
    ensure_dirs();
    save_raw(&codes_path(), entries);
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
