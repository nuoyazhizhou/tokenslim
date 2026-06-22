# PluginDispatcher 模块功能点

## 1. 模块概述
PluginDispatcher 是微内核的第四层模块，位于 ContentAnalyzer 之后、CompressionPipeline 之前。它的职责是**根据内容分析结果选择最合适的插件，并驱动插件执行压缩**，同时管理插件链和异常情况。

PluginDispatcher 接收来自 ContentAnalyzer 的 `AnalysisResult` 以及对应的 `Slice`，遍历已注册的插件，根据类型匹配、置信度和插件优先级选出最佳插件（或插件链），调用其 `compress` 方法生成 `CompressResult`（包含 Token 流和元数据）。它还需要处理插件执行过程中的错误（panic、超时），并回退到默认行为，确保主流程稳定。

v6.2/v6.3 后，Dispatcher 在调度结果元数据中显式记录解析分层（parse tier）语义：
- `full`：插件链正常命中并成功执行
- `degraded`：本应由插件处理但插件执行失败，退化到文本透传
- `passthrough`：快速跳过或无候选插件，直接透传

并补充 `parse_reason` 字段，便于审计与观测。

---

## 2. 数据结构
```rust
use crate::core::text_slicer::Slice;
use crate::core::content_analyzer::AnalysisResult;
use std::any::Any;
use std::fmt;

/// 插件 trait，所有具体插件必须实现
pub trait Plugin: Send + Sync {
    /// 插件唯一名称
    fn name(&self) -> &'static str;

    /// 插件优先级（0-255，越大优先级越高）
    fn priority(&self) -> u8;

    /// 检测插件是否适合处理该切片（可选，用于二次确认）
    fn detect(&self, _slice: &Slice) -> Option<f32> {
        None
    }

    /// 压缩切片，返回压缩结果
    fn compress(&self, slice: &Slice, dict_engine: &mut DictionaryEngine, dedup_engine: &mut DedupEngine) -> CompressResult;

    /// v6.2: 上下文感知压缩（默认回退到 compress）
    fn compress_with_context(
        &self,
        slice: &Slice,
        dict_engine: &mut DictionaryEngine,
        dedup_engine: &mut DedupEngine,
        arena: &Bump,
        context: &mut CompressionContext,
    ) -> CompressResult {
        self.compress(slice, dict_engine, dedup_engine)
    }

    /// 解压缩（由还原流水线调用）
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String;

    /// 声明后续插件链（返回插件名称列表）
    fn next_plugins(&self) -> Vec<&'static str> {
        Vec::new()
    }
}

/// 插件返回的元数据（未来扩展）
pub struct Metadata;

/// 插件执行结果
pub struct CompressResult {
    pub tokens: Vec<Token>,      // 生成的 Token 流
    pub metadata: Option<Metadata>,
    pub plugin_name: Option<&'static str>,
}


/// 插件执行错误（用于内部错误隔离）
#[derive(Debug)]
pub enum PluginExecutionError {
    Panic,
    Timeout,
    InvalidResult,
}

/// 调度器配置
pub struct DispatcherConfig {
    pub fallback_plugin: String,          // 默认 fallback 插件名称
    pub plugin_timeout_ms: u64,            // 插件执行超时时间
}

/// 插件调度器主结构
pub struct PluginDispatcher {
    pub(crate) plugins: Vec<Box<dyn Plugin>>,
    pub(crate) plugin_map: HashMap<String, usize>,
    pub(crate) config: DispatcherConfig,
    pub(crate) executor: SafeExecutor,
    pub(crate) plugin_failures: Mutex<HashMap<String, u32>>,
    pub(crate) semantic_classifier: Option<Arc<SemanticClassifier>>, // AI 语义分类器
}
```

---

## 3. 核心功能实现

### 3.0 Parse Tier 标注（v6.3）
- Dispatcher 在返回结果 metadata 中写入：
  - `parse_tier = full|degraded|passthrough`
  - `parse_reason = plugin_match | sticky_plugin | quick_skip_no_keyword | plugin_failed | no_plugin_candidate`
- 目的：将“兜底是否发生、为何发生”从隐式行为变成可观测行为。

### 3.1 插件链式调用 (Plugin Chaining)
- **原理**：允许一个切片被多个插件依次流水线处理。
- **逻辑**：
    1. 插件 `A` 执行 `compress` 返回一组 Token。
    2. 调度器检查插件 `A` 的 `next_plugins()` 声明。
    3. 如果声明了插件 `B`，调度器遍历 `A` 输出的所有 `Token::Text` 类型。
    4. 将这些文本片段临时包装为 `Slice` 喂给插件 `B`。
    5. 用插件 `B` 的输出替换原来的文本 Token。
- **深度限制**：递归深度限制为 5 层，防止死循环。

### 3.2 AI 语义兜底识别 (Semantic Fallback)
- **触发条件**：当所有插件的正则表达式 `detect()` 置信度均低于阈值或返回 `None` 时。
- **逻辑**：
    1. 调用内置的 BERT 嵌入模型 (`SemanticClassifier`) 将切片转换为 384 维向量。
    2. 计算该向量与已知插件“语义指纹”库的余弦相似度。
    3. 若相似度 > 0.85，则强行委派对应的专用插件进行处理。

### 3.3 故障隔离执行 (SafeExecutor)
- **机制**：通过 `std::panic::catch_unwind` 捕获插件内部崩溃，防止主进程退出。
- **超时控制**：使用 Scoped Thread 和 Crossbeam Channel 实现毫秒级硬超时控制（默认 1000ms）。
- **熔断机制 (Circuit Breaker)**：连续失败次数超过阈值的插件将被临时加入黑名单。

### 3.4 Context 传递与按需工具化（v6.2）
- Dispatcher 调用链支持传入 `CompressionContext`。
- 插件可在 `compress_with_context` 内按需调用时间戳/路径压缩工具，而非由 Pipeline 全局硬编码预处理。

---

## 3. MVP 功能点清单

### 3.1 注册插件
- **功能描述**：向调度器添加插件实例，建立名称索引。支持在初始化时一次性传入插件列表，也可后续动态添加（未来）。
- **函数签名**：`pub fn new(plugins: Vec<Box<dyn Plugin>>, config: DispatcherConfig) -> Self`
- **调用者**：上层模块（如 CompressionPipeline）在初始化时创建调度器。
- **被调用者**：内部建立哈希映射，检查插件名称唯一性。
- **依赖**：无。
- **测试要点**：
  - 插件列表正确注册，可通过名称查询。
  - 重复名称应 panic 或返回错误（MVP 可简单 panic）。
- **优先级**：MVP

### 3.2 根据分析结果选择插件
- **功能描述**：给定 `AnalysisResult` 和 `Slice`，从已注册插件中选出最适合的插件。选择策略：
  1. 优先匹配插件声明的类型（插件通过其 `detect` 方法返回置信度，或由调度器根据类型直接映射）。
  2. 若有多个插件匹配同一类型，则按优先级排序，选优先级最高的。
  3. 若无插件匹配，则使用配置的 `fallback_plugin`（例如自然语言插件）。
- **函数签名**：`fn select_plugin(&self, result: &AnalysisResult, slice: &Slice) -> Option<&dyn Plugin>`
- **调用者**：`dispatch_slice` 内部。
- **被调用者**：遍历插件，调用 `detect`（可选）。
- **依赖**：无。
- **测试要点**：
  - 类型匹配正确。
  - 优先级生效。
  - 无匹配时返回 fallback 插件。
- **优先级**：MVP

### 3.3 执行单个插件压缩（带错误隔离）
- **功能描述**：安全地调用指定插件的 `compress` 方法，捕获 panic 和超时。若执行成功，返回 `CompressResult`；若失败，记录错误并返回 `None`，由上层决定是否 fallback。
- **函数签名**：`fn execute_plugin(&self, plugin: &dyn Plugin, slice: &Slice, dict: &mut Dictionary) -> Option<CompressResult>`
- **调用者**：`dispatch_slice` 内部。
- **被调用者**：使用 `std::panic::catch_unwind` 捕获 panic，并加入超时控制（可用线程或 future）。
- **依赖**：可能需要 `std::panic` 和超时机制（如 `std::thread::spawn` 加 channel）。
- **测试要点**：
  - 正常插件返回正确结果。
  - 插件 panic 被捕获，返回 None，错误记录。
  - 插件超时被中止，返回 None。
- **优先级**：MVP

### 3.4 完整调度一个切片
- **功能描述**：对外暴露的主方法，接收 `Slice` 和 `AnalysisResult`，执行以下步骤：
  1. 调用 `select_plugin` 获取初始插件。
  2. 调用 `execute_plugin` 执行插件压缩。
  3. 如果执行失败或返回 None，则使用 fallback 插件重试一次。
  4. 如果插件声明了 `next_plugins`，则依次执行后续插件，并将前一个插件的输出作为输入传递给下一个（需定义链式传递方式，MVP 可先不支持链或简化）。
  5. 返回最终的 `CompressResult`（可能合并多个插件的结果）。
- **函数签名**：`pub fn dispatch_slice(&self, slice: &Slice, result: &AnalysisResult, dict: &mut Dictionary) -> CompressResult`
- **调用者**：CompressionPipeline 的主循环。
- **被调用者**：上述内部函数。
- **依赖**：无。
- **测试要点**：
  - 正常情况返回插件压缩结果。
  - 插件失败后 fallback 生效。
  - 多个切片独立调度。
- **优先级**：MVP

### 3.5 获取插件信息（可选）
- **功能描述**：返回所有已注册插件的名称和优先级，用于调试或配置检查。
- **函数签名**：`pub fn list_plugins(&self) -> Vec<(&str, u8)>`
- **调用者**：管理工具或测试。
- **被调用者**：无。
- **依赖**：无。
- **测试要点**：返回列表与注册一致。
- **优先级**：未来（MVP 可暂缓）

### 3.6 动态添加插件（未来）
- **功能描述**：运行时添加新的插件，更新索引。
- **函数签名**：`pub fn add_plugin(&mut self, plugin: Box<dyn Plugin>) -> Result<(), String>`
- **调用者**：插件管理器。
- **被调用者**：检查名称唯一性，插入 plugins 和 map。
- **依赖**：无。
- **测试要点**：添加后可用。
- **优先级**：未来

---

## 4. 未来功能点清单（待定）

### 4.1 插件链支持
- **功能描述**：完整实现 `next_plugins` 机制，允许一个插件处理后，将结果传递给链中的下一个插件。需要定义中间数据的传递方式（例如修改同一个 `CompressResult` 或生成新结果）。
- **优先级**：未来

### 4.2 插件超时配置细化
- **功能描述**：允许为每个插件单独配置超时时间，而非全局统一。
- **优先级**：未来

### 4.3 插件热加载
- **功能描述**：支持从动态库或 WASM 加载插件，实现热插拔。
- **优先级**：未来

### 4.4 插件依赖解析
- **功能描述**：处理插件之间的依赖关系，确保执行顺序正确。
- **优先级**：未来

---

## 5. 插件的开发与存放位置
插件本身是独立的实现模块，**不放在 PluginDispatcher 目录下**。每个插件应有自己独立的目录，位于 `src/plugins/` 下，例如：

```
src/plugins/
├── mod.rs               # 插件模块导出
├── gcc_log/             # gcc 日志插件
│   ├── mod.rs
│   ├── struct.rs
│   └── impl.rs
├── java_stack/          # Java 堆栈插件
│   ├── mod.rs
│   └── ...
└── natural_language/    # 自然语言插件（fallback）
    └── ...
```

PluginDispatcher 在初始化时，由上层模块（如 CompressionPipeline）负责收集所有插件实例，并传递给 `PluginDispatcher::new()`。这种方式保持调度器与插件实现的解耦，便于后续动态加载。

插件的具体实现必须实现 `Plugin` trait，并通常会在其 `mod.rs` 中导出工厂函数或直接导出插件结构体。MVP 阶段可采用静态注册（在代码中显式创建插件列表），未来可支持动态发现。

---

## 6. 与其它模块的交互
- **输入**：
  - 从 ContentAnalyzer 接收 `AnalysisResult` 和对应的 `Slice`。
  - 在初始化时接收插件列表（由上层模块从 `plugins/` 收集）。
- **输出**：生成 `CompressResult` 传递给 CompressionPipeline。
- **错误隔离**：内部使用 `SafeExecutor` 模式捕获插件异常，不影响主流程。
- **字典传递**：`compress` 方法接收可变的 `Dictionary` 引用，插件可向其中添加字典项。

---

## 7. 待办与注意事项
- **插件 trait 设计**：`detect` 方法在 MVP 阶段可能用不到，但保留以便后续扩展。`compress` 和 `decompress` 必须成对实现，保证可逆性。
- **错误处理**：插件 panic 应记录日志，并使用 fallback 插件。需要定义 fallback 插件的存在性（如自然语言插件必须注册）。
- **性能**：插件选择过程应高效，避免每次调度都遍历所有插件。可用类型到插件的映射缓存优化。
- **线程安全**：`Plugin` trait 要求 `Send + Sync`，以便调度器在多线程环境下共享。
- **超时实现**：MVP 阶段可用简单的 `std::thread::spawn` 加 `recv_timeout` 模拟，但注意线程开销。更优雅的方式是使用异步运行时（如 `tokio`），但会增加复杂度。
- **与 ContentAnalyzer 的类型对齐**：插件应声明自己能够处理的类型，调度器需要根据 `AnalysisResult.slice_type` 匹配。目前 `SliceType` 枚举由 TextSlicer 定义，插件需引用该类型。


