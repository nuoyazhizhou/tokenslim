use crate::core::content_analyzer::ContentAnalyzer;
use crate::core::debug_audit::DebugAuditAttribution;
use crate::core::dedup_engine::{DedupConfig, SharedDedupEngine};
use crate::core::dictionary_engine::DictionaryEngine;
use crate::core::dictionary_manager::DictionaryManager;
use crate::core::log_reorderer::{LogReorderer, ReorderConfig};
use crate::core::metrics::MetricsCollector;
use crate::core::plugin_dispatcher::{DispatcherConfig, PluginDispatcher};
use crate::core::text_slicer::{SlicerConfig, TextSlicer};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct PipelineConfig {
    pub slicer_config: SlicerConfig,
    pub dispatcher_config: DispatcherConfig,
    pub dedup_config: DedupConfig,
    pub reorder_config: ReorderConfig,
    pub stream_buffer_size: usize,
    pub parallel_threshold: usize,
    pub dictionary_threshold: usize,
    pub debug_audit_jsonl: Option<PathBuf>,
    pub debug_audit_attribution: DebugAuditAttribution,
}

impl Default for PipelineConfig {
    /// 构建默认管道配置：各子配置取 `Default`，流缓冲 8KiB、并行阈值 1MiB，
    /// 字典阈值 0（默认禁用）、调试审计 JSONL 未启用。
    fn default() -> Self {
        Self {
            slicer_config: SlicerConfig::default(),
            dispatcher_config: DispatcherConfig::default(),
            dedup_config: DedupConfig::default(),
            reorder_config: ReorderConfig::default(),
            stream_buffer_size: 8 * 1024,
            parallel_threshold: 1024 * 1024,
            dictionary_threshold: 0,
            debug_audit_jsonl: None,
            debug_audit_attribution: DebugAuditAttribution::default(),
        }
    }
}

pub use crate::core::compression::CompressionMetadata;
pub use crate::core::compression::CompressionOutput;

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("E_PIPELINE_STREAM:{0}")]
    Stream(#[from] crate::core::stream_reader::StreamError),
    #[error("E_PIPELINE_SLICER:{0}")]
    Slicer(String),
    #[error("E_PIPELINE_DISPATCHER:{0}")]
    Dispatcher(String),
    #[error("E_PIPELINE_DICTIONARY:{0}")]
    Dictionary(#[from] crate::core::dictionary_engine::DictError),
    #[error("E_PIPELINE_DEDUP:{0}")]
    Dedup(String),
    #[error("E_PIPELINE_IO:{0}")]
    Io(#[from] std::io::Error),
    #[error("E_PIPELINE_CUSTOM:{0}")]
    Custom(String),
}

pub struct CompressionPipeline {
    pub(crate) slicer: TextSlicer,
    pub(crate) analyzer: ContentAnalyzer,
    pub(crate) dispatcher: PluginDispatcher,
    pub(crate) dict_engine: DictionaryEngine,
    pub(crate) dict_manager: Arc<DictionaryManager>,
    pub(crate) dedup_engine: Arc<SharedDedupEngine>,
    #[allow(dead_code)]
    pub(crate) log_reorderer: LogReorderer,
    pub(crate) metrics: MetricsCollector,
    pub(crate) processing_context: crate::core::compression_context::CompressionContext,
    pub config: PipelineConfig,
}
