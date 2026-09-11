//! xml html plugin 方法实现
//!
//! # 方法概述
//!
//! 本模块实现了 xml html plugin 模块的主要业务逻辑。
//! 包含所有公共 API 的实现，以及内部辅助函数。

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_config_loader::CompiledPluginConfig;
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use std::borrow::Cow;

impl Default for XmlHtmlPlugin {
    /// 提供该插件类型的默认配置实现。
    fn default() -> Self {
        Self::new()
    }
}

impl XmlHtmlPlugin {
    /// 实例化并返回该插件的默认配置对象。
    pub fn new() -> Self {
        XmlHtmlPlugin {
            name: "xml_html",
            priority: 140, // 优先级较低
            config: None,
        }
    }

    /// 从配置文件加载
    pub fn with_config(config: CompiledPluginConfig) -> Self {
        XmlHtmlPlugin {
            name: Box::leak(config.name.clone().into_boxed_str()) as &'static str,
            priority: config.priority,
            config: Some(config),
        }
    }
}

impl Plugin for XmlHtmlPlugin {
    /// 返回插件的唯一标识名称，用于日志记录和监控。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回插件的执行优先级。数值越小，执行调度越靠前。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 分析输入的文本切片，检测是否符合当前插件的处理特征，并返回一个 0.0 到 1.0 的置信度（Confidence）。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();

        let mut sample_len = text.len().min(1000);
        while sample_len > 0 && !text.is_char_boundary(sample_len) {
            sample_len -= 1;
        }
        let sample = &text[..sample_len];

        // 简易探测：如果包含大量的 <...> 并且不是偶然的
        let tag_open_count = sample.matches('<').count();
        let tag_close_count = sample.matches('>').count();
        let tag_slash_count = sample.matches("</").count();

        if tag_open_count > 1 && tag_close_count > 1 && tag_slash_count >= 1 {
            // 尝试使用 quick_xml 解析一下前部
            use quick_xml::Reader;
            let mut reader = Reader::from_str(sample);
            if let Ok(_) = reader.read_event() {
                return Some(0.8);
            }
        }
        None
    }

    /// 对 XML/HTML 文本做轻量压缩：用正则把标签之间的空白（`>\s+<`）合并为 `><`，去除换行与缩进。
    /// 这是半有损压缩——被丢弃的换行无法完美还原；不抽取 Token、不写字典引擎，仅输出单个 `Token::Text`。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine, // 未来可用
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        // 为保证极致速度，MVP阶段的压缩策略：
        // 去除 XML 标签之间的空白（换行和缩进）。由于 XML 工具链复杂，直接字符串替换：
        // 将 `>\s+<` 替换为 `><`
        let regex = regex::Regex::new(r">\s+<").unwrap();
        let compressed = regex.replace_all(text, "><").into_owned();

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(compressed))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 还原压缩结果。由于 `compress` 丢弃了标签间换行（半有损），无法完美恢复原始缩进，
    /// 这里直接原样返回压缩文本（未使用字典引擎 `_dict`）。后续可接入美化算法提升可读性。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        // 由于 XML 的换行是被我们丢弃的，这属于半有损压缩，无法完美恢复原缩进。
        // 在这我们可以尝试使用一个快速美化算法或者原样返回
        compressed.to_string()
    }

    /// 返回当前插件执行完毕后，推荐调度器优先尝试执行的后续插件列表（构建处理管道）。
    fn next_plugins(&self) -> Vec<&'static str> {
        vec!["smart_path"]
    }
}
