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
    if let Ok(config) = load_from_path(".tokenslim/rewrites.toml") {
        return config;
    }

    // 尝试从用户主目录加载
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home_path = Path::new(&home).join(".tokenslim").join("rewrites.toml");
        if let Ok(config) = load_from_path(&home_path) {
            return config;
        }
    }

    // 返回默认配置
    RewriteConfig::default()
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

    #[test]
    fn test_parse_empty_config() {
        let config = parse_config("").unwrap();
        assert!(config.user_rules.is_empty());
        assert!(config.skip_patterns.is_empty());
    }

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

    #[test]
    fn test_parse_config_invalid_toml() {
        let result = parse_config("invalid toml [[[");
        assert!(result.is_err());
    }

    #[test]
    fn test_default_config() {
        let config = RewriteConfig::default();
        assert!(config.user_rules.is_empty());
        assert!(config.skip_patterns.is_empty());
    }

    #[test]
    fn test_load_user_config_returns_default_when_not_found() {
        // 这个测试假设当前目录和用户主目录都没有 rewrites.toml
        let config = load_user_config();
        // 应该返回默认配置，不应该 panic
        assert!(config.user_rules.is_empty());
    }
}
