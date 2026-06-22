// template_render/parser.rs
// 模板解析器 - 将模板字符串解析为 Token 列表

use super::types::TemplateToken;

const E_TEMPLATE_TAG_UNCLOSED: &str = "E_TEMPLATE_TAG_UNCLOSED";
const E_TEMPLATE_SECTION_PROPERTY_UNKNOWN: &str = "E_TEMPLATE_SECTION_PROPERTY_UNKNOWN";

/// 解析模板字符串为 Token 列表
///
/// # 参数
/// - `template`: 模板字符串
///
/// # 返回
/// - Token 列表，如果语法错误则返回错误信息
#[tracing::instrument(level = "debug", skip_all)]
pub fn parse_template(template: &str) -> Result<Vec<TemplateToken>, String> {
    let mut tokens = Vec::new();
    let mut chars = template.chars().peekable();
    let mut current_text = String::new();

    while let Some(ch) = chars.next() {
        if ch == '{' && chars.peek() == Some(&'{') {
            // 开始解析模板标签
            chars.next(); // 消费第二个 '{'

            // 保存之前的文本
            if !current_text.is_empty() {
                tokens.push(TemplateToken::Text(current_text.clone()));
                current_text.clear();
            }

            // 提取标签内容
            let mut tag_content = String::new();
            let mut found_close = false;

            while let Some(ch) = chars.next() {
                if ch == '}' && chars.peek() == Some(&'}') {
                    chars.next(); // 消费第二个 '}'
                    found_close = true;
                    break;
                }
                tag_content.push(ch);
            }

            if !found_close {
                return Err(format!("{E_TEMPLATE_TAG_UNCLOSED}:{{{{{}}}", tag_content));
            }

            // 解析标签内容
            let tag_content = tag_content.trim();
            let token = parse_tag(tag_content)?;
            tokens.push(token);
        } else {
            current_text.push(ch);
        }
    }

    // 保存剩余的文本
    if !current_text.is_empty() {
        tokens.push(TemplateToken::Text(current_text));
    }

    Ok(tokens)
}

/// 解析单个标签内容
fn parse_tag(content: &str) -> Result<TemplateToken, String> {
    // 条件块开始: #if var
    if let Some(var_name) = content.strip_prefix("#if ") {
        return Ok(TemplateToken::IfStart(var_name.trim().to_string()));
    }

    // 条件块结束: /if
    if content == "/if" {
        return Ok(TemplateToken::IfEnd);
    }

    // 反向条件块开始: #unless var
    if let Some(var_name) = content.strip_prefix("#unless ") {
        return Ok(TemplateToken::UnlessStart(var_name.trim().to_string()));
    }

    // 反向条件块结束: /unless
    if content == "/unless" {
        return Ok(TemplateToken::UnlessEnd);
    }

    // Section 计数: section.count
    if let Some(dot_pos) = content.find('.') {
        let section_name = &content[..dot_pos];
        let property = &content[dot_pos + 1..];

        match property {
            "count" => return Ok(TemplateToken::SectionCount(section_name.to_string())),
            "items" => return Ok(TemplateToken::SectionItems(section_name.to_string())),
            _ => {
                return Err(format!(
                    "{E_TEMPLATE_SECTION_PROPERTY_UNKNOWN}:{}",
                    property
                ))
            }
        }
    }

    // 简单变量
    Ok(TemplateToken::Variable(content.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_text() {
        let tokens = parse_template("Hello World").unwrap();
        assert_eq!(tokens, vec![TemplateToken::Text("Hello World".to_string())]);
    }

    #[test]
    fn test_parse_simple_variable() {
        let tokens = parse_template("Hello {{name}}!").unwrap();
        assert_eq!(
            tokens,
            vec![
                TemplateToken::Text("Hello ".to_string()),
                TemplateToken::Variable("name".to_string()),
                TemplateToken::Text("!".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_multiple_variables() {
        let tokens = parse_template("{{a}} and {{b}}").unwrap();
        assert_eq!(
            tokens,
            vec![
                TemplateToken::Variable("a".to_string()),
                TemplateToken::Text(" and ".to_string()),
                TemplateToken::Variable("b".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_section_count() {
        let tokens = parse_template("Count: {{errors.count}}").unwrap();
        assert_eq!(
            tokens,
            vec![
                TemplateToken::Text("Count: ".to_string()),
                TemplateToken::SectionCount("errors".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_section_items() {
        let tokens = parse_template("Items: {{files.items}}").unwrap();
        assert_eq!(
            tokens,
            vec![
                TemplateToken::Text("Items: ".to_string()),
                TemplateToken::SectionItems("files".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_if_block() {
        let tokens = parse_template("{{#if show}}Visible{{/if}}").unwrap();
        assert_eq!(
            tokens,
            vec![
                TemplateToken::IfStart("show".to_string()),
                TemplateToken::Text("Visible".to_string()),
                TemplateToken::IfEnd,
            ]
        );
    }

    #[test]
    fn test_parse_unless_block() {
        let tokens = parse_template("{{#unless hide}}Visible{{/unless}}").unwrap();
        assert_eq!(
            tokens,
            vec![
                TemplateToken::UnlessStart("hide".to_string()),
                TemplateToken::Text("Visible".to_string()),
                TemplateToken::UnlessEnd,
            ]
        );
    }

    #[test]
    fn test_parse_unclosed_tag() {
        let result = parse_template("{{unclosed");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains(E_TEMPLATE_TAG_UNCLOSED));
    }

    #[test]
    fn test_parse_unknown_section_property() {
        let result = parse_template("{{section.unknown}}");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains(E_TEMPLATE_SECTION_PROPERTY_UNKNOWN));
    }

    #[test]
    fn test_parse_nested_conditions() {
        let tokens = parse_template("{{#if a}}{{#if b}}nested{{/if}}{{/if}}").unwrap();
        assert_eq!(
            tokens,
            vec![
                TemplateToken::IfStart("a".to_string()),
                TemplateToken::IfStart("b".to_string()),
                TemplateToken::Text("nested".to_string()),
                TemplateToken::IfEnd,
                TemplateToken::IfEnd,
            ]
        );
    }
}
