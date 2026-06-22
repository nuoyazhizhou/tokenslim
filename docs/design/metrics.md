# Metrics 模块功能点

## 1. 模块概述
Metrics 模块负责收集和记录微内核各个模块及插件的运行指标，包括处理时间、调用次数、错误计数、压缩率等。这些数据可用于性能分析、监控、调试以及后续优化。模块应提供轻量、无侵入的埋点接口，并支持输出格式化的报告（如 JSON、Prometheus 格式）。

Metrics 不直接影响核心逻辑，仅作为可观测性组件存在。

---

## 2. 数据结构
```rust
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
    // 可扩展
}

/// 错误日志条目
#[derive(Debug, Clone)]
pub struct ErrorLog {
    pub timestamp: chrono::DateTime<chrono::Utc>,
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

/// 指标收集器配置
#[derive(Clone)]
pub struct MetricsConfig {
    pub enable_module_timing: bool,
    pub enable_plugin_stats: bool,
    pub enable_error_logging: bool,
    pub max_error_logs: usize,        // 最多保存多少条错误日志
}

/// 指标收集器主结构
pub struct MetricsCollector {
    config: MetricsConfig,
    start_time: Option<std::time::Instant>,
    total_input_size: usize,
    total_output_size: usize,
    slice_count: usize,
    module_timings: ModuleTiming,
    module_timings_current: HashMap<String, std::time::Instant>, // 记录模块开始时间
    plugin_stats: HashMap<String, PluginStats>,
    errors: Vec<ErrorLog>,
}
```

---

## 3. MVP 功能点清单

### 3.1 初始化收集器
- **功能描述**：创建 MetricsCollector 实例，重置所有统计信息。
- **函数签名**：`pub fn new(config: MetricsConfig) -> Self`
- **调用者**：CompressionPipeline 或上层应用。
- **被调用者**：无。
- **依赖**：无。
- **测试要点**：配置正确应用，所有计数器归零。
- **优先级**：MVP

### 3.2 记录整体输入输出大小
- **功能描述**：设置或累加原始输入大小和压缩后输出大小，用于计算压缩率。
- **函数签名**：
  - `pub fn set_input_size(&mut self, size: usize)`
  - `pub fn set_output_size(&mut self, size: usize)`
  - `pub fn add_slice_count(&mut self, count: usize)`
- **调用者**：CompressionPipeline 在开始和结束时调用。
- **测试要点**：数值累加正确。
- **优先级**：MVP

### 3.3 模块耗时测量（计时器）
- **功能描述**：提供开始和结束模块计时的接口，自动累加耗时。
- **函数签名**：
  - `pub fn start_module(&mut self, module_name: &str)`
  - `pub fn end_module(&mut self, module_name: &str)`
- **调用者**：各模块在其处理前后调用。
- **被调用者**：内部使用 `std::time::Instant` 记录时间差，累加到 `module_timings` 对应字段。
- **测试要点**：多次调用累加正确，嵌套调用不影响（假设不嵌套，若嵌套需设计栈，但 MVP 可假设串行）。
- **优先级**：MVP

### 3.4 记录插件调用
- **功能描述**：记录插件的调用次数和耗时。提供开始/结束方法，或直接记录单次耗时。
- **函数签名**：
  - `pub fn record_plugin_detect(&mut self, plugin_name: &str, duration: Duration)`
  - `pub fn record_plugin_compress(&mut self, plugin_name: &str, duration: Duration)`
  - `pub fn record_plugin_decompress(&mut self, plugin_name: &str, duration: Duration)`
  - `pub fn inc_plugin_panic(&mut self, plugin_name: &str)`
  - `pub fn inc_plugin_timeout(&mut self, plugin_name: &str)`
  - `pub fn inc_plugin_fallback(&mut self, plugin_name: &str)`
- **调用者**：PluginDispatcher 在插件执行后调用。
- **测试要点**：统计数据正确累加。
- **优先级**：MVP

### 3.5 记录错误日志
- **功能描述**：记录发生的错误，包括模块、插件（可选）、错误类型、消息、关联的 slice_id。
- **函数签名**：`pub fn log_error(&mut self, error: ErrorLog)`
- **调用者**：各模块在捕获到错误时调用。
- **测试要点**：错误日志被正确添加，超过 `max_error_logs` 时可丢弃旧日志或保留最近。
- **优先级**：MVP

### 3.6 生成指标快照
- **功能描述**：收集当前所有指标，生成 MetricsSnapshot，可用于输出或展示。
- **函数签名**：`pub fn snapshot(&self) -> MetricsSnapshot`
- **调用者**：CompressionPipeline 结束时调用。
- **测试要点**：快照包含所有已记录的指标。
- **优先级**：MVP

### 3.7 重置收集器
- **功能描述**：清空所有统计信息，准备处理新的任务。
- **函数签名**：`pub fn reset(&mut self)`
- **调用者**：若复用收集器，在处理新文件前调用。
- **测试要点**：重置后所有数据归零。
- **优先级**：MVP（可选）

---

## 4. 未来功能点清单（待定）
- **Prometheus 导出**：直接输出 Prometheus 格式的指标。
- **实时流式指标**：支持通过 channel 推送实时指标。
- **指标聚合**：跨多个文件或请求聚合指标。
- **自定义标签**：允许用户添加自定义标签（如项目名、环境）。

---

## 5. 与其它模块的交互
- **调用者**：CompressionPipeline、PluginDispatcher、各模块等。
- **数据流向**：各模块调用收集器的方法记录数据，最终由上层输出或展示。
- **无侵入**：收集器仅在被调用时记录，不参与核心逻辑。

---

## 6. 待办与注意事项
- **性能**：记录操作应尽可能轻量，避免使用锁等影响性能（单线程环境可不用锁）。
- **时间测量**：使用 `std::time::Instant`，精度足够。
- **错误日志数量**：限制错误日志数量，防止内存无限增长。
- **模块名标准化**：定义一组常量模块名，避免字符串硬编码。