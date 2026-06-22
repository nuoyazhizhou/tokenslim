//! smart path plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use std::any::Any;

impl SmartPathPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SmartPathPlugin {
    fn default() -> Self {
        Self
    }
}

impl Plugin for SmartPathPlugin {
    fn name(&self) -> &'static str {
        "smart_path"
    }
    fn priority(&self) -> u8 {
        250
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        if slice.text.contains('/') || slice.text.contains('\\') {
            Some(0.9)
        } else {
            None
        }
    }

    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();
        let optimized = crate::core::path_compressor::methods::replace_paths_in_text_scoped(
            text,
            dict_engine,
            Some(arena),
        );

        CompressResult {
            tokens: vec![Token::Text(optimized)],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn load_config(&mut self, _config: &dyn Any) -> Result<(), String> {
        Ok(())
    }
}

impl Clone for SmartPathPlugin {
    fn clone(&self) -> Self {
        SmartPathPlugin
    }
}
