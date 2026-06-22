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
    let start = text.find('{')?;
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

    #[test]
    fn extract_from_noisy_prefix_and_suffix() {
        let input = "pnpm notice [meta] {\"a\":1,\"b\":{\"c\":2}} trailing";
        let extracted = extract_json_object(input).expect("should extract json object");
        assert_eq!(extracted.raw, "{\"a\":1,\"b\":{\"c\":2}}");
        assert_eq!(&input[extracted.start..extracted.end], extracted.raw);
    }

    #[test]
    fn extract_handles_escaped_quotes_and_braces_in_string() {
        let input = r#"INFO {"msg":"brace { inside } and quote \"ok\"","v":1} end"#;
        let extracted = extract_json_object(input).expect("should extract json object");
        assert_eq!(
            extracted.raw,
            r#"{"msg":"brace { inside } and quote \"ok\"","v":1}"#
        );
    }

    #[test]
    fn returns_none_when_unbalanced() {
        let input = "prefix {\"a\":1";
        assert!(extract_json_object(input).is_none());
    }
}
