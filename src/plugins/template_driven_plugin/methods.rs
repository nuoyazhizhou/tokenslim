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
            config,
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
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        for (re, _) in &self.compiled_rules {
            if re.is_match(text) {
                return Some(0.9);
            }
        }
        None
    }

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

    fn normalize(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (re, rule) in &self.compiled_rules {
            result = re.replace_all(&result, &rule.pattern).to_string();
        }
        result
    }

    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn load_config(&mut self, config: &dyn std::any::Any) -> Result<(), String> {
        if let Some(cfg) = config.downcast_ref::<TemplateConfig>() {
            self.config = cfg.clone();
            self.compiled_rules.clear();
            for rule in &self.config.rules {
                if let Ok(re) = Regex::new(&rule.pattern) {
                    self.compiled_rules.push((re, rule.clone()));
                }
            }
            return Ok(());
        }
        Err("Invalid config type".to_string())
    }
}
