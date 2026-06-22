//! Bash AST 拆分 — 复合命令解析与环境变量处理
//!
//! 提供 bash 命令的基础解析功能，不追求完整 AST，仅实现必要的拆分逻辑。

/// 拆分复合命令
///
/// 支持的分隔符：`&&`, `||`, `;`
///
/// 返回: Vec<(命令, 分隔符)>
///
/// ## 示例
///
/// ```ignore
/// let parts = split_compound("cmd1 && cmd2 || cmd3");
/// // 返回: [("cmd1", "&&"), ("cmd2", "||"), ("cmd3", "")]
/// ```
pub fn split_compound(command: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escape_next = false;

    while let Some(ch) = chars.next() {
        // 处理转义
        if escape_next {
            current.push(ch);
            escape_next = false;
            continue;
        }

        if ch == '\\' {
            escape_next = true;
            current.push(ch);
            continue;
        }

        // 处理引号
        if ch == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            current.push(ch);
            continue;
        }

        if ch == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            current.push(ch);
            continue;
        }

        // 在引号内，直接添加字符
        if in_single_quote || in_double_quote {
            current.push(ch);
            continue;
        }

        // 检查分隔符
        if ch == '&' && chars.peek() == Some(&'&') {
            chars.next(); // 消费第二个 &
            result.push((current.trim().to_string(), "&&".to_string()));
            current.clear();
        } else if ch == '|' && chars.peek() == Some(&'|') {
            chars.next(); // 消费第二个 |
            result.push((current.trim().to_string(), "||".to_string()));
            current.clear();
        } else if ch == ';' {
            result.push((current.trim().to_string(), ";".to_string()));
            current.clear();
        } else {
            current.push(ch);
        }
    }

    // 添加最后一个命令
    if !current.trim().is_empty() {
        result.push((current.trim().to_string(), String::new()));
    }

    result
}

/// 剥离环境变量前缀
///
/// 识别并剥离 `KEY=value` 形式的环境变量前缀。
///
/// 返回: (环境变量前缀, 剩余命令)
///
/// ## 示例
///
/// ```ignore
/// let (env, cmd) = strip_env_prefix("RUST_LOG=debug cargo test");
/// // env = "RUST_LOG=debug"
/// // cmd = "cargo test"
/// ```
pub fn strip_env_prefix(command: &str) -> (String, String) {
    let mut env_vars = Vec::new();
    let parts: Vec<&str> = command.split_whitespace().collect();

    let mut i = 0;
    while i < parts.len() {
        let part = parts[i];
        // 检查是否为 KEY=value 格式
        if part.contains('=') && !part.starts_with('=') {
            // 简单检查：等号前后都有内容
            let eq_pos = part.find('=').unwrap();
            if eq_pos > 0 && eq_pos < part.len() - 1 {
                // 检查等号前是否为有效的变量名（字母、数字、下划线）
                let key = &part[..eq_pos];
                if key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    env_vars.push(part);
                    i += 1;
                    continue;
                }
            }
        }
        break;
    }

    let env_prefix = env_vars.join(" ");
    let remaining = parts[i..].join(" ");

    (env_prefix, remaining)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_compound_simple() {
        let parts = split_compound("cmd1 && cmd2");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], ("cmd1".to_string(), "&&".to_string()));
        assert_eq!(parts[1], ("cmd2".to_string(), String::new()));
    }

    #[test]
    fn test_split_compound_multiple() {
        let parts = split_compound("cmd1 && cmd2 || cmd3 ; cmd4");
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0].1, "&&");
        assert_eq!(parts[1].1, "||");
        assert_eq!(parts[2].1, ";");
        assert_eq!(parts[3].1, "");
    }

    #[test]
    fn test_split_compound_with_quotes() {
        let parts = split_compound("echo 'hello && world' && cmd2");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].0, "echo 'hello && world'");
        assert_eq!(parts[1].0, "cmd2");
    }

    #[test]
    fn test_split_compound_single_command() {
        let parts = split_compound("single command");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], ("single command".to_string(), String::new()));
    }

    #[test]
    fn test_strip_env_prefix_single() {
        let (env, cmd) = strip_env_prefix("RUST_LOG=debug cargo test");
        assert_eq!(env, "RUST_LOG=debug");
        assert_eq!(cmd, "cargo test");
    }

    #[test]
    fn test_strip_env_prefix_multiple() {
        let (env, cmd) = strip_env_prefix("FOO=bar BAZ=qux cargo build");
        assert_eq!(env, "FOO=bar BAZ=qux");
        assert_eq!(cmd, "cargo build");
    }

    #[test]
    fn test_strip_env_prefix_none() {
        let (env, cmd) = strip_env_prefix("cargo test");
        assert_eq!(env, "");
        assert_eq!(cmd, "cargo test");
    }

    #[test]
    fn test_strip_env_prefix_with_equals_in_args() {
        let (env, cmd) = strip_env_prefix("cargo test -- --test=foo");
        assert_eq!(env, "");
        assert_eq!(cmd, "cargo test -- --test=foo");
    }
}
