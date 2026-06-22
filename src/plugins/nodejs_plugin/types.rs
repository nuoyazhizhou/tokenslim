//! Node.js 插件类型定义

use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::borrow::Cow;
use std::sync::Arc;

/// Node.js 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeJsConfig {
    pub strip_node_modules_paths: bool,
    pub fold_internal_frames: bool,
}

impl Default for NodeJsConfig {
    fn default() -> Self {
        Self {
            strip_node_modules_paths: true,
            fold_internal_frames: true,
        }
    }
}

/// Node.js 日志与错误分析插件
pub struct NodeJsPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) config: NodeJsConfig,
    pub(crate) error_pattern: Arc<Regex>,
}

impl NodeJsPlugin {
    pub fn new() -> Self {
        Self {
            name: "nodejs",
            priority: 90,
            config: NodeJsConfig::default(),
            error_pattern: Arc::new(Regex::new(r"(?P<type>\w+Error): (?P<msg>.*)").unwrap()),
        }
    }
}

impl Plugin for NodeJsPlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        let lower = text.to_ascii_lowercase();
        if text.contains("node_modules")
            || text.contains("Error:")
            || text.contains("at Module.")
            || lower.contains("pnpm ")
            || lower.contains("yarn ")
            || lower.contains("npm ")
            || lower.contains("jest")
            || lower.contains("eslint")
            || lower.contains("webpack")
            || lower.contains("typescript")
            || lower.contains("error ts")
            || lower.contains("vite")
            || lower.contains("vitest")
        {
            return Some(0.85);
        }
        None
    }

    fn compress<'a>(
        &self,
        slice: &Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        // Step 1: 应用高级压缩（npm install、TypeScript、ESLint、Webpack、Jest）
        let advanced_compressed = self.apply_advanced_compression(text);

        // Step 2: 路径压缩
        let optimized = crate::core::path_compressor::methods::replace_paths_in_text(
            &advanced_compressed,
            dict_engine,
        );

        CompressResult {
            tokens: vec![crate::core::compression::Token::Text(Cow::Owned(
                optimized.into_owned(),
            ))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn load_config(&mut self, config: &dyn Any) -> Result<(), String> {
        if let Some(c) = config.downcast_ref::<NodeJsConfig>() {
            self.config = c.clone();
            return Ok(());
        }
        Err("Invalid config".to_string())
    }
}

impl Clone for NodeJsPlugin {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            priority: self.priority,
            config: self.config.clone(),
            error_pattern: self.error_pattern.clone(),
        }
    }
}
