//! Bash AST 拆分 — 复合命令解析与环境变量处理
//!
//! 提供 bash 命令的基础解析功能，不追求完整 AST，仅实现必要的拆分逻辑。

/// 拆分复合命令
///
/// 支持的分隔符：`&&`, `||`, `;`
///
/// P2-34：追踪 `(...)` 括号深度与反引号——命令替换 `$( ... )`、`` ` ... ` ``
/// 与进程替换 `<( ... )` 内的分隔符不再被误拆（此前 `echo $(make a && make b)`
/// 会被拆成 `echo $(make a` / `make b)` 两段，导致规则命中范围漂移）。
/// 不追求完整 AST，深度计数为 O(1) 成本；heredoc 仍不识别。
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
    let mut paren_depth: usize = 0; // P2-34：`(...)` 深度，>0 时分隔符不拆分
    let mut in_backtick = false; // P2-34：反引号命令替换内分隔符不拆分

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

        // P2-34：反引号状态翻转（引号外）
        if ch == '`' && !in_single_quote && !in_double_quote {
            in_backtick = !in_backtick;
            current.push(ch);
            continue;
        }

        // 在引号内，直接添加字符
        if in_single_quote || in_double_quote {
            current.push(ch);
            continue;
        }

        // P2-34：括号深度计数（含 `$(`、`<(`、`>(`）；引号/反引号内的括号不影响深度
        if ch == '(' {
            paren_depth += 1;
            current.push(ch);
            continue;
        }
        if ch == ')' {
            paren_depth = paren_depth.saturating_sub(1);
            current.push(ch);
            continue;
        }

        // 检查分隔符：仅在括号深度 0 且不在反引号内时生效（P2-34）
        if paren_depth == 0 && !in_backtick {
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
    let parts: Vec<String> = tokenize_quote_aware(command);

    let mut i = 0;
    while i < parts.len() {
        let part = &parts[i];
        // 检查是否为 KEY=value 格式
        if part.contains('=') && !part.starts_with('=') {
            // 简单检查：等号前后都有内容
            let eq_pos = part.find('=').unwrap();
            if eq_pos > 0 && eq_pos < part.len() - 1 {
                // 检查等号前是否为有效的变量名（字母、数字、下划线）
                let key = &part[..eq_pos];
                if key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    env_vars.push(part.clone());
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

/// 引号感知的空白切分：带引号的值（如 `FOO="debug foo"`）内部的空格不切断 token，
/// 保持 `KEY="debug foo"` 为单一 token，供 strip_env_prefix 正确识别（Q477 处置）。
///
/// P2-35：token 外的 `\` 转义同样不切断 token——`FOO=a\ b make test`（bash 语义
/// env `FOO="a b"`）此前被切成 `["FOO=a\", "b", ...]`，`FOO=a\` 被误判为合法 env
/// 导致 prog 误判为 `b`、内置包装器注入静默失效。现 `\` 直接吞掉下一字符
/// （原样保留两字符，输出 join 单空格可无损还原）；引号内行为不变（单引号内
/// `\` 不转义，由整体吞引号对的既有逻辑天然覆盖）。
fn tokenize_quote_aware(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars();
    let mut in_token = false;
    while let Some(c) = chars.next() {
        if c == '\\' {
            // 转义：无条件吞掉下一字符（含空白），两字符原样入 token（P2-35）
            in_token = true;
            cur.push(c);
            if let Some(c2) = chars.next() {
                cur.push(c2);
            }
            continue;
        }
        if c.is_whitespace() {
            if in_token {
                tokens.push(std::mem::take(&mut cur));
                in_token = false;
            }
            continue;
        }
        in_token = true;
        cur.push(c);
        if c == '"' || c == '\'' {
            // 读取直碰的成对引号，值中可含空格
            for c2 in chars.by_ref() {
                cur.push(c2);
                if c2 == c {
                    break;
                }
            }
        }
    }
    if in_token {
        tokens.push(cur);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试：拆分简单的 && 复合命令，应得到两个片段且分隔符记录正确。
    #[test]
    /// 契约：单条 `&&` 分隔应拆出 2 段，首段带 `&&` 分隔符、末段分隔符为空。
    fn test_split_compound_simple() {
        let parts = split_compound("cmd1 && cmd2");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], ("cmd1".to_string(), "&&".to_string()));
        assert_eq!(parts[1], ("cmd2".to_string(), String::new()));
    }

    /// 测试：同时包含 &&、||、; 三种分隔符的命令被拆分为四个片段。
    #[test]
    /// 契约：混合 `&&`/`||`/`;` 分隔应全部识别，各段分隔符一一对应、末段为空。
    fn test_split_compound_multiple() {
        let parts = split_compound("cmd1 && cmd2 || cmd3 ; cmd4");
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0].1, "&&");
        assert_eq!(parts[1].1, "||");
        assert_eq!(parts[2].1, ";");
        assert_eq!(parts[3].1, "");
    }

    /// 测试：引号内的分隔符不应触发拆分，引号内文本整体保留。
    #[test]
    /// 契约：引号内的 `&&` 不得被误判为分隔符（单引号包裹的字符串原样保留）。
    fn test_split_compound_with_quotes() {
        let parts = split_compound("echo 'hello && world' && cmd2");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].0, "echo 'hello && world'");
        assert_eq!(parts[1].0, "cmd2");
    }

    /// 测试：无分隔符的普通命令仅产生一个片段且无分隔符。
    #[test]
    /// 契约：无分隔符的单条命令应原样返回为单段，分隔符为空。
    fn test_split_compound_single_command() {
        let parts = split_compound("single command");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], ("single command".to_string(), String::new()));
    }

    /// 测试：单个环境变量前缀被剥离，剩余命令原样保留。
    #[test]
    /// 契约：单个 `KEY=value` 前缀应被剥离为环境变量，剩余部分为命令。
    fn test_strip_env_prefix_single() {
        let (env, cmd) = strip_env_prefix("RUST_LOG=debug cargo test");
        assert_eq!(env, "RUST_LOG=debug");
        assert_eq!(cmd, "cargo test");
    }

    /// 测试：多个连续环境变量前缀均被剥离并合并为前缀串。
    #[test]
    /// 契约：多个连续 `KEY=value` 前缀应全部剥离并合并，剩余部分为命令。
    fn test_strip_env_prefix_multiple() {
        let (env, cmd) = strip_env_prefix("FOO=bar BAZ=qux cargo build");
        assert_eq!(env, "FOO=bar BAZ=qux");
        assert_eq!(cmd, "cargo build");
    }

    /// 测试：无环境变量前缀时前缀为空、命令原样返回。
    #[test]
    /// 契约：无环境变量前缀时应返回空前缀，命令原样保留。
    fn test_strip_env_prefix_none() {
        let (env, cmd) = strip_env_prefix("cargo test");
        assert_eq!(env, "");
        assert_eq!(cmd, "cargo test");
    }

    /// 测试：参数中的 KEY=value 形态（如 --test=foo）不作为环境变量前缀剥离。
    #[test]
    /// 契约：参数中含 `=`（如 `--test=foo`）但非前缀的不得被误判为环境变量。
    fn test_strip_env_prefix_with_equals_in_args() {
        let (env, cmd) = strip_env_prefix("cargo test -- --test=foo");
        assert_eq!(env, "");
        assert_eq!(cmd, "cargo test -- --test=foo");
    }

    /// 测试：带引号/env 值含空格时不被拆散（Q477 处置）。
    #[test]
    /// 契约：`FOO="debug foo"` 应作为单一前缀保留，命令部分正确截断。
    fn test_strip_env_prefix_quoted_value_with_space() {
        let (env, cmd) = strip_env_prefix("FOO=\"debug foo\" cargo test");
        assert_eq!(env, "FOO=\"debug foo\"");
        assert_eq!(cmd, "cargo test");
    }

    /// 测试：命令替换内的 `&&` 不被误拆（P2-34 处置）。
    #[test]
    /// 契约：`echo $(make a && make b)` 应整体拆出 1 段，`$(...)` 内分隔符不生效。
    fn test_split_compound_command_substitution() {
        let parts = split_compound("echo $(make a && make b)");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].0, "echo $(make a && make b)");
        assert_eq!(parts[0].1, "");
    }

    /// 测试：反引号命令替换内的 `&&` 不被误拆，替换外正常拆分（P2-34 处置）。
    #[test]
    /// 契约：`` cmd1 && echo `make a && make b` `` 应拆 2 段，反引号内保持完整。
    fn test_split_compound_backtick_substitution() {
        let parts = split_compound("cmd1 && echo `make a && make b`");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].0, "cmd1");
        assert_eq!(parts[1].0, "echo `make a && make b`");
    }

    /// 测试：进程替换 `<( )` 内的分隔符不生效（P2-34 处置）。
    #[test]
    /// 契约：`diff <(make a && make b) ref` 应整体拆出 1 段。
    fn test_split_compound_process_substitution() {
        let parts = split_compound("diff <(make a && make b) ref");
        assert_eq!(parts.len(), 1);
    }

    /// 测试：反斜杠转义空格不切断 token，env 前缀正确剥离（P2-35 处置）。
    #[test]
    /// 契约：`FOO=a\\ b make test` 应识别 env 前缀 `FOO=a\\ b`，命令部分为 `make test`。
    fn test_strip_env_prefix_escaped_space() {
        let (env, cmd) = strip_env_prefix("FOO=a\\ b make test");
        assert_eq!(env, "FOO=a\\ b");
        assert_eq!(cmd, "make test");
    }
}
