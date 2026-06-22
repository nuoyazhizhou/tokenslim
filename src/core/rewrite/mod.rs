//! 命令重写引擎 — bash AST 拆分与规则应用
//!
//! 本模块提供命令重写功能，支持：
//! - bash 复合命令拆分 (`&&` / `||` / `;`)
//! - 环境变量前缀剥离 (`KEY=value cmd`)
//! - 用户自定义重写规则
//! - 内置包装器 (make SHELL=tokenslim, just --shell tokenslim)
//! - 透明命令列表 (ssh/mysql/psql 等不参与重写)
//!
//! ## 用法
//!
//! ```ignore
//! use tokenslim::core::rewrite::{rewrite_command, RewriteConfig};
//!
//! let config = RewriteConfig::default();
//! let rewritten = rewrite_command("make test && cargo build", &config);
//! // 输出: "make SHELL=tokenslim test && cargo SHELL=tokenslim build"
//! ```
//!
//! 参考: TOKF `other/tokf/crates/tokf-cli/src/rewrite/` (约 2000 行)

mod bash_ast;
mod rules;
mod transparent;
mod user_config;

pub use bash_ast::{split_compound, strip_env_prefix};
pub use rules::{apply_builtin_wrappers, apply_rules};
pub use transparent::is_transparent_command;
pub use user_config::{load_user_config, RewriteConfig, RewriteRule};

/// 重写单个命令
///
/// 处理流程：
/// 1. 检查是否为透明命令（ssh/mysql 等）
/// 2. 剥离环境变量前缀
/// 3. 拆分复合命令
/// 4. 对每个子命令应用重写规则
/// 5. 重新组合
pub fn rewrite_command(command: &str, config: &RewriteConfig) -> String {
    let command = command.trim();
    if command.is_empty() {
        return String::new();
    }

    // 检查是否为透明命令
    if is_transparent_command(command) {
        return command.to_string();
    }

    // 剥离环境变量前缀
    let (env_prefix, cmd_without_env) = strip_env_prefix(command);

    // 拆分复合命令
    let parts = split_compound(&cmd_without_env);

    // 对每个部分应用重写规则
    let rewritten_parts: Vec<String> = parts
        .into_iter()
        .map(|(cmd, separator)| {
            let rewritten = apply_rules(&cmd, config);
            if separator.is_empty() {
                rewritten
            } else {
                format!("{} {}", rewritten, separator)
            }
        })
        .collect();

    // 重新组合
    let result = rewritten_parts.join(" ");

    // 添加回环境变量前缀
    if env_prefix.is_empty() {
        result
    } else {
        format!("{} {}", env_prefix, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rewrite_simple_command() {
        let config = RewriteConfig::default();
        let result = rewrite_command("echo hello", &config);
        assert_eq!(result, "echo hello");
    }

    #[test]
    fn test_rewrite_make_command() {
        let config = RewriteConfig::default();
        let result = rewrite_command("make test", &config);
        assert!(result.contains("SHELL=tokenslim"));
    }

    #[test]
    fn test_rewrite_compound_command() {
        let config = RewriteConfig::default();
        let result = rewrite_command("make test && cargo build", &config);
        assert!(result.contains("&&"));
        // 每个子命令都应该被重写
        assert!(result.contains("SHELL=tokenslim"));
    }

    #[test]
    fn test_transparent_command_not_rewritten() {
        let config = RewriteConfig::default();
        let result = rewrite_command("ssh user@host", &config);
        assert_eq!(result, "ssh user@host");
    }

    #[test]
    fn test_env_prefix_preserved() {
        let config = RewriteConfig::default();
        let result = rewrite_command("RUST_LOG=debug cargo test", &config);
        assert!(result.starts_with("RUST_LOG=debug"));
    }
}
