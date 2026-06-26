//! plugin dispatcher 类型定义

use crate::core::compression::Token;
use crate::core::compression_context::CompressionContext;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::error_isolation::SafeExecutor;
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 压缩结果，带有生命周期以支持 Arena 内存分配
#[derive(Debug, Clone)]
pub struct CompressResult<'a> {
    pub tokens: Vec<Token<'a>>,
    pub metadata: Option<HashMap<String, String>>,
    pub plugin_name: Option<&'static str>,
}

/// 插件接口定义
pub trait Plugin: Send + Sync + Any {
    /// 返回插件名称
    fn name(&self) -> &'static str;

    /// 返回优先级（越小越优先）
    fn priority(&self) -> u8;

    /// 探测切片是否匹配该插件
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32>;

    /// 执行压缩，引入 Arena 内存池
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a>;

    /// 执行压缩（带上下文能力）。默认复用 `compress`，插件可按需重写。
    fn compress_with_context<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
        _context: &mut CompressionContext,
    ) -> CompressResult<'a> {
        self.compress(slice, dict_engine, dedup_engine, arena)
    }

    /// 执行还原
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String;

    /// 推荐的后续处理插件
    fn next_plugins(&self) -> Vec<&'static str> {
        vec![]
    }

    /// 归一化处理（用于 Diff）
    fn normalize(&self, text: &str) -> String {
        text.to_string()
    }

    /// 加载配置
    fn load_config(&mut self, _config: &dyn Any) -> Result<(), String> {
        Ok(())
    }

    /// 尝试作为脱壳器 (Unwrapper) 剥离外壳，如果成功脱壳则返回内层纯净文本，否则返回 None。
    fn unwrap(&self, _text: &str) -> Option<String> {
        None
    }
}

/// 插件调度器配置
#[derive(Clone)]
pub struct DispatcherConfig {
    pub fallback_plugin: String,
    pub plugin_timeout_ms: u64,
}

impl Default for DispatcherConfig {
    fn default() -> Self {
        Self {
            plugin_timeout_ms: 1000,
            fallback_plugin: String::new(),
        }
    }
}

/// 插件执行错误
#[derive(Debug, thiserror::Error)]
pub enum PluginExecutionError {
    #[error("E_PLUGIN_EXECUTION_PANIC")]
    Panic,
    #[error("E_PLUGIN_EXECUTION_TIMEOUT:{0:?}")]
    Timeout(std::time::Duration),
    #[error("E_PLUGIN_EXECUTION_OTHER:{0}")]
    Other(String),
}

/// 调度器错误
#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("E_DISPATCH_PLUGIN_NOT_FOUND:{0}")]
    PluginNotFound(String),
    #[error("E_DISPATCH_DUPLICATE_PLUGIN:{0}")]
    DuplicatePlugin(String),
}

use crate::core::dictionary_manager::DictionaryManager;

/// 插件调度器主结构
pub struct PluginDispatcher {
    pub(crate) plugins: Vec<Box<dyn Plugin>>,
    pub(crate) plugin_map: HashMap<String, usize>,
    #[allow(dead_code)]
    pub(crate) config: DispatcherConfig,
    #[allow(dead_code)]
    pub(crate) executor: SafeExecutor,
    #[allow(dead_code)]
    pub(crate) dict_manager: Arc<DictionaryManager>,
    pub(crate) keyword_scanner: Arc<aho_corasick::AhoCorasick>,
    pub(crate) plugin_failures: Mutex<HashMap<String, u32>>,
}
