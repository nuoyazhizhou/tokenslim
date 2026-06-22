// template_render/types.rs
// 模板渲染类型定义

use std::collections::HashMap;

/// 模板上下文 - 存储变量和 section 数据
#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    /// 简单变量映射
    variables: HashMap<String, String>,
    /// Section 内容映射
    section_contents: HashMap<String, String>,
    /// Section 计数映射
    section_counts: HashMap<String, usize>,
    /// Section 条目列表映射
    section_items: HashMap<String, Vec<String>>,
}

impl TemplateContext {
    /// 创建新的模板上下文
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置简单变量
    pub fn set_var(&mut self, name: &str, value: &str) {
        self.variables.insert(name.to_string(), value.to_string());
    }

    /// 获取简单变量
    pub fn get_var(&self, name: &str) -> Option<&str> {
        self.variables.get(name).map(|s| s.as_str())
    }

    /// 设置 section 内容
    pub fn set_section(&mut self, section: &str, content: &str) {
        self.section_contents
            .insert(section.to_string(), content.to_string());
    }

    /// 获取 section 内容
    pub fn get_section(&self, section: &str) -> Option<&str> {
        self.section_contents.get(section).map(|s| s.as_str())
    }

    /// 设置 section 计数
    pub fn set_section_count(&mut self, section: &str, count: usize) {
        self.section_counts.insert(section.to_string(), count);
    }

    /// 获取 section 计数
    pub fn get_section_count(&self, section: &str) -> Option<usize> {
        self.section_counts.get(section).copied()
    }

    /// 设置 section 条目列表
    pub fn set_section_items(&mut self, section: &str, items: Vec<String>) {
        self.section_items.insert(section.to_string(), items);
    }

    /// 获取 section 条目列表
    pub fn get_section_items(&self, section: &str) -> Option<&Vec<String>> {
        self.section_items.get(section)
    }

    /// 检查变量是否存在且非空
    pub fn is_truthy(&self, name: &str) -> bool {
        self.get_var(name).map(|v| !v.is_empty()).unwrap_or(false)
    }
}

/// 模板 Token
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateToken {
    /// 纯文本
    Text(String),
    /// 简单变量: {{var}}
    Variable(String),
    /// Section 计数: {{section.count}}
    SectionCount(String),
    /// Section 条目: {{section.items}}
    SectionItems(String),
    /// 条件块开始: {{#if var}}
    IfStart(String),
    /// 条件块结束: {{/if}}
    IfEnd,
    /// 反向条件块开始: {{#unless var}}
    UnlessStart(String),
    /// 反向条件块结束: {{/unless}}
    UnlessEnd,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_new() {
        let context = TemplateContext::new();
        assert!(context.get_var("test").is_none());
    }

    #[test]
    fn test_context_set_get_var() {
        let mut context = TemplateContext::new();
        context.set_var("name", "Alice");
        assert_eq!(context.get_var("name"), Some("Alice"));
    }

    #[test]
    fn test_context_set_get_section() {
        let mut context = TemplateContext::new();
        context.set_section("errors", "Error 1\nError 2");
        assert_eq!(context.get_section("errors"), Some("Error 1\nError 2"));
    }

    #[test]
    fn test_context_set_get_section_count() {
        let mut context = TemplateContext::new();
        context.set_section_count("errors", 5);
        assert_eq!(context.get_section_count("errors"), Some(5));
    }

    #[test]
    fn test_context_set_get_section_items() {
        let mut context = TemplateContext::new();
        let items = vec!["a".to_string(), "b".to_string()];
        context.set_section_items("files", items.clone());
        assert_eq!(context.get_section_items("files"), Some(&items));
    }

    #[test]
    fn test_context_is_truthy() {
        let mut context = TemplateContext::new();

        // 不存在的变量为 false
        assert!(!context.is_truthy("missing"));

        // 空字符串为 false
        context.set_var("empty", "");
        assert!(!context.is_truthy("empty"));

        // 非空字符串为 true
        context.set_var("present", "yes");
        assert!(context.is_truthy("present"));
    }
}
