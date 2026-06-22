// filter_discover/parser.rs
// Session 文件解析器 - 支持 DeepSeek/Claude session JSON 格式

use super::types::SessionCommand;
use serde_json::Value;
use std::fs;
use std::path::Path;

const E_FILTER_DISCOVER_SESSION_READ: &str = "E_FILTER_DISCOVER_SESSION_READ";
const E_FILTER_DISCOVER_SESSION_JSON_PARSE: &str = "E_FILTER_DISCOVER_SESSION_JSON_PARSE";
const E_FILTER_DISCOVER_SESSION_FORMAT_UNKNOWN: &str = "E_FILTER_DISCOVER_SESSION_FORMAT_UNKNOWN";

/// 解析 session 文件
///
/// 支持的格式:
/// - DeepSeek session JSON
/// - Claude session JSON
///
/// # 参数
/// - `path`: session 文件路径
///
/// # 返回
/// - `Vec<SessionCommand>`: 提取的命令列表
#[tracing::instrument(level = "debug", skip_all)]
pub fn parse_session_file(path: &Path) -> Result<Vec<SessionCommand>, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("{E_FILTER_DISCOVER_SESSION_READ}:{path:?}:{e}"))?;

    let json: Value = serde_json::from_str(&content)
        .map_err(|e| format!("{E_FILTER_DISCOVER_SESSION_JSON_PARSE}:{path:?}:{e}"))?;

    // 尝试不同的解析策略
    if let Some(commands) = try_parse_deepseek(&json) {
        return Ok(commands);
    }

    if let Some(commands) = try_parse_claude(&json) {
        return Ok(commands);
    }

    // 如果都不匹配，尝试通用解析
    if let Some(commands) = try_parse_generic(&json) {
        return Ok(commands);
    }

    Err(format!(
        "{E_FILTER_DISCOVER_SESSION_FORMAT_UNKNOWN}:{path:?}"
    ))
}

/// 尝试解析 DeepSeek session 格式
#[tracing::instrument(level = "trace", skip_all)]
fn try_parse_deepseek(json: &Value) -> Option<Vec<SessionCommand>> {
    let messages = json.get("messages")?.as_array()?;
    let mut commands = Vec::new();

    for msg in messages {
        if let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) {
            for call in tool_calls {
                if let Some(func) = call.get("function") {
                    if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                        if name == "execute_command" || name == "bash" || name == "shell" {
                            if let Some(args_str) = func.get("arguments").and_then(|v| v.as_str()) {
                                if let Ok(args) = serde_json::from_str::<Value>(args_str) {
                                    if let Some(cmd) = args.get("command").and_then(|v| v.as_str())
                                    {
                                        commands.push(SessionCommand {
                                            command: cmd.to_string(),
                                            input_bytes: None,
                                            output_bytes: None,
                                            input_tokens: None,
                                            output_tokens: None,
                                            timestamp: msg
                                                .get("timestamp")
                                                .and_then(|v| v.as_str())
                                                .map(|s| s.to_string()),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if commands.is_empty() {
        None
    } else {
        Some(commands)
    }
}

/// 尝试解析 Claude session 格式
#[tracing::instrument(level = "trace", skip_all)]
fn try_parse_claude(json: &Value) -> Option<Vec<SessionCommand>> {
    let mut commands = Vec::new();

    // 尝试顶层 content 数组
    if let Some(content) = json.get("content").and_then(|v| v.as_array()) {
        for item in content {
            if let Some(tool_type) = item.get("type").and_then(|v| v.as_str()) {
                if tool_type == "tool_use" {
                    if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                        if name == "execute_pwsh" || name == "execute_bash" || name == "bash" {
                            if let Some(input) = item.get("input") {
                                if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                                    commands.push(SessionCommand {
                                        command: cmd.to_string(),
                                        input_bytes: None,
                                        output_bytes: None,
                                        input_tokens: None,
                                        output_tokens: None,
                                        timestamp: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 尝试 messages 数组中的 content
    if let Some(messages) = json.get("messages").and_then(|v| v.as_array()) {
        for msg in messages {
            if let Some(content) = msg.get("content").and_then(|v| v.as_array()) {
                for item in content {
                    if let Some(tool_type) = item.get("type").and_then(|v| v.as_str()) {
                        if tool_type == "tool_use" {
                            if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                                if name == "execute_pwsh"
                                    || name == "execute_bash"
                                    || name == "bash"
                                {
                                    if let Some(input) = item.get("input") {
                                        if let Some(cmd) =
                                            input.get("command").and_then(|v| v.as_str())
                                        {
                                            commands.push(SessionCommand {
                                                command: cmd.to_string(),
                                                input_bytes: None,
                                                output_bytes: None,
                                                input_tokens: None,
                                                output_tokens: None,
                                                timestamp: msg
                                                    .get("timestamp")
                                                    .and_then(|v| v.as_str())
                                                    .map(|s| s.to_string()),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if commands.is_empty() {
        None
    } else {
        Some(commands)
    }
}

/// 尝试通用解析（递归搜索 command 字段）
#[tracing::instrument(level = "trace", skip_all)]
fn try_parse_generic(json: &Value) -> Option<Vec<SessionCommand>> {
    let mut commands = Vec::new();
    extract_commands_recursive(json, &mut commands);

    if commands.is_empty() {
        None
    } else {
        Some(commands)
    }
}

/// 递归提取命令
fn extract_commands_recursive(value: &Value, commands: &mut Vec<SessionCommand>) {
    match value {
        Value::Object(map) => {
            // 检查是否包含 command 字段
            if let Some(cmd) = map.get("command").and_then(|v| v.as_str()) {
                // 过滤掉明显不是 shell 命令的内容
                if !cmd.is_empty() && !cmd.starts_with('{') && !cmd.starts_with('[') {
                    commands.push(SessionCommand {
                        command: cmd.to_string(),
                        input_bytes: map.get("input_bytes").and_then(|v| v.as_u64()),
                        output_bytes: map.get("output_bytes").and_then(|v| v.as_u64()),
                        input_tokens: map.get("input_tokens").and_then(|v| v.as_i64()),
                        output_tokens: map.get("output_tokens").and_then(|v| v.as_i64()),
                        timestamp: map
                            .get("timestamp")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    });
                }
            }

            // 递归处理所有值
            for v in map.values() {
                extract_commands_recursive(v, commands);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                extract_commands_recursive(v, commands);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_deepseek_format() {
        let json = json!({
            "messages": [
                {
                    "role": "assistant",
                    "tool_calls": [
                        {
                            "function": {
                                "name": "execute_command",
                                "arguments": "{\"command\":\"git status\"}"
                            }
                        }
                    ],
                    "timestamp": "2024-01-01T00:00:00Z"
                }
            ]
        });

        let commands = try_parse_deepseek(&json).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, "git status");
        assert_eq!(
            commands[0].timestamp,
            Some("2024-01-01T00:00:00Z".to_string())
        );
    }

    #[test]
    fn test_parse_claude_format() {
        let json = json!({
            "messages": [
                {
                    "content": [
                        {
                            "type": "tool_use",
                            "name": "execute_pwsh",
                            "input": {
                                "command": "cargo test"
                            }
                        }
                    ],
                    "timestamp": "2024-01-01T00:00:00Z"
                }
            ]
        });

        let commands = try_parse_claude(&json).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, "cargo test");
    }

    #[test]
    fn test_parse_generic_format() {
        let json = json!({
            "data": {
                "items": [
                    {
                        "command": "npm test",
                        "input_bytes": 1000,
                        "output_bytes": 5000
                    }
                ]
            }
        });

        let commands = try_parse_generic(&json).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, "npm test");
        assert_eq!(commands[0].input_bytes, Some(1000));
        assert_eq!(commands[0].output_bytes, Some(5000));
    }

    #[test]
    fn test_parse_multiple_commands() {
        let json = json!({
            "messages": [
                {
                    "tool_calls": [
                        {
                            "function": {
                                "name": "execute_command",
                                "arguments": "{\"command\":\"git status\"}"
                            }
                        },
                        {
                            "function": {
                                "name": "execute_command",
                                "arguments": "{\"command\":\"cargo build\"}"
                            }
                        }
                    ]
                }
            ]
        });

        let commands = try_parse_deepseek(&json).unwrap();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].command, "git status");
        assert_eq!(commands[1].command, "cargo build");
    }
}
