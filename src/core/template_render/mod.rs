// template_render/mod.rs
// 模板渲染系统 - 支持变量替换和条件渲染

pub mod parser;
pub mod renderer;
pub mod types;

use crate::utils::i18n::{t, t1};

pub use parser::*;
pub use renderer::*;
pub use types::*;

/// 将模板渲染的内部错误码映射为带 i18n 文案的错误信息；
/// 未识别的错误码原样返回。
fn map_template_error(err: String) -> String {
    let (code, detail) = match err.split_once(':') {
        Some((code, detail)) => (code, detail),
        None => (err.as_str(), ""),
    };

    match code {
        "E_TEMPLATE_TAG_UNCLOSED" => {
            format!("{code}: {}", t1("template_error_tag_unclosed", detail))
        }
        "E_TEMPLATE_SECTION_PROPERTY_UNKNOWN" => format!(
            "{code}: {}",
            t1("template_error_section_property_unknown", detail)
        ),
        "E_TEMPLATE_BLOCK_END_UNEXPECTED" => {
            format!("{code}: {}", t("template_error_block_end_unexpected"))
        }
        "E_TEMPLATE_BLOCK_END_NOT_FOUND" => {
            format!("{code}: {}", t("template_error_block_end_not_found"))
        }
        _ => err,
    }
}

/// 渲染模板
///
/// # 参数
/// - `template`: 模板字符串
/// - `context`: 变量上下文
///
/// # 返回
/// - 渲染后的字符串，如果模板语法错误则返回错误信息
///
/// # 示例
/// ```
/// use tokenslim::core::template_render::{render_template, TemplateContext};
///
/// let mut context = TemplateContext::new();
/// context.set_var("name", "Alice");
/// context.set_var("count", "42");
///
/// let template = "Hello {{name}}, you have {{count}} messages.";
/// let result = render_template(template, &context);
/// assert_eq!(result, Ok("Hello Alice, you have 42 messages.".to_string()));
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn render_template(template: &str, context: &TemplateContext) -> Result<String, String> {
    // 1. 解析模板
    let tokens = parse_template(template).map_err(map_template_error)?;

    // 2. 渲染 tokens
    render_tokens(&tokens, context).map_err(map_template_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试：单个变量 {{name}} 被替换为上下文中的值。
    #[test]
    fn test_simple_variable_replacement() {
        let mut context = TemplateContext::new();
        context.set_var("name", "Alice");

        let template = "Hello {{name}}!";
        let result = render_template(template, &context).unwrap();
        assert_eq!(result, "Hello Alice!");
    }

    /// 测试：多个变量在模板中同时被替换。
    #[test]
    fn test_multiple_variables() {
        let mut context = TemplateContext::new();
        context.set_var("name", "Bob");
        context.set_var("count", "5");

        let template = "{{name}} has {{count}} items";
        let result = render_template(template, &context).unwrap();
        assert_eq!(result, "Bob has 5 items");
    }

    /// 测试：缺失的变量替换为空字符串。
    #[test]
    fn test_missing_variable() {
        let context = TemplateContext::new();

        let template = "Hello {{name}}!";
        let result = render_template(template, &context).unwrap();
        // 缺失的变量替换为空字符串
        assert_eq!(result, "Hello !");
    }

    /// 测试：{{errors}} 形式的 section 内容被整体插入。
    #[test]
    fn test_section_content() {
        let mut context = TemplateContext::new();
        context.set_section("errors", "Error 1\nError 2\nError 3");

        let template = "Found errors:\n{{errors}}";
        let result = render_template(template, &context).unwrap();
        assert_eq!(result, "Found errors:\nError 1\nError 2\nError 3");
    }

    /// 测试：{{#if show}} 变量为真时渲染块内容。
    #[test]
    fn test_conditional_if_true() {
        let mut context = TemplateContext::new();
        context.set_var("show", "yes");

        let template = "{{#if show}}Visible{{/if}}";
        let result = render_template(template, &context).unwrap();
        assert_eq!(result, "Visible");
    }

    /// 测试：{{#if show}} 变量缺失（假）时块内容被省略。
    #[test]
    fn test_conditional_if_false() {
        let context = TemplateContext::new();

        let template = "{{#if show}}Visible{{/if}}";
        let result = render_template(template, &context).unwrap();
        assert_eq!(result, "");
    }

    /// 测试：{{#unless show}} 变量缺失（假）时渲染块内容。
    #[test]
    fn test_conditional_unless_true() {
        let context = TemplateContext::new();

        let template = "{{#unless show}}Hidden{{/unless}}";
        let result = render_template(template, &context).unwrap();
        assert_eq!(result, "Hidden");
    }

    /// 测试：{{#unless show}} 变量为真时块内容被省略。
    #[test]
    fn test_conditional_unless_false() {
        let mut context = TemplateContext::new();
        context.set_var("show", "yes");

        let template = "{{#unless show}}Hidden{{/unless}}";
        let result = render_template(template, &context).unwrap();
        assert_eq!(result, "");
    }

    /// 测试：{{errors.count}} 渲染为 section 计数。
    #[test]
    fn test_section_count() {
        let mut context = TemplateContext::new();
        context.set_section_count("errors", 3);

        let template = "Found {{errors.count}} errors";
        let result = render_template(template, &context).unwrap();
        assert_eq!(result, "Found 3 errors");
    }

    /// 测试：{{files.items}} 渲染为逗号分隔的条目列表。
    #[test]
    fn test_section_items() {
        let mut context = TemplateContext::new();
        context.set_section_items("files", vec!["a.rs".to_string(), "b.rs".to_string()]);

        let template = "Files: {{files.items}}";
        let result = render_template(template, &context).unwrap();
        assert_eq!(result, "Files: a.rs, b.rs");
    }

    /// 测试：未闭合的条件块返回错误。
    #[test]
    fn test_invalid_syntax() {
        let context = TemplateContext::new();

        let template = "{{#if unclosed";
        let result = render_template(template, &context);
        assert!(result.is_err());
    }
}
