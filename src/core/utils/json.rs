#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractedJsonObject<'a> {
    pub start: usize,
    pub end: usize,
    pub raw: &'a str,
}

/// 从混杂文本中提取第一个括号平衡的 JSON 对象。
///
/// - 自动跳过字符串内部的大括号
/// - 处理转义引号（`\"`）
/// - 返回原始切片在源文本中的 [start, end) 范围
pub fn extract_json_object(text: &str) -> Option<ExtractedJsonObject<'_>> {
    // Q462 处置：从串首扫描并追踪字符串状态，找到第一个「字符串之外」的 `{` 作为起点，
    // 避免前导 JSON 字符串里出现的 `{` 被误当对象起点；找不到则回退历史口径。
    let mut start = text.find('{')?;
    {
        let mut in_string = false;
        let mut escaped = false;
        for (idx, ch) in text.char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                    continue;
                }
                match ch {
                    '\\' => escaped = true,
                    '"' => in_string = false,
                    _ => {}
                }
                continue;
            }
            match ch {
                '"' => in_string = true,
                '{' => {
                    start = idx;
                    break;
                }
                _ => {}
            }
        }
    }
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (idx, ch) in text[start..].char_indices() {
        let absolute = start + idx;

        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    let end = absolute + ch.len_utf8();
                    return Some(ExtractedJsonObject {
                        start,
                        end,
                        raw: &text[start..end],
                    });
                }
            }
            _ => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::extract_json_object;

    /// 测试：从带噪声前缀/后缀的文本中提取括号平衡的 JSON 对象，并校验返回范围与切片一致。
    #[test]
    fn extract_from_noisy_prefix_and_suffix() {
        let input = "pnpm notice [meta] {\"a\":1,\"b\":{\"c\":2}} trailing";
        let extracted = extract_json_object(input).expect("should extract json object");
        assert_eq!(extracted.raw, "{\"a\":1,\"b\":{\"c\":2}}");
        assert_eq!(&input[extracted.start..extracted.end], extracted.raw);
    }

    /// 测试：字符串内部的大括号与转义引号不会被误判为 JSON 结构边界。
    #[test]
    fn extract_handles_escaped_quotes_and_braces_in_string() {
        let input = r#"INFO {"msg":"brace { inside } and quote \"ok\"","v":1} end"#;
        let extracted = extract_json_object(input).expect("should extract json object");
        assert_eq!(
            extracted.raw,
            r#"{"msg":"brace { inside } and quote \"ok\"","v":1}"#
        );
    }

    /// 测试：括号不平衡（缺少闭合）时返回 None。
    #[test]
    fn returns_none_when_unbalanced() {
        let input = "prefix {\"a\":1";
        assert!(extract_json_object(input).is_none());
    }
}
