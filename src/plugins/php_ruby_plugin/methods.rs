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
    /// 创建 PhpRubyPlugin 实例（名称 php_ruby，优先级 85，默认配置）。
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
    /// 返回插件名称 "php_ruby"。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级 85。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：PHP 致命错误/堆栈特征得 0.9，Ruby/Rails 特征得 0.9，Whoops 错误页得 0.95。
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

    /// 压缩切片：按配置剥离 HTML 包装标签，提取纯文本日志。
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

    /// 解压：原文透传。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    /// 归一化：剥离 HTML 并抹除 32 位 hex ID（用于 diff 比对）。
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
}

impl Clone for PhpRubyPlugin {
    /// 克隆插件实例：复制名称、优先级与配置。
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            priority: self.priority,
            config: self.config.clone(),
        }
    }
}
