//! 重写规则应用 — 内置包装器与用户规则

use super::user_config::{RewriteConfig, RewriteRule};

/// 应用重写规则
///
/// 按优先级应用：
/// 1. 用户自定义规则
/// 2. 内置包装器
pub fn apply_rules(command: &str, config: &RewriteConfig) -> String {
    let command = command.trim();
    if command.is_empty() {
        return String::new();
    }

    // 0. P2-32：落实 skip_patterns 语义——匹配跳过模式的命令原样保留，不做任何重写。
    //    此前 skip_patterns 存在但从未被 apply_rules 读取，是「承诺语义为零」的死配置。
    for pat in &config.skip_patterns {
        if let Ok(re) = regex::Regex::new(pat) {
            if re.is_match(command) {
                return command.to_string();
            }
        }
    }

    // 1. 应用用户规则
    for rule in &config.user_rules {
        if let Some(rewritten) = apply_user_rule(command, rule) {
            return rewritten;
        }
    }

    // 2. 应用内置包装器
    apply_builtin_wrappers(command)
}

/// 应用用户自定义规则
fn apply_user_rule(command: &str, rule: &RewriteRule) -> Option<String> {
    if let Ok(re) = regex::Regex::new(&rule.pattern) {
        if re.is_match(command) {
            return Some(re.replace(command, &rule.replacement).to_string());
        }
    }
    None
}

/// 应用内置包装器
///
/// 支持的工具：
/// - make → make SHELL=tokenslim
/// - just → just --shell tokenslim
pub fn apply_builtin_wrappers(command: &str) -> String {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return command.to_string();
    }

    let prog = parts[0];
    let args = &parts[1..];

    match prog {
        "make" | "gmake" => {
            // 检查是否已经有 SHELL= 参数
            if args.iter().any(|arg| arg.starts_with("SHELL=")) {
                return command.to_string();
            }
            format!("{} SHELL=tokenslim {}", prog, args.join(" "))
        }
        "just" => {
            // 检查是否已经有 --shell 参数
            if args.iter().any(|arg| arg.starts_with("--shell")) {
                return command.to_string();
            }
            format!("{} --shell tokenslim {}", prog, args.join(" "))
        }
        _ => command.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试：内置包装器为 make 命令注入 SHELL=tokenslim。
    #[test]
    fn test_apply_builtin_make() {
        let result = apply_builtin_wrappers("make test");
        assert_eq!(result, "make SHELL=tokenslim test");
    }

    /// 测试：make 命令已含 SHELL= 参数时不重复注入。
    #[test]
    fn test_apply_builtin_make_already_has_shell() {
        let result = apply_builtin_wrappers("make SHELL=/bin/bash test");
        assert_eq!(result, "make SHELL=/bin/bash test");
    }

    /// 测试：内置包装器为 just 命令注入 --shell tokenslim。
    #[test]
    fn test_apply_builtin_just() {
        let result = apply_builtin_wrappers("just build");
        assert_eq!(result, "just --shell tokenslim build");
    }

    /// 测试：just 命令已含 --shell 参数时不重复注入。
    #[test]
    fn test_apply_builtin_just_already_has_shell() {
        let result = apply_builtin_wrappers("just --shell bash build");
        assert_eq!(result, "just --shell bash build");
    }

    /// 测试：其他命令（cargo test）不被内置包装器改写。
    #[test]
    fn test_apply_builtin_other_command() {
        let result = apply_builtin_wrappers("cargo test");
        assert_eq!(result, "cargo test");
    }

    /// 测试：用户自定义规则命中时按规则替换命令。
    #[test]
    fn test_apply_user_rule() {
        let rule = RewriteRule {
            pattern: r"^npm test$".to_string(),
            replacement: "npm run test:tokenslim".to_string(),
        };
        let result = apply_user_rule("npm test", &rule);
        assert_eq!(result, Some("npm run test:tokenslim".to_string()));
    }

    /// 测试：用户规则不匹配时返回 None，命令不重写。
    #[test]
    fn test_apply_user_rule_no_match() {
        let rule = RewriteRule {
            pattern: r"^npm test$".to_string(),
            replacement: "npm run test:tokenslim".to_string(),
        };
        let result = apply_user_rule("npm build", &rule);
        assert_eq!(result, None);
    }

    /// 测试：带用户规则的配置优先于内置包装器生效。
    #[test]
    fn test_apply_rules_with_user_config() {
        let config = RewriteConfig {
            user_rules: vec![RewriteRule {
                pattern: r"^cargo test$".to_string(),
                replacement: "cargo test --quiet".to_string(),
            }],
            skip_patterns: vec![],
        };
        let result = apply_rules("cargo test", &config);
        assert_eq!(result, "cargo test --quiet");
    }

    /// 测试：无用户规则命中时回退到内置包装器。
    #[test]
    fn test_apply_rules_fallback_to_builtin() {
        let config = RewriteConfig::default();
        let result = apply_rules("make test", &config);
        assert_eq!(result, "make SHELL=tokenslim test");
    }

    /// P2-32 测试：命中 skip_patterns 的命令原样保留，不做任何重写（即使存在更上层规则）。
    #[test]
    fn test_apply_rules_skips_skip_patterns() {
        let config = RewriteConfig {
            user_rules: vec![RewriteRule {
                pattern: r"^cargo test$".to_string(),
                replacement: "cargo test --quiet".to_string(),
            }],
            skip_patterns: vec!["^cargo ".to_string()],
        };
        // skip_patterns 命中 cargo test，应跳过 user_rules，原样返回。
        let result = apply_rules("cargo test", &config);
        assert_eq!(result, "cargo test");
    }

    /// P2-32 测试：skip_patterns 未命中时正常应用用户规则。
    #[test]
    fn test_apply_rules_skip_not_matching() {
        let config = RewriteConfig {
            user_rules: vec![RewriteRule {
                pattern: r"^cargo test$".to_string(),
                replacement: "cargo test --quiet".to_string(),
            }],
            skip_patterns: vec!["^npm ".to_string()],
        };
        let result = apply_rules("cargo test", &config);
        assert_eq!(result, "cargo test --quiet");
    }
}
