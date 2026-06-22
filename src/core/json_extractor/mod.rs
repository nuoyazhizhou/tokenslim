//! JSON 提取器 — 从混杂文本中提取 JSON 对象
//!
//! 使用括号平衡算法从包含日志、错误信息等混杂内容的文本中提取完整的 JSON 对象。
//! 参考: RTK `other/rtk/src/parser/` 中的 JSON 提取逻辑

/// JSON 提取状态机
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtractState {
    /// 空闲状态：寻找 JSON 对象起始
    Idle,
    /// 对象内部：正在收集 JSON 内容
    InObject,
    /// 字符串内部：跳过字符串中的特殊字符
    InString,
    /// 转义状态：处理字符串中的转义字符
    Escaped,
}

/// 从文本中提取第一个完整的 JSON 对象
///
/// 使用深度计数器和字符串跳过状态机，从混杂文本中提取括号平衡的 JSON 对象。
///
/// # 算法
/// 1. 扫描文本，寻找 `{` 起始符
/// 2. 维护深度计数器：`{` +1, `}` -1
/// 3. 跳过字符串内部的括号（处理转义）
/// 4. 当深度归零时，提取完整对象
///
/// # 参数
/// - `text`: 包含 JSON 对象的混杂文本
///
/// # 返回
/// - `Some(String)`: 提取的 JSON 对象字符串
/// - `None`: 未找到完整的 JSON 对象
///
/// # 示例
/// ```
/// use tokenslim::core::json_extractor::extract_json_object;
///
/// let text = r#"Some log output {"key": "value", "nested": {"a": 1}} more text"#;
/// let json = extract_json_object(text).unwrap();
/// assert_eq!(json, r#"{"key": "value", "nested": {"a": 1}}"#);
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn extract_json_object(text: &str) -> Option<String> {
    let mut state = ExtractState::Idle;
    let mut depth = 0;
    let mut start_idx = 0;
    let chars: Vec<char> = text.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        match state {
            ExtractState::Idle => {
                if ch == '{' {
                    state = ExtractState::InObject;
                    depth = 1;
                    start_idx = i;
                }
            }
            ExtractState::InObject => {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            // 找到完整对象
                            let json_str: String = chars[start_idx..=i].iter().collect();
                            return Some(json_str);
                        }
                    }
                    '"' => state = ExtractState::InString,
                    _ => {}
                }
            }
            ExtractState::InString => match ch {
                '\\' => state = ExtractState::Escaped,
                '"' => state = ExtractState::InObject,
                _ => {}
            },
            ExtractState::Escaped => {
                // 转义字符后的任意字符都跳过
                state = ExtractState::InString;
            }
        }
    }

    None
}

/// 从文本中提取所有 JSON 对象
///
/// 与 `extract_json_object` 类似，但会提取文本中的所有 JSON 对象。
///
/// # 参数
/// - `text`: 包含 JSON 对象的混杂文本
///
/// # 返回
/// - `Vec<String>`: 提取的所有 JSON 对象字符串
///
/// # 示例
/// ```
/// use tokenslim::core::json_extractor::extract_all_json_objects;
///
/// let text = r#"{"a": 1} some text {"b": 2}"#;
/// let jsons = extract_all_json_objects(text);
/// assert_eq!(jsons.len(), 2);
/// assert_eq!(jsons[0], r#"{"a": 1}"#);
/// assert_eq!(jsons[1], r#"{"b": 2}"#);
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn extract_all_json_objects(text: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut remaining = text;

    while let Some(json) = extract_json_object(remaining) {
        let json_len = json.len();
        results.push(json);

        // 移动到下一个可能的 JSON 对象
        if let Some(idx) = remaining.find('{') {
            let skip_len = idx + json_len;
            if skip_len >= remaining.len() {
                break;
            }
            remaining = &remaining[skip_len..];
        } else {
            break;
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== extract_json_object 测试 ==========

    #[test]
    fn test_extract_simple_object() {
        let text = r#"{"key": "value"}"#;
        let result = extract_json_object(text);
        assert_eq!(result, Some(r#"{"key": "value"}"#.to_string()));
    }

    #[test]
    fn test_extract_nested_object() {
        let text = r#"{"outer": {"inner": 1}}"#;
        let result = extract_json_object(text);
        assert_eq!(result, Some(r#"{"outer": {"inner": 1}}"#.to_string()));
    }

    #[test]
    fn test_extract_with_prefix() {
        let text = r#"Some log output {"key": "value"}"#;
        let result = extract_json_object(text);
        assert_eq!(result, Some(r#"{"key": "value"}"#.to_string()));
    }

    #[test]
    fn test_extract_with_suffix() {
        let text = r#"{"key": "value"} more text"#;
        let result = extract_json_object(text);
        assert_eq!(result, Some(r#"{"key": "value"}"#.to_string()));
    }

    #[test]
    fn test_extract_with_both() {
        let text = r#"prefix {"key": "value"} suffix"#;
        let result = extract_json_object(text);
        assert_eq!(result, Some(r#"{"key": "value"}"#.to_string()));
    }

    #[test]
    fn test_extract_string_with_braces() {
        let text = r#"{"key": "value with {braces}"}"#;
        let result = extract_json_object(text);
        assert_eq!(
            result,
            Some(r#"{"key": "value with {braces}"}"#.to_string())
        );
    }

    #[test]
    fn test_extract_string_with_escaped_quote() {
        let text = r#"{"key": "value with \"quote\""}"#;
        let result = extract_json_object(text);
        assert_eq!(
            result,
            Some(r#"{"key": "value with \"quote\""}"#.to_string())
        );
    }

    #[test]
    fn test_extract_deeply_nested() {
        let text = r#"{"a": {"b": {"c": {"d": 1}}}}"#;
        let result = extract_json_object(text);
        assert_eq!(result, Some(r#"{"a": {"b": {"c": {"d": 1}}}}"#.to_string()));
    }

    #[test]
    fn test_extract_no_object() {
        let text = "No JSON here";
        let result = extract_json_object(text);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_incomplete_object() {
        let text = r#"{"key": "value""#;
        let result = extract_json_object(text);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_unbalanced_braces() {
        let text = r#"{"key": {"nested": 1}"#;
        let result = extract_json_object(text);
        assert_eq!(result, None);
    }

    // ========== extract_all_json_objects 测试 ==========

    #[test]
    fn test_extract_all_single() {
        let text = r#"{"a": 1}"#;
        let results = extract_all_json_objects(text);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], r#"{"a": 1}"#);
    }

    #[test]
    fn test_extract_all_multiple() {
        let text = r#"{"a": 1} some text {"b": 2}"#;
        let results = extract_all_json_objects(text);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], r#"{"a": 1}"#);
        assert_eq!(results[1], r#"{"b": 2}"#);
    }

    #[test]
    fn test_extract_all_nested_and_separate() {
        let text = r#"{"outer": {"inner": 1}} {"separate": 2}"#;
        let results = extract_all_json_objects(text);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], r#"{"outer": {"inner": 1}}"#);
        assert_eq!(results[1], r#"{"separate": 2}"#);
    }

    #[test]
    fn test_extract_all_none() {
        let text = "No JSON here";
        let results = extract_all_json_objects(text);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_extract_all_with_noise() {
        let text = r#"
            [INFO] Starting process
            {"status": "running", "pid": 1234}
            [DEBUG] Some debug info
            {"status": "completed", "result": {"code": 0}}
            [INFO] Done
        "#;
        let results = extract_all_json_objects(text);
        assert_eq!(results.len(), 2);
        assert!(results[0].contains("running"));
        assert!(results[1].contains("completed"));
    }
}
