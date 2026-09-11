//! template driven plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::borrow::Cow;

impl TemplateDrivenPlugin {
    /// 实例化并返回该插件的配置对象。
    pub fn new(config: TemplateConfig) -> Self {
        let mut compiled = Vec::new();
        for rule in &config.rules {
            if let Ok(re) = Regex::new(&rule.pattern) {
                compiled.push((re, rule.clone()));
            }
        }

        TemplateDrivenPlugin {
            name: "template_driven",
            priority: 100,
            compiled_rules: compiled,
        }
    }

    /// 辅助方法：将 Drain 模板转换为正则
    pub fn build_regex_from_template(template: &[String]) -> String {
        let mut parts = Vec::new();
        for t in template {
            if t == "<*>" {
                parts.push(r"(?P<var>.*?)".to_string());
            } else if t.chars().all(|c| c.is_ascii_hexdigit()) && t.len() > 8 {
                parts.push(r"[a-fA-F0-9]{8,}".to_string());
            } else if t.chars().all(|c| c.is_ascii_digit() || c == '.') {
                parts.push(r"[\d\.]+".to_string());
            } else {
                parts.push(regex::escape(t));
            }
        }
        format!("^{}$", parts.join(r"\s+"))
    }
}

impl Plugin for TemplateDrivenPlugin {
    /// 返回插件名称 "template_driven"。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级 100。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：任一编译规则正则命中文本得 0.9。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        for (re, _) in &self.compiled_rules {
            if re.is_match(text) {
                return Some(0.9);
            }
        }
        None
    }

    /// 压缩切片：规则命中时将捕获的变量值字典化并替换 <*> 占位，未命中时原样返回。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        for (re, rule) in &self.compiled_rules {
            if let Some(caps) = re.captures(text) {
                let mut result_line = rule.pattern.clone();

                for cap in caps.iter().skip(1) {
                    if let Some(m) = cap {
                        let val = m.as_str();
                        let token = dict_engine.add_macro(val);
                        result_line = result_line.replacen("<*>", &token, 1);
                    }
                }

                return CompressResult {
                    tokens: vec![Token::Text(Cow::Owned(result_line))],
                    metadata: None,
                    plugin_name: Some(self.name()),
                };
            }
        }

        CompressResult {
            tokens: vec![Token::Text(Cow::Borrowed(arena.alloc_str(text)))],
            metadata: None,
            plugin_name: None,
        }
    }

    /// 归一化：将文本中所有规则匹配片段替换为规则模式（用于 diff 比对）。
    fn normalize(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (re, rule) in &self.compiled_rules {
            result = re.replace_all(&result, &rule.pattern).to_string();
        }
        result
    }

    /// 解压：原文透传（模板压缩不可逆）。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }
}
