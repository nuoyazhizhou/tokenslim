use regex::Regex;
use serde::{Deserialize, Serialize};

/// 模板规则配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateRule {
    pub name: String,
    pub pattern: String,          // 正则表达式字符串
    pub template: Option<String>, // 替换模板，如 "Error: {msg}"
    pub confidence: f32,          // 匹配时的置信度
}

/// 模板驱动插件总配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TemplateConfig {
    pub rules: Vec<TemplateRule>,
}

/// 模板驱动通用插件
pub struct TemplateDrivenPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) config: TemplateConfig,
    pub(crate) compiled_rules: Vec<(Regex, TemplateRule)>,
}
