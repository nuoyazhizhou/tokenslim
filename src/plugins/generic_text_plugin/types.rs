use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct GenericTextConfig {
    pub collapse_blank_lines: bool,
    pub trim_trailing_whitespace: bool,
    pub normalize_tabs: bool,
    /// G-2 压缩矩阵：连续重复行收敛为「首行 + ×N」。
    pub collapse_repeats: bool,
    /// G-2 压缩矩阵：行首时间戳归一为占位 token（保留精度级、不保留数值）。
    pub normalize_timestamps: bool,
    /// G-2 压缩矩阵：纯装饰/纯进度噪声行裁剪（高风险，默认关）。
    pub drop_noise_lines: bool,
    /// G-2 压缩矩阵：中长重复行入 Dictionary 表替换为短引用（默认关）。
    pub enable_dictionary: bool,
    /// G-2 压缩矩阵：跨切片重复块复用 DedupEngine 引用（默认关）。
    pub enable_dedup: bool,
}

impl Default for GenericTextConfig {
    /// GenericTextConfig 默认值：轻量清理全开；压缩矩阵中「高保真」策略（重复行收敛、
    /// 时间戳归一）默认开，「高风险」策略（噪声裁剪/字典化/去重）默认关，避免过度压缩丢语义。
    fn default() -> Self {
        Self {
            collapse_blank_lines: true,
            trim_trailing_whitespace: true,
            normalize_tabs: true,
            collapse_repeats: true,
            normalize_timestamps: true,
            drop_noise_lines: false,
            enable_dictionary: false,
            enable_dedup: false,
        }
    }
}

pub struct GenericTextPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) config: GenericTextConfig,
    pub(crate) ansi_pattern: Arc<Regex>,
    /// G-2 时间戳归一：匹配行首 `HH:MM(:SS)(.millis)` 或 `YYYY-MM-DD HH:MM:SS(.millis)`。
    pub(crate) timestamp_pattern: Arc<Regex>,
}

impl GenericTextPlugin {
    /// 创建 GenericTextPlugin 实例（名称 generic_text，优先级 160），编译 ANSI 剥离与时间戳正则。
    pub fn new() -> Self {
        Self {
            name: "generic_text",
            priority: 160,
            config: GenericTextConfig::default(),
            ansi_pattern: Arc::new(
                Regex::new(r"\x1B(?:[@-Z\-_]|\[[0-?]*[ -/]*[@-~])")
                    .expect("Failed to compile ANSI regex"),
            ),
            timestamp_pattern: Arc::new(
                Regex::new(r"^(?:\d{4}-\d{2}-\d{2}[T ])?\d{1,2}:\d{2}:\d{2}(?:[.,]\d+)?\s*")
                    .expect("Failed to compile timestamp regex"),
            ),
        }
    }
}

impl Plugin for GenericTextPlugin {
    /// 返回插件名称 "generic_text"。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回插件优先级 160。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：非空文本返回低置信度 0.11（兜底插件），空文本不匹配。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        if slice.text.as_ref().trim().is_empty() {
            None
        } else {
            // 作为 run 兜底插件，仅提供低置信度。
            Some(0.11)
        }
    }

    /// 压缩切片：委托 compress_generic_text 执行通用文本压缩（含压缩矩阵）。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a> {
        crate::plugins::generic_text_plugin::methods::compress_generic_text(
            self,
            slice,
            dict_engine,
            dedup_engine,
            arena,
        )
    }

    /// 解压：原文透传（通用文本压缩无损）。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }
}
