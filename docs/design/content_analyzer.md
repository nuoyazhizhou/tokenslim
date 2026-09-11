# ContentAnalyzer 模块功能点

## 1. 模块概述
ContentAnalyzer 是微内核的第三层模块，位于 TextSlicer 之后、PluginDispatcher 之前。它的职责是**对每个 `Slice` 进行内容类型识别**，输出该 `Slice` 的最终类型（如 `gcc_log`、`java_stack`、`html` 等）以及置信度，供 PluginDispatcher 选择最合适的插件。

ContentAnalyzer 接收 TextSlicer 输出的 `Slice` 流，利用规则识别（关键词、正则）以及可选的 embedding 相似度，为每个 `Slice` 生成 `AnalysisResult`。它不负责压缩或还原，只做类型推断，并且应尽量保持无状态（或轻量状态）和高性能。

---

## 2. 数据结构
```rust
use crate::core::text_slicer::SliceType;  // 复用 TextSlicer 的 SliceType

/// 分析结果
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub slice_type: SliceType,   // 最终推断的类型
    pub confidence: f32,          // 0.0 ~ 1.0
    pub details: Option<String>,  // 调试信息（可选）
}

/// 单个规则的定义
pub struct Rule {
    pub name: String,
    pub pattern: regex::Regex,    // 或更高效的匹配器
    pub type_on_match: SliceType,
    pub weight: f32,               // 匹配时增加的置信度
}

/// 内容分析器配置
#[derive(Clone)]
pub struct AnalyzerConfig {
    pub enable_rules: bool,
    pub enable_embedding: bool,    // 未来
    pub rules: Vec<Rule>,           // 规则列表
    pub fallback_type: SliceType,   // 当无规则匹配时的默认类型
    pub confidence_threshold: f32,  // 低于此阈值则 fallback
}

/// 内容分析器主结构
pub struct ContentAnalyzer {
    config: AnalyzerConfig,
    // 可能缓存 embedding 向量等（未来）
}
```

---

## 3. MVP 功能点清单

### 3.1 初始化与配置
- **功能描述**：创建 `ContentAnalyzer` 实例，接受 `AnalyzerConfig`，初始化规则引擎。
- **函数签名**：`pub fn new(config: AnalyzerConfig) -> Self`
- **调用者**：上层模块（如 CompressionPipeline）。
- **被调用者**：无。
- **依赖**：无。
- **测试要点**：配置正确应用，规则编译成功。
- **优先级**：MVP

### 3.2 规则识别（核心）
- **功能描述**：对给定的 `Slice`，遍历所有规则，计算匹配得分，并选出最高置信度的类型。规则可以基于关键词、正则、行首特征等。匹配时累加权重，可多次匹配同一规则（或只计一次）。最终返回 `AnalysisResult`，包含类型和置信度。
- **函数签名**：`pub fn analyze<'a>(&self, slice: &Slice<'a>) -> AnalysisResult`
- **调用者**：PluginDispatcher 或外部调用者。
- **被调用者**：内部规则匹配引擎。
- **依赖**：`regex` crate（用于正则规则）。
- **测试要点**：
  - 单条规则匹配 → 返回对应类型和权重。
  - 多条规则匹配 → 累加置信度，返回最高分类型。
  - 无规则匹配 → 返回配置的 `fallback_type`，置信度 0.0。
  - 正则规则应正确处理特殊字符。
- **优先级**：MVP

### 3.3 置信度归一化
- **功能描述**：将累加的得分归一化到 [0.0, 1.0] 区间（例如除以最大可能得分或使用 sigmoid）。MVP 阶段可简单使用 `min(score, 1.0)` 或直接返回原始分（要求规则权重总和不超过 1）。
- **函数签名**：内部函数，不对外暴露。
- **调用者**：`analyze` 内部。
- **被调用者**：无。
- **依赖**：无。
- **测试要点**：归一化后的置信度在 [0,1] 内。
- **优先级**：MVP

### 3.4 获取支持的规则列表（可选）
- **功能描述**：返回当前加载的所有规则名称和对应类型，用于调试或配置检查。
- **函数签名**：`pub fn list_rules(&self) -> Vec<(&str, SliceType)>`
- **调用者**：管理工具。
- **被调用者**：无。
- **依赖**：无。
- **测试要点**：返回的列表与配置一致。
- **优先级**：未来（MVP 可暂缓）

### 3.5 动态更新规则（未来）
- **功能描述**：允许在运行时添加、删除或修改规则，无需重启。
- **函数签名**：`pub fn update_rules(&mut self, new_rules: Vec<Rule>) -> Result<(), AnalyzerError>`
- **调用者**：管理接口。
- **被调用者**：规则引擎重新编译。
- **依赖**：无。
- **测试要点**：更新后 `analyze` 使用新规则。
- **优先级**：未来

---

## 4. 未来功能点清单（待定）

### 4.1 Embedding 识别
- **功能描述**：利用本地 embedding 模型（如 ONNX 运行时的轻量模型）对 `Slice` 文本生成向量，与预先计算的类型向量中心比较相似度，作为规则识别的补充。
- **依赖**：`candle` 或 `onnxruntime`，以及预训练模型文件。
- **优先级**：未来

### 4.2 置信度融合
- **功能描述**：融合规则识别和 embedding 识别的结果，例如加权平均或最大值，输出最终置信度。
- **优先级**：未来

### 4.3 上下文感知识别
- **功能描述**：在分析当前 `Slice` 时，参考前后几个 `Slice` 的类型或内容，提高准确率（例如堆栈片段可能连续出现）。
- **优先级**：未来

### 4.4 插件化规则
- **功能描述**：允许第三方以插件形式提供自定义规则，动态加载。
- **优先级**：未来

---

## 5. 与其它模块的交互
- **输入**：从 TextSlicer 接收 `Slice` 流。`Slice` 包含文本、初步类型、位置信息和元数据。初步类型（`slice_type`）可被 ContentAnalyzer 作为参考，但不强制使用。
- **输出**：产生 `AnalysisResult` 流，每个结果包含最终类型和置信度。这些结果将传递给 PluginDispatcher，用于选择插件。
- **元数据传递**：`Slice` 中的 `file_metadata` 可能被规则使用（例如根据文件路径判断语言），但 MVP 阶段可不依赖。

---

## 6. 待办与注意事项
- **规则设计**：规则应尽量精确，避免误判。例如 Java 堆栈可用 `^at [a-zA-Z0-9_.]+\(` 正则匹配；gcc 日志可用 `error:`、`warning:` 关键词。
- **性能**：规则匹配应高效，可预编译正则，使用 Aho-Corasick 等多模式匹配加速关键词扫描。
- **置信度阈值**：需确定合理的 fallback 阈值（如 0.5），低于阈值则使用 `fallback_type`。
- **与 TextSlicer 的协作**：`SliceType` 枚举应与 TextSlicer 保持一致，以便在 ContentAnalyzer 中直接使用。目前 TextSlicer 的 `SliceType` 包含 `Line`、`Paragraph` 等初步类型，ContentAnalyzer 可以将其作为特征之一，但最终类型可能更精细（如 `gcc_log`、`java_stack`）。建议将 `SliceType` 扩展为包含所有可能类型（包括初步类型和最终类型），或单独定义最终类型枚举。MVP 阶段可先复用 `SliceType`，但未来可能需要分离。
- **错误处理**：如果规则编译失败，应在初始化时返回错误，避免运行时 panic。

---

## 7. 对 TextSlicer 模块接口的检查与建议

基于 ContentAnalyzer 的需求，检查 TextSlicer 的接口：

- **`Slice` 结构体**：
  - `text` 使用 `Cow<'a, str>` 完美满足需求，既能借用又能拥有。
  - `slice_type` 字段提供初步类型，ContentAnalyzer 可以将其作为特征之一，但 ContentAnalyzer 的输出类型可能与初步类型不同，因此保留该字段是有益的。
  - `offset`、`line_start`、`line_end` 对某些规则可能有用（例如根据行号范围判断），但 MVP 阶段不一定需要。保留无妨。
  - `file_metadata` 对某些规则（如根据文件扩展名判断语言）非常有用，应保留。

- **切片策略**：MVP 阶段 TextSlicer 只实现按行切片和按空行切片，这已足够生成初步的 `Slice`。ContentAnalyzer 可以通过规则识别出更具体的类型（如 gcc 日志），因此无需在切片阶段过度细分。

- **潜在改进**：目前 `SliceType` 枚举同时包含初步类型（如 `Line`）和未来可能出现的最终类型（如 `LogBlock`），这可能导致命名冲突。建议将初步类型和最终类型分离，或至少明确区分。但 MVP 阶段可以先用一个枚举，后续再重构。

**建议**：暂时保持 TextSlicer 接口不变，ContentAnalyzer 直接使用现有 `Slice`。未来如果发现 `slice_type` 字段冗余，可以移除或改为可选。