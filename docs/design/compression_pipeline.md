# CompressionPipeline 模块功能点

> **⚠️ 文档同步状态**: 2026-03-25 已与源码对齐

## 1. 模块概述
CompressionPipeline 是微内核的核心协调模块，负责**驱动整个压缩流程**。它整合 StreamReader、LogReorderer、TextSlicer、ContentAnalyzer、PluginDispatcher、DictionaryEngine、DedupEngine、TimestampConverter、DictionaryManager 等模块，将原始文本逐步处理，最终生成压缩结果 `CompressionOutput`。

### 1.1 双路径架构（v6.1+）

CompressionPipeline 实际采用**双路径**执行策略，根据输入大小自动选择：

- **串行路径 (Serial)**: 文件 < 256KB 或启用 `reorder` 时使用。完整经过 TextSlicer → ContentAnalyzer → PluginDispatcher 分层流水线。
- **并行路径 (Parallel)**: 文件 ≥ 256KB 时使用。将数据分块（5MB/chunk）后通过 `rayon` 并行处理。每个 chunk 独立创建 DictionaryEngine、DedupEngine、TextSlicer 和 TimestampConverter，使用 `dispatch_slice_sticky` 进行"粘性调度"。

串行路径流程：
1. StreamReader → iter_lines() → TimestampConverter → LogReorderer (可选) → TextSlicer → PluginDispatcher → Token 流

并行路径流程：
1. StreamReader → get_data() → 按 5MB 分块
2. 每个 chunk 并行: line iteration → TimestampConverter → PathCompression → SharedDedupEngine → dispatch_slice_sticky → Token 流
3. 合并所有 chunk 的 Token 流

---

## 2. 数据结构（同步至源码 2026-03-25）
```rust
use crate::core::content_analyzer::{AnalyzerConfig, ContentAnalyzer};
use crate::core::dedup_engine::{DedupConfig, SharedDedupEngine};
use crate::core::dictionary_engine::DictionaryEngine;
use crate::core::dictionary_manager::DictionaryManager;
use crate::core::metrics::MetricsCollector;
use crate::core::plugin_dispatcher::{DispatcherConfig, PluginDispatcher};
use crate::core::text_slicer::{SlicerConfig, TextSlicer};
use crate::core::log_reorderer::{ReorderConfig, LogReorderer};
use std::sync::Arc;

/// 压缩流水线配置
#[derive(Clone)]
pub struct PipelineConfig {
    pub slicer_config: SlicerConfig,
    pub analyzer_config: AnalyzerConfig,
    pub dispatcher_config: DispatcherConfig,
    pub dedup_config: DedupConfig,
    pub reorder_config: ReorderConfig,
    pub stream_buffer_size: usize,
    pub parallel_threshold: usize,
    pub stream_mmap_threshold: Option<usize>,
    pub dictionary_threshold: usize,
}

/// 压缩结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionOutput {
    pub tokens: Vec<Token<'static>>,
    pub dictionary: Dictionary,
    pub metadata: CompressionMetadata,
}

/// 压缩元数据
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompressionMetadata {
    pub original_size: usize,
    pub compressed_size: usize,
    pub original_tokens: usize,
    pub compressed_tokens: usize,
    pub token_savings: usize,
    pub compression_ratio: f32,
    pub token_ratio: f32,
    pub slice_count: usize,
    pub processing_time_ms: u128,
    pub order_info: Option<OrderInfo>,
    pub base_timestamp: Option<String>,
}

/// 压缩流水线主结构
pub struct CompressionPipeline {
    slicer: TextSlicer,
    analyzer: ContentAnalyzer,
    dispatcher: PluginDispatcher,
    dict_engine: DictionaryEngine,
    dict_manager: Arc<DictionaryManager>,
    dedup_engine: Arc<SharedDedupEngine>,
    log_reorderer: LogReorderer,
    metrics: MetricsCollector,
    timestamp_converter: TimestampConverter,
    pub config: PipelineConfig,
}
```

---

## 3. 已实现功能点清单

### 3.1 初始化流水线
- **函数签名**：`pub fn new(config: PipelineConfig, plugins: Vec<Box<dyn Plugin>>, metrics: MetricsCollector) -> Self`
- **说明**：创建 Pipeline 时需要 `MetricsCollector` 参数。内部自动创建 `DictionaryManager` (Arc 共享)。
- **状态**: ✅ 已实现

### 3.2 从字符串压缩
- **函数签名**：`pub fn compress_str(&mut self, text: &str) -> Result<CompressionOutput, PipelineError>`
- **说明**：使用 `StreamReader::from_str` 创建 Reader，然后调用 `compress_stream`。
- **状态**: ✅ 已实现

### 3.3 从文件压缩
- **函数签名**：`pub fn compress_file(&mut self, path: &Path) -> Result<CompressionOutput, PipelineError>`
- **说明**：使用 `StreamReader::from_file` 创建 Reader 并调用 `compress_stream`。
- **状态**: ✅ 已实现

### 3.4 核心压缩流程（串行）
- **函数签名**：`fn compress_stream_serial(&mut self, reader: &StreamReader) -> Result<CompressionOutput, PipelineError>`
- **说明**：完整经过 TextSlicer 和 TimestampConverter / LogReorderer，支持 reorder 模式。
- **状态**: ✅ 已实现

### 3.5 核心压缩流程（并行）
- **函数签名**：`fn compress_stream_parallel(&mut self, reader: &StreamReader) -> Result<CompressionOutput, PipelineError>`
- **说明**：5MB 分块 + rayon 并行。各 chunk 创建独立的 DictionaryEngine、DedupEngine、TextSlicer 等，调用 `dispatch_slice_sticky`。
- **状态**: ✅ 已实现
- **注**: 并行路径作为 fast_path 独立优化，路径提取由 `core::path_compressor` 支持，解除了 `smart_path_plugin` 的硬编码耦合。

### 3.6 Token 融合与合并
- **`fuse_tokens_local`**: 将 Text + DictRef 合并为单个 Text Token（已将缓冲区优化为 64KB）。
- **`merge_adjacent_tokens_static`**: 合并相邻的 Text Token（全局最终合并）。
- **状态**: ✅ 已实现

### 3.7 Metrics 监控与打点
- **功能描述**: 支持在流处理前后统计行数及输入/输出大小。
- **状态**: ✅ 已实现

---

## 4. 与其它模块的交互
- **输入**：原始文本（通过 StreamReader）。
- **输出**：`CompressionOutput`，包含 Token 流、字典和元数据。
- **协作模块**：
  - StreamReader：提供输入流
  - LogReorderer：v6.1 全局日志重排序
  - TimestampConverter：相对时间戳转换
  - TextSlicer：生成 Slice
  - ContentAnalyzer：分析内容类型
  - PluginDispatcher：执行压缩
  - DictionaryEngine + DictionaryManager：管理字典（Arc 共享）
  - SharedDedupEngine：跨线程去重
  - PathCompressor：支持并行路径的高性能提前压缩
  - MetricsCollector：收集统计信息
- **错误隔离**：插件内的错误由 PluginDispatcher 处理并回退，流水线不因普通插件错误而中断。

---

## 5. 待办与注意事项
- **流式输出**: 可考虑边压边输出以处理任意大的极其庞杂的流，目前会将全结果累积于内存中。
