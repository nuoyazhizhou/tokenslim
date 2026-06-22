use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::any::Any;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct GenericTextConfig {
    pub collapse_blank_lines: bool,
    pub trim_trailing_whitespace: bool,
    pub normalize_tabs: bool,
}

impl Default for GenericTextConfig {
    fn default() -> Self {
        Self {
            collapse_blank_lines: true,
            trim_trailing_whitespace: true,
            normalize_tabs: true,
        }
    }
}

pub struct GenericTextPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) config: GenericTextConfig,
    pub(crate) ansi_pattern: Arc<Regex>,
}

impl GenericTextPlugin {
    pub fn new() -> Self {
        Self {
            name: "generic_text",
            priority: 160,
            config: GenericTextConfig::default(),
            ansi_pattern: Arc::new(
                Regex::new(r"\x1B(?:[@-Z\-_]|\[[0-?]*[ -/]*[@-~])")
                    .expect("Failed to compile ANSI regex"),
            ),
        }
    }
}

impl Plugin for GenericTextPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        if slice.text.as_ref().trim().is_empty() {
            None
        } else {
            // 作为 run 兜底插件，仅提供低置信度。
            Some(0.11)
        }
    }

    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        crate::plugins::generic_text_plugin::methods::compress_generic_text(self, slice)
    }

    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn load_config(&mut self, config: &dyn Any) -> Result<(), String> {
        if let Some(c) = config.downcast_ref::<GenericTextConfig>() {
            self.config = c.clone();
            return Ok(());
        }
        Err("Invalid config type for GenericTextPlugin".to_string())
    }
}
