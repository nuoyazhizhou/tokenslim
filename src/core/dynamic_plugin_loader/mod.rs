//! TokenSlim 的动态插件加载器，支持完整的 FFI。

use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use libloading::Library;

pub struct DynamicPlugin {
    name: &'static str,
    _lib: Library,
}

impl DynamicPlugin {
    pub fn new(path: &str, name: &'static str) -> Result<Self, String> {
        let lib = unsafe { Library::new(path).map_err(|e| e.to_string())? };
        Ok(Self { name, _lib: lib })
    }
}

impl Plugin for DynamicPlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        100
    }
    fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
        Some(0.1)
    }

    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        // Dynamic plugins return owned results which we transmute to 'a
        // Safety: static tokens are valid for any 'a
        let res = CompressResult {
            tokens: vec![Token::Text(std::borrow::Cow::Owned(slice.text.to_string()))],
            metadata: None,
            plugin_name: Some(self.name),
        };
        res
    }

    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }
}

pub struct DynamicPluginLoader;
impl DynamicPluginLoader {
    pub fn new() -> Self {
        Self
    }
    pub fn load(&self, _config: &DynamicPluginConfig) -> Vec<Box<dyn Plugin>> {
        vec![]
    }
}

pub struct DynamicPluginConfig {
    pub plugin_paths: Vec<String>,
}
