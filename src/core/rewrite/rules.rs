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

    #[test]
    fn test_apply_builtin_make() {
        let result = apply_builtin_wrappers("make test");
        assert_eq!(result, "make SHELL=tokenslim test");
    }

    #[test]
    fn test_apply_builtin_make_already_has_shell() {
        let result = apply_builtin_wrappers("make SHELL=/bin/bash test");
        assert_eq!(result, "make SHELL=/bin/bash test");
    }

    #[test]
    fn test_apply_builtin_just() {
        let result = apply_builtin_wrappers("just build");
        assert_eq!(result, "just --shell tokenslim build");
    }

    #[test]
    fn test_apply_builtin_just_already_has_shell() {
        let result = apply_builtin_wrappers("just --shell bash build");
        assert_eq!(result, "just --shell bash build");
    }

    #[test]
    fn test_apply_builtin_other_command() {
        let result = apply_builtin_wrappers("cargo test");
        assert_eq!(result, "cargo test");
    }

    #[test]
    fn test_apply_user_rule() {
        let rule = RewriteRule {
            pattern: r"^npm test$".to_string(),
            replacement: "npm run test:tokenslim".to_string(),
        };
        let result = apply_user_rule("npm test", &rule);
        assert_eq!(result, Some("npm run test:tokenslim".to_string()));
    }

    #[test]
    fn test_apply_user_rule_no_match() {
        let rule = RewriteRule {
            pattern: r"^npm test$".to_string(),
            replacement: "npm run test:tokenslim".to_string(),
        };
        let result = apply_user_rule("npm build", &rule);
        assert_eq!(result, None);
    }

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

    #[test]
    fn test_apply_rules_fallback_to_builtin() {
        let config = RewriteConfig::default();
        let result = apply_rules("make test", &config);
        assert_eq!(result, "make SHELL=tokenslim test");
    }
}
