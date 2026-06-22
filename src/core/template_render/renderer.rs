// template_render/renderer.rs
// 模板渲染器 - 将 Token 列表渲染为最终字符串

use super::types::{TemplateContext, TemplateToken};

const E_TEMPLATE_BLOCK_END_UNEXPECTED: &str = "E_TEMPLATE_BLOCK_END_UNEXPECTED";
const E_TEMPLATE_BLOCK_END_NOT_FOUND: &str = "E_TEMPLATE_BLOCK_END_NOT_FOUND";

/// 渲染 Token 列表
///
/// # 参数
/// - `tokens`: Token 列表
/// - `context`: 变量上下文
///
/// # 返回
/// - 渲染后的字符串，如果条件块不匹配则返回错误
#[tracing::instrument(level = "debug", skip_all)]
pub fn render_tokens(
    tokens: &[TemplateToken],
    context: &TemplateContext,
) -> Result<String, String> {
    let mut output = String::new();
    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i] {
            TemplateToken::Text(text) => {
                output.push_str(text);
                i += 1;
            }
            TemplateToken::Variable(name) => {
                // 首先尝试作为 section 内容
                if let Some(content) = context.get_section(name) {
                    output.push_str(content);
                } else if let Some(value) = context.get_var(name) {
                    // 然后尝试作为简单变量
                    output.push_str(value);
                }
                // 缺失的变量和 section 替换为空字符串
                i += 1;
            }
            TemplateToken::SectionCount(section) => {
                if let Some(count) = context.get_section_count(section) {
                    output.push_str(&count.to_string());
                }
                i += 1;
            }
            TemplateToken::SectionItems(section) => {
                if let Some(items) = context.get_section_items(section) {
                    output.push_str(&items.join(", "));
                }
                i += 1;
            }
            TemplateToken::IfStart(var_name) => {
                // 查找对应的 IfEnd
                let (block_tokens, end_pos) =
                    extract_block(&tokens[i + 1..], &TemplateToken::IfEnd)?;

                // 如果变量为真，渲染块内容
                if context.is_truthy(var_name) {
                    let block_output = render_tokens(&block_tokens, context)?;
                    output.push_str(&block_output);
                }

                // 跳过整个块
                i = i + 1 + end_pos + 1;
            }
            TemplateToken::UnlessStart(var_name) => {
                // 查找对应的 UnlessEnd
                let (block_tokens, end_pos) =
                    extract_block(&tokens[i + 1..], &TemplateToken::UnlessEnd)?;

                // 如果变量为假，渲染块内容
                if !context.is_truthy(var_name) {
                    let block_output = render_tokens(&block_tokens, context)?;
                    output.push_str(&block_output);
                }

                // 跳过整个块
                i = i + 1 + end_pos + 1;
            }
            TemplateToken::IfEnd | TemplateToken::UnlessEnd => {
                return Err(E_TEMPLATE_BLOCK_END_UNEXPECTED.to_string());
            }
        }
    }

    Ok(output)
}

/// 提取条件块的内容
///
/// # 参数
/// - `tokens`: 从块开始后的 Token 列表
/// - `end_token`: 期望的结束 Token
///
/// # 返回
/// - (块内 Token 列表, 结束 Token 的位置)
fn extract_block(
    tokens: &[TemplateToken],
    end_token: &TemplateToken,
) -> Result<(Vec<TemplateToken>, usize), String> {
    let mut block_tokens = Vec::new();
    let mut depth = 0;
    let mut end_pos = None;

    for (i, token) in tokens.iter().enumerate() {
        // 检查是否为嵌套的开始标签
        let is_nested_start = matches!(
            token,
            TemplateToken::IfStart(_) | TemplateToken::UnlessStart(_)
        );

        // 检查是否为结束标签
        let is_end = token == end_token;

        if is_nested_start {
            depth += 1;
            block_tokens.push(token.clone());
        } else if is_end {
            if depth == 0 {
                end_pos = Some(i);
                break;
            } else {
                depth -= 1;
                block_tokens.push(token.clone());
            }
        } else {
            block_tokens.push(token.clone());
        }
    }

    match end_pos {
        Some(pos) => Ok((block_tokens, pos)),
        None => Err(E_TEMPLATE_BLOCK_END_NOT_FOUND.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_text() {
        let tokens = vec![TemplateToken::Text("Hello".to_string())];
        let context = TemplateContext::new();
        let result = render_tokens(&tokens, &context).unwrap();
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_render_variable() {
        let tokens = vec![
            TemplateToken::Text("Hello ".to_string()),
            TemplateToken::Variable("name".to_string()),
        ];
        let mut context = TemplateContext::new();
        context.set_var("name", "Alice");
        let result = render_tokens(&tokens, &context).unwrap();
        assert_eq!(result, "Hello Alice");
    }

    #[test]
    fn test_render_section_content() {
        let tokens = vec![
            TemplateToken::Text("Errors:\n".to_string()),
            TemplateToken::Variable("errors".to_string()),
        ];
        let mut context = TemplateContext::new();
        context.set_section("errors", "Error 1\nError 2");
        let result = render_tokens(&tokens, &context).unwrap();
        assert_eq!(result, "Errors:\nError 1\nError 2");
    }

    #[test]
    fn test_render_missing_variable() {
        let tokens = vec![
            TemplateToken::Text("Hello ".to_string()),
            TemplateToken::Variable("name".to_string()),
        ];
        let context = TemplateContext::new();
        let result = render_tokens(&tokens, &context).unwrap();
        assert_eq!(result, "Hello ");
    }

    #[test]
    fn test_render_section_count() {
        let tokens = vec![
            TemplateToken::Text("Count: ".to_string()),
            TemplateToken::SectionCount("errors".to_string()),
        ];
        let mut context = TemplateContext::new();
        context.set_section_count("errors", 5);
        let result = render_tokens(&tokens, &context).unwrap();
        assert_eq!(result, "Count: 5");
    }

    #[test]
    fn test_render_section_items() {
        let tokens = vec![
            TemplateToken::Text("Files: ".to_string()),
            TemplateToken::SectionItems("files".to_string()),
        ];
        let mut context = TemplateContext::new();
        context.set_section_items("files", vec!["a.rs".to_string(), "b.rs".to_string()]);
        let result = render_tokens(&tokens, &context).unwrap();
        assert_eq!(result, "Files: a.rs, b.rs");
    }

    #[test]
    fn test_render_if_true() {
        let tokens = vec![
            TemplateToken::IfStart("show".to_string()),
            TemplateToken::Text("Visible".to_string()),
            TemplateToken::IfEnd,
        ];
        let mut context = TemplateContext::new();
        context.set_var("show", "yes");
        let result = render_tokens(&tokens, &context).unwrap();
        assert_eq!(result, "Visible");
    }

    #[test]
    fn test_render_if_false() {
        let tokens = vec![
            TemplateToken::IfStart("show".to_string()),
            TemplateToken::Text("Visible".to_string()),
            TemplateToken::IfEnd,
        ];
        let context = TemplateContext::new();
        let result = render_tokens(&tokens, &context).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_unless_true() {
        let tokens = vec![
            TemplateToken::UnlessStart("hide".to_string()),
            TemplateToken::Text("Visible".to_string()),
            TemplateToken::UnlessEnd,
        ];
        let context = TemplateContext::new();
        let result = render_tokens(&tokens, &context).unwrap();
        assert_eq!(result, "Visible");
    }

    #[test]
    fn test_render_unless_false() {
        let tokens = vec![
            TemplateToken::UnlessStart("hide".to_string()),
            TemplateToken::Text("Visible".to_string()),
            TemplateToken::UnlessEnd,
        ];
        let mut context = TemplateContext::new();
        context.set_var("hide", "yes");
        let result = render_tokens(&tokens, &context).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_nested_if() {
        let tokens = vec![
            TemplateToken::IfStart("a".to_string()),
            TemplateToken::Text("A:".to_string()),
            TemplateToken::IfStart("b".to_string()),
            TemplateToken::Text("B".to_string()),
            TemplateToken::IfEnd,
            TemplateToken::IfEnd,
        ];
        let mut context = TemplateContext::new();
        context.set_var("a", "yes");
        context.set_var("b", "yes");
        let result = render_tokens(&tokens, &context).unwrap();
        assert_eq!(result, "A:B");
    }

    #[test]
    fn test_render_unexpected_end() {
        let tokens = vec![TemplateToken::IfEnd];
        let context = TemplateContext::new();
        let result = render_tokens(&tokens, &context);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_block_simple() {
        let tokens = vec![
            TemplateToken::Text("content".to_string()),
            TemplateToken::IfEnd,
        ];
        let (block, pos) = extract_block(&tokens, &TemplateToken::IfEnd).unwrap();
        assert_eq!(block, vec![TemplateToken::Text("content".to_string())]);
        assert_eq!(pos, 1);
    }

    #[test]
    fn test_extract_block_nested() {
        let tokens = vec![
            TemplateToken::IfStart("inner".to_string()),
            TemplateToken::Text("nested".to_string()),
            TemplateToken::IfEnd,
            TemplateToken::IfEnd,
        ];
        let (block, pos) = extract_block(&tokens, &TemplateToken::IfEnd).unwrap();
        assert_eq!(
            block,
            vec![
                TemplateToken::IfStart("inner".to_string()),
                TemplateToken::Text("nested".to_string()),
                TemplateToken::IfEnd,
            ]
        );
        assert_eq!(pos, 3);
    }
}
