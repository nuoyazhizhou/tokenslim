//! metrics 类型定义
//!
//! # 类型概述
//!
//! 本模块定义了 metrics 模块所需的核心数据类型。
//! 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::time::Duration;

/// 单个插件的统计信息
#[derive(Debug, Clone, Default)]
pub struct PluginStats {
    pub detect_calls: usize,
    pub compress_calls: usize,
    pub decompress_calls: usize,
    pub total_detect_time: Duration,
    pub total_compress_time: Duration,
    pub total_decompress_time: Duration,
    pub panic_count: usize,
    pub timeout_count: usize,
    pub fallback_count: usize,
}

/// 模块级耗时统计
#[derive(Debug, Clone, Default)]
pub struct ModuleTiming {
    pub stream_reader: Duration,
    pub text_slicer: Duration,
    pub content_analyzer: Duration,
    pub plugin_dispatcher: Duration,
    pub dictionary_engine: Duration,
    pub dedup_engine: Duration,
    pub compression_pipeline: Duration,
    pub rehydration_pipeline: Duration,
}

/// 错误日志条目
#[derive(Debug, Clone)]
pub struct ErrorLog {
    pub timestamp: DateTime<Utc>,
    pub module: String,
    pub plugin: Option<String>,
    pub error_type: String,
    pub message: String,
    pub slice_id: Option<u64>,
}

/// 最终指标快照
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub total_input_size: usize,
    pub total_output_size: usize,
    pub compression_ratio: f32,
    pub slice_count: usize,
    pub processing_time: Duration,
    pub module_timings: ModuleTiming,
    pub plugin_stats: HashMap<String, PluginStats>,
    pub errors: Vec<ErrorLog>,
}

#[derive(Clone, Debug)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub enable_module_timing: bool,
    pub enable_plugin_stats: bool,
    pub enable_error_logging: bool,
    pub max_error_logs: usize,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_module_timing: true,
            enable_plugin_stats: true,
            enable_error_logging: true,
            max_error_logs: 100,
        }
    }
}

/// 指标收集器主结构
pub struct MetricsCollector {
    pub(crate) config: MetricsConfig,
    pub(crate) start_time: Option<std::time::Instant>,
    pub(crate) total_input_size: usize,
    pub(crate) total_output_size: usize,
    pub(crate) slice_count: usize,
    pub(crate) module_timings: ModuleTiming,
    pub(crate) module_timings_current: HashMap<String, std::time::Instant>,
    pub(crate) plugin_stats: HashMap<String, PluginStats>,
    pub(crate) errors: Vec<ErrorLog>,
}
