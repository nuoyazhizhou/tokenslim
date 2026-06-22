//! markdown plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

static LINK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[(?P<text>.*?)\]\((?P<url>https?://[^\s\)]+)\)").unwrap());
static IMG_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"!\[(?P<alt>.*?)\]\((?P<url>https?://[^\s\)]+)\)").unwrap());
static COMMENT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<!--.*?-->").unwrap());

impl MarkdownPlugin {
    /// 实例化并返回该插件的默认配置对象。
    pub fn new() -> Self {
        MarkdownPlugin {
            name: "markdown",
            priority: 110,
            config: MarkdownConfig::default(),
        }
    }
}

impl Plugin for MarkdownPlugin {
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

        let mut score: f32 = 0.0;

        // 检查标题
        if text.starts_with("# ") || text.contains("\n# ") {
            score += 0.4;
        }
        // 检查链接/图片
        if LINK_RE.is_match(text) {
            score += 0.3;
        }
        if IMG_RE.is_match(text) {
            score += 0.3;
        }
        // 检查列表
        if text.contains("\n- ") || text.contains("\n* ") || text.contains("\n1. ") {
            score += 0.2;
        }

        if score > 0.3 {
            Some(score.min(1.0))
        } else {
            None
        }
    }

    /// 执行核心的压缩与特征提取逻辑。将输入文本中的重复长字符串、路径、包名等转换为紧凑的 Token，并存入字典引擎。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        // 1. 移除注释
        let processed = if self.config.remove_comments {
            COMMENT_RE.replace_all(text, "").into_owned()
        } else {
            text.to_string()
        };

        let _ = dict_engine;

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(processed))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 对文本进行归一化处理（用于日志比对）。消除时间戳、随机 Hash、乱序参数等 Diff 噪音。
    fn normalize(&self, text: &str) -> String {
        let mut result = text.to_string();
        // 抹除 Markdown 链接中的查询参数
        let link_re = regex::Regex::new(r"(\[.*?\]\(.*?)\?.*?\)(.*?)").unwrap();
        result = link_re.replace_all(&result, "$1?...)$2").to_string();

        result
    }

    /// 执行反向的还原逻辑。利用字典引擎中存储的上下文，将压缩后的 Token 流重新展开为完整、人类可读的原始文本。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn load_config(&mut self, config: &dyn std::any::Any) -> Result<(), String> {
        if let Some(new_config) = config.downcast_ref::<MarkdownConfig>() {
            self.config = new_config.clone();
            Ok(())
        } else {
            Err("Invalid config type".to_string())
        }
    }
}
