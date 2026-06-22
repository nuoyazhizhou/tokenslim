use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::borrow::Cow;

impl PhpRubyPlugin {
    pub fn new() -> Self {
        Self {
            name: "php_ruby",
            priority: 85,
            config: PhpRubyConfig::default(),
        }
    }

    /// 剥离 HTML 标签以提取纯文本日志
    fn strip_html(&self, text: &str) -> String {
        let re = Regex::new(r"<[^>]*>").unwrap();
        re.replace_all(text, "").into_owned()
    }
}

impl Plugin for PhpRubyPlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();

        // 1. PHP 特征
        if text.contains("Fatal error:")
            || text.contains("PHP Stack trace:")
            || text.contains("Uncaught Error:")
        {
            return Some(0.9);
        }

        // 2. Ruby/Rails 特征
        if text.contains("ActionView::Template::Error")
            || text.contains(".rb:")
            || text.contains("rake aborted!")
        {
            return Some(0.9);
        }

        // 3. HTML 错误页面特征 (Whoops, Ignition)
        if text.contains("<title>Whoops!")
            || text.contains("exception_title")
            || text.contains("sf-stacktrace")
        {
            return Some(0.95);
        }

        None
    }

    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let mut text = slice.text.as_ref().to_string();

        // 1. 如果包含 HTML 标签则尝试剥离
        if self.config.strip_html_wrappers && (text.contains("<html>") || text.contains("<div")) {
            text = self.strip_html(&text);
        }

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn normalize(&self, text: &str) -> String {
        let cleaned = if self.config.strip_html_wrappers {
            self.strip_html(text)
        } else {
            text.to_string()
        };
        // 抹除十六进制 32 位 ID
        let re = Regex::new(r"\b[0-9a-f]{32}\b").unwrap();
        re.replace_all(&cleaned, "[ID]").to_string()
    }

    fn load_config(&mut self, config: &dyn std::any::Any) -> Result<(), String> {
        if let Some(new_config) = config.downcast_ref::<PhpRubyConfig>() {
            self.config = new_config.clone();
            Ok(())
        } else {
            Err("Invalid config type".to_string())
        }
    }
}

impl Clone for PhpRubyPlugin {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            priority: self.priority,
            config: self.config.clone(),
        }
    }
}
