//! 统一配置管理器
//!
//! 实现全局与本地项目配置的 Get/Set/Unset/Reset 操作，并支持环境变量覆盖。
//! 修改配置时会通过 `toml_edit` 保留文件原有的注释与格式。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Value};
use lazy_static::lazy_static;

use crate::core::plugin_config_loader::PluginConfigLoader;

lazy_static! {
    /// 默认配置键值对定义
    static ref DEFAULT_CONFIG: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("general.preset", "balanced");
        m.insert("general.project_type", "");
        m.insert("general.framework", "");
        m.insert("general.package_manager", "");
        m.insert("compression.reorder", "true");
        m.insert("compression.ai_export", "false");
        m.insert("compression.preset", "balanced");
        m.insert("encoding.force_utf8", "true");
        m.insert("plugins.plugin_chain", "true");
        m.insert("token_optimizer.enabled", "true");
        m
    };
}

/// 配置作用域：全局配置或本地项目配置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigScope {
    /// 全局配置：Windows 下为 `%APPDATA%/tokenslim/config.toml`，Unix 下为 `~/.config/tokenslim/config.toml`
    Global,
    /// 本地配置：项目根目录下的 `.tokenslim.toml`
    Local,
}

/// Schema 支持的数据类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaType {
    String,
    Bool,
}

/// 获取配置项的 Schema 类型
pub fn get_schema_type(key: &str) -> Result<SchemaType, String> {
    match key {
        "general.preset" | "compression.preset" => Ok(SchemaType::String),
        "general.project_type" | "general.framework" | "general.package_manager" => Ok(SchemaType::String),
        "compression.reorder"
        | "compression.ai_export"
        | "encoding.force_utf8"
        | "plugins.plugin_chain"
        | "token_optimizer.enabled" => Ok(SchemaType::Bool),
        _ if key.starts_with("plugins.") && key.ends_with(".enabled") => {
            let parts: Vec<&str> = key.split('.').collect();
            if parts.len() == 3 {
                let plugin_name = parts[1];
                if is_valid_plugin(plugin_name) {
                    Ok(SchemaType::Bool)
                } else {
                    Err(format!("插件 '{}' 不存在", plugin_name))
                }
            } else {
                Err(format!("无效的插件配置键: {}", key))
            }
        }
        _ => Err(format!("未知的配置键: {}", key)),
    }
}

/// 检查插件是否存在（通过扫描 `config/plugins` 目录中的 json 文件）
fn is_valid_plugin(name: &str) -> bool {
    let config_dir = PluginConfigLoader::find_config_dir();
    if !config_dir.exists() {
        // Fallback 支持内置的插件名称以防配置目录不存在
        let builtins = [
            "shell", "access_log", "data_struct", "vcs", "build", "error_trace",
            "git", "cargo", "npm", "go", "python", "docker", "maven", "gradle",
            "pytest", "jest", "webpack", "tsc"
        ];
        return builtins.contains(&name);
    }
    if let Ok(entries) = fs::read_dir(config_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem == name {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// 校验配置项的值是否合法
pub fn validate_value(key: &str, value: &str) -> Result<(), String> {
    let schema_type = get_schema_type(key)?;
    match schema_type {
        SchemaType::Bool => {
            if value != "true" && value != "false" {
                return Err(format!(
                    "键 '{}' 的值必须为布尔值 (true/false)，当前为: '{}'",
                    key, value
                ));
            }
        }
        SchemaType::String => {
            if key == "general.preset" || key == "compression.preset" {
                if value != "fast" && value != "balanced" && value != "ai" {
                    return Err(format!(
                        "preset 的值必须是 fast, balanced 或 ai 之一，当前为: '{}'",
                        value
                    ));
                }
            }
        }
    }
    Ok(())
}

/// 获取全局配置文件路径
pub fn global_config_path() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let mut p = PathBuf::from(appdata);
            p.push("tokenslim");
            p.push("config.toml");
            return Some(p);
        }
    }
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        let mut p = PathBuf::from(home);
        p.push(".config");
        p.push("tokenslim");
        p.push("config.toml");
        return Some(p);
    }
    None
}

/// 获取本地项目配置文件路径（向上递归查找包含 `.git` 文件夹或 `.tokenslim.toml` 文件的根目录）
pub fn local_config_path() -> Option<PathBuf> {
    if let Ok(mut dir) = std::env::current_dir() {
        loop {
            let candidate = dir.join(".tokenslim.toml");
            if candidate.exists() {
                return Some(candidate);
            }
            if dir.join(".git").exists() {
                return Some(candidate);
            }
            if !dir.pop() {
                break;
            }
        }
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(".tokenslim.toml"))
        .ok()
}

/// 将配置文本解析并展平为一维 Map
fn parse_toml_to_flat_map(content: &str) -> Result<HashMap<String, String>, String> {
    let val: toml::Value = toml::from_str(content).map_err(|e| e.to_string())?;
    let mut flat = HashMap::new();
    flatten_toml_value("", &val, &mut flat);
    Ok(flat)
}

/// 递归展平 TOML
fn flatten_toml_value(prefix: &str, val: &toml::Value, flat: &mut HashMap<String, String>) {
    match val {
        toml::Value::Table(table) => {
            for (k, v) in table {
                let new_prefix = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                flatten_toml_value(&new_prefix, v, flat);
            }
        }
        toml::Value::String(s) => {
            flat.insert(prefix.to_string(), s.clone());
        }
        toml::Value::Integer(i) => {
            flat.insert(prefix.to_string(), i.to_string());
        }
        toml::Value::Float(f) => {
            flat.insert(prefix.to_string(), f.to_string());
        }
        toml::Value::Boolean(b) => {
            flat.insert(prefix.to_string(), b.to_string());
        }
        toml::Value::Datetime(d) => {
            flat.insert(prefix.to_string(), d.to_string());
        }
        toml::Value::Array(arr) => {
            if let Ok(s) = toml::to_string(arr) {
                flat.insert(prefix.to_string(), s.trim().to_string());
            }
        }
    }
}

/// 从环境变量中提取并覆盖配置 (前缀 TOKENSLIM_)
fn apply_env_overrides(config: &mut HashMap<String, String>) {
    for (k, v) in std::env::vars() {
        if k.starts_with("TOKENSLIM_") {
            let key_without_prefix = &k["TOKENSLIM_".len()..];
            let lower = key_without_prefix.to_lowercase();
            let mapped_key = if lower.starts_with("plugins_") && lower.ends_with("_enabled") {
                let plugin_name = &lower["plugins_".len()..lower.len() - "_enabled".len()];
                format!("plugins.{}.enabled", plugin_name)
            } else if lower.starts_with("token_optimizer_") {
                let rest = &lower["token_optimizer_".len()..];
                format!("token_optimizer.{}", rest)
            } else {
                if let Some(pos) = lower.find('_') {
                    format!("{}.{}", &lower[..pos], &lower[pos + 1..])
                } else {
                    lower
                }
            };
            if get_schema_type(&mapped_key).is_ok() {
                config.insert(mapped_key, v);
            }
        }
    }
}

/// 递归在 DocumentMut 中设置嵌套字段值
fn set_nested_value(
    doc: &mut DocumentMut,
    key: &str,
    value_str: &str,
    schema_type: SchemaType,
) -> Result<(), String> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return Err("配置键不能为空".to_string());
    }

    let val = match schema_type {
        SchemaType::Bool => {
            if value_str == "true" {
                Value::from(true)
            } else {
                Value::from(false)
            }
        }
        SchemaType::String => Value::from(value_str),
    };

    let mut current = doc.as_item_mut();
    for (i, &part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            if let Some(table) = current.as_table_mut() {
                if let Some(existing) = table.get_mut(part) {
                    if let Some(existing_val) = existing.as_value_mut() {
                        *existing_val = val.clone();
                    } else {
                        table.insert(part, Item::Value(val.clone()));
                    }
                } else {
                    table.insert(part, Item::Value(val.clone()));
                }
            } else {
                *current = Item::Table(toml_edit::Table::new());
                if let Some(table) = current.as_table_mut() {
                    table.insert(part, Item::Value(val.clone()));
                }
            }
            break;
        } else {
            if current.is_none() || (!current.is_table() && !current.is_inline_table()) {
                *current = Item::Table(toml_edit::Table::new());
            }

            if let Some(table) = current.as_table_mut() {
                if !table.contains_key(part) {
                    table.insert(part, Item::Table(toml_edit::Table::new()));
                }
                current = table.get_mut(part).ok_or_else(|| "无法在 TOML 中定位表项".to_string())?;
            } else {
                return Err(format!("路径组件 '{}' 不是合法的 Table", part));
            }
        }
    }
    Ok(())
}

/// 递归在 DocumentMut 中移除嵌套字段
fn remove_nested_value(doc: &mut DocumentMut, key: &str) -> Result<bool, String> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return Ok(false);
    }

    let mut current = doc.as_item_mut();
    for (i, &part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            if let Some(table) = current.as_table_mut() {
                return Ok(table.remove(part).is_some());
            }
            return Ok(false);
        } else {
            if let Some(table) = current.as_table_mut() {
                if let Some(next) = table.get_mut(part) {
                    current = next;
                } else {
                    return Ok(false);
                }
            } else {
                return Ok(false);
            }
        }
    }
    Ok(false)
}

/// 配置管理器结构体
pub struct ConfigManager;

impl ConfigManager {
    /// 加载并合并所有配置，遵循优先级链：环境变量 > 本地配置 > 全局配置 > 默认配置
    pub fn load_merged_config() -> HashMap<String, String> {
        let mut config = HashMap::new();

        // 1. 加载默认配置
        for (k, v) in DEFAULT_CONFIG.iter() {
            config.insert(k.to_string(), v.to_string());
        }

        // 2. 加载全局配置
        if let Some(global_path) = global_config_path() {
            if global_path.exists() {
                if let Ok(content) = fs::read_to_string(&global_path) {
                    if let Ok(toml_map) = parse_toml_to_flat_map(&content) {
                        for (k, v) in toml_map {
                            config.insert(k, v);
                        }
                    }
                }
            }
        }

        // 3. 加载本地项目配置
        if let Some(local_path) = local_config_path() {
            if local_path.exists() {
                if let Ok(content) = fs::read_to_string(&local_path) {
                    if let Ok(toml_map) = parse_toml_to_flat_map(&content) {
                        for (k, v) in toml_map {
                            config.insert(k, v);
                        }
                    }
                }
            }
        }

        // 4. 加载环境变量覆盖
        apply_env_overrides(&mut config);

        config
    }

    /// 获取配置项最终合并生效的值
    pub fn get_value(key: &str) -> Option<String> {
        let merged = Self::load_merged_config();
        merged.get(key).cloned()
    }

    /// 获取布尔类型的配置项值
    pub fn get_bool(key: &str) -> Option<bool> {
        Self::get_value(key).and_then(|v| v.parse::<bool>().ok())
    }

    /// 在指定作用域内写入配置项
    pub fn set_value(scope: ConfigScope, key: &str, value: &str) -> Result<(), String> {
        validate_value(key, value)?;

        let path = match scope {
            ConfigScope::Global => {
                let p = global_config_path().ok_or_else(|| "无法获取全局配置路径".to_string())?;
                if let Some(parent) = p.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                p
            }
            ConfigScope::Local => {
                let p = local_config_path().ok_or_else(|| "无法获取本地配置路径".to_string())?;
                if let Some(parent) = p.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                p
            }
        };

        let mut doc = if path.exists() {
            let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            content.parse::<DocumentMut>().map_err(|e| e.to_string())?
        } else {
            DocumentMut::new()
        };

        let schema_type = get_schema_type(key)?;
        set_nested_value(&mut doc, key, value, schema_type)?;

        fs::write(&path, doc.to_string()).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 在指定作用域内删除配置项
    pub fn unset_value(scope: ConfigScope, key: &str) -> Result<bool, String> {
        let path = match scope {
            ConfigScope::Global => global_config_path().ok_or_else(|| "无法获取全局配置路径".to_string())?,
            ConfigScope::Local => local_config_path().ok_or_else(|| "无法获取本地配置路径".to_string())?,
        };

        if !path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let mut doc = content.parse::<DocumentMut>().map_err(|e| e.to_string())?;

        let removed = remove_nested_value(&mut doc, key)?;
        if removed {
            fs::write(&path, doc.to_string()).map_err(|e| e.to_string())?;
        }

        Ok(removed)
    }

    /// 清空并重置指定作用域的配置文件
    pub fn reset(scope: ConfigScope) -> Result<(), String> {
        let path = match scope {
            ConfigScope::Global => global_config_path().ok_or_else(|| "无法获取全局配置路径".to_string())?,
            ConfigScope::Local => local_config_path().ok_or_else(|| "无法获取本地配置路径".to_string())?,
        };

        if path.exists() {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_validation() {
        assert!(validate_value("general.preset", "fast").is_ok());
        assert!(validate_value("general.preset", "unknown").is_err());
        assert!(validate_value("compression.reorder", "true").is_ok());
        assert!(validate_value("compression.reorder", "yes").is_err());
    }

    #[test]
    fn test_toml_edit_preserves_formatting() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_config.toml");
        if test_file.exists() {
            let _ = fs::remove_file(&test_file);
        }

        let initial_content = r#"# 这是一个测试配置
[general]
# 默认配置键
preset = "balanced"

# 下面是另一个段落
[compression]
reorder = true
"#;
        fs::write(&test_file, initial_content).unwrap();

        let mut doc = initial_content.parse::<DocumentMut>().unwrap();
        set_nested_value(&mut doc, "general.preset", "fast", SchemaType::String).unwrap();
        set_nested_value(&mut doc, "compression.ai_export", "true", SchemaType::Bool).unwrap();

        let result = doc.to_string();
        assert!(result.contains("# 这是一个测试配置"));
        assert!(result.contains("# 默认配置键"));
        assert!(result.contains("preset = \"fast\""));
        assert!(result.contains("ai_export = true"));
    }
}
