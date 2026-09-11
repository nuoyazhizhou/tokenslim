//! 用户配置加载 — rewrites.toml 解析

use serde::{Deserialize, Serialize};
use std::path::Path;

const E_REWRITE_CONFIG_READ: &str = "E_REWRITE_CONFIG_READ";
const E_REWRITE_CONFIG_PARSE: &str = "E_REWRITE_CONFIG_PARSE";

/// 重写配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RewriteConfig {
    /// 用户自定义重写规则
    #[serde(default)]
    pub user_rules: Vec<RewriteRule>,
    /// 跳过模式列表（不重写匹配这些模式的命令）
    #[serde(default)]
    pub skip_patterns: Vec<String>,
}

/// 重写规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewriteRule {
    /// 匹配模式（正则表达式）
    pub pattern: String,
    /// 替换字符串
    pub replacement: String,
}

/// 从文件加载用户配置
///
/// 查找顺序：
/// 1. 当前目录的 `.tokenslim/rewrites.toml`
/// 2. 用户主目录的 `~/.tokenslim/rewrites.toml`
pub fn load_user_config() -> RewriteConfig {
    // 尝试从当前目录加载
    if let Some(config) = load_if_present(Path::new(".tokenslim/rewrites.toml")) {
        return config;
    }

    // 尝试从用户主目录加载
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home_path = Path::new(&home).join(".tokenslim").join("rewrites.toml");
        if let Some(config) = load_if_present(&home_path) {
            return config;
        }
    }

    // 返回默认配置
    RewriteConfig::default()
}

/// P2-33：`rewrites.toml` 静默回退修复。
///
/// 旧实现用 `if let Ok(config) = load_from_path(...)` 统一吞掉「文件不存在」与
/// 「文件存在但读取/解析失败」两类错误——后者会让 `E_REWRITE_CONFIG_PARSE`
/// 生产路径永不报告，坏配置被静默替换为默认配置。现区分处理：文件不存在属
/// 正常降级（静默跳过）；文件存在但加载/解析失败则 `log::warn!` 显式告警。
fn load_if_present<P: AsRef<Path>>(path: P) -> Option<RewriteConfig> {
    let path_ref = path.as_ref();
    if !path_ref.exists() {
        return None;
    }
    match load_from_path(path_ref) {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            log::warn!("{e}");
            None
        }
    }
}

/// 从指定路径加载配置
fn load_from_path<P: AsRef<Path>>(path: P) -> Result<RewriteConfig, String> {
    let path_ref = path.as_ref();
    let content = std::fs::read_to_string(path_ref)
        .map_err(|e| format!("{E_REWRITE_CONFIG_READ}:{path_ref:?}:{e}"))?;

    parse_config(&content)
}

/// 从 TOML 字符串解析配置
pub fn parse_config(toml_str: &str) -> Result<RewriteConfig, String> {
    toml::from_str(toml_str).map_err(|e| format!("{E_REWRITE_CONFIG_PARSE}:{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试：解析空 TOML 字符串得到无规则、无跳过模式的空配置。
    #[test]
    fn test_parse_empty_config() {
        let config = parse_config("").unwrap();
        assert!(config.user_rules.is_empty());
        assert!(config.skip_patterns.is_empty());
    }

    /// 测试：解析包含 skip_patterns 与多条 user_rules 的 TOML 配置。
    #[test]
    fn test_parse_config_with_rules() {
        let toml = r#"
skip_patterns = ["^git ", "^docker "]

[[user_rules]]
pattern = "^npm test$"
replacement = "npm run test:tokenslim"

[[user_rules]]
pattern = "^cargo test$"
replacement = "cargo test --quiet"
        "#;

        let config = parse_config(toml).unwrap_or_else(|e| {
            panic!("解析失败: {}", e);
        });

        assert_eq!(config.user_rules.len(), 2, "应该解析出 2 个规则");
        assert_eq!(config.user_rules[0].pattern, "^npm test$");
        assert_eq!(config.user_rules[0].replacement, "npm run test:tokenslim");
        assert_eq!(config.skip_patterns.len(), 2);
        assert_eq!(config.skip_patterns[0], "^git ");
    }

    /// 测试：非法 TOML 内容解析失败并返回错误。
    #[test]
    fn test_parse_config_invalid_toml() {
        let result = parse_config("invalid toml [[[");
        assert!(result.is_err());
    }

    /// 测试：RewriteConfig 默认值应为空规则与空跳过模式。
    #[test]
    fn test_default_config() {
        let config = RewriteConfig::default();
        assert!(config.user_rules.is_empty());
        assert!(config.skip_patterns.is_empty());
    }

    /// 测试：当前目录与用户主目录均无 rewrites.toml 时返回默认配置且不 panic。
    #[test]
    fn test_load_user_config_returns_default_when_not_found() {
        // 这个测试假设当前目录和用户主目录都没有 rewrites.toml
        let config = load_user_config();
        // 应该返回默认配置，不应该 panic
        assert!(config.user_rules.is_empty());
    }
}
