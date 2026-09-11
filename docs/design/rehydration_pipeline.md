# RehydrationPipeline 模块功能点

## 1. 模块概述
RehydrationPipeline 是微内核的还原流水线模块，位于压缩流程的逆过程，负责将 `CompressionOutput`（Token 流、字典、元数据）**恢复为原始文本**。它是对称于 CompressionPipeline 的下行模块，确保压缩过程的完全可逆。

RehydrationPipeline 接收压缩结果，通过字典解析、Token 流展开、AST 重建（可选）以及插件的 `decompress` 方法，逐级还原每个切片，最终拼接出与输入完全一致的原始文本。它需要处理 Token 流中的各种类型（`Text`、`DictRef`、`Repeat`、`Marker`、`Diff`），并协调插件的自定义解压逻辑。

---

## 2. 数据结构
```rust
use crate::core::compression::{CompressionOutput, Token};
use crate::core::dictionary_engine::Dictionary;
use crate::core::plugin_dispatcher::Plugin;

/// 还原流水线配置
#[derive(Clone)]
pub struct RehydrationConfig {
    pub preserve_order: bool,           // 是否保留重排序信息（未来）
    pub fallback_on_error: bool,        // 遇到无法还原的 token 时是否 fallback（例如跳过）
}

/// 还原流水线主结构
pub struct RehydrationPipeline {
    dict: Dictionary,
    plugins: HashMap<String, Box<dyn Plugin>>, // 插件名称到实例的映射
    config: RehydrationConfig,
}

/// 还原过程中的上下文（可选，用于跨 token 状态）
struct RehydrationContext {
    // 可能记录当前行号、缩进等
}

/// 还原错误类型
#[derive(Debug, thiserror::Error)]
pub enum RehydrationError {
    #[error("Unknown token: {0}")]
    UnknownToken(String),
    #[error("Dictionary resolution failed for token: {0}")]
    DictResolutionFailed(String),
    #[error("Plugin not found: {0}")]
    PluginNotFound(String),
    #[error("Plugin decompress failed: {0}")]
    PluginDecompressFailed(String),
    #[error("AST reconstruction failed")]
    AstReconstructionFailed,
}
```

---

## 3. MVP 功能点清单

### 3.1 初始化还原流水线
- **功能描述**：创建 RehydrationPipeline 实例，接收字典和插件列表，建立插件名称到实例的映射。
- **函数签名**：`pub fn new(dict: Dictionary, plugins: Vec<Box<dyn Plugin>>, config: RehydrationConfig) -> Self`
- **调用者**：上层（如 CLI、IDE 插件）在需要还原时调用。
- **被调用者**：内部建立 `HashMap<String, Box<dyn Plugin>>`。
- **依赖**：无。
- **测试要点**：插件映射正确，字典存储正确。
- **优先级**：MVP

### 3.2 还原 Token 流（核心）
- **功能描述**：遍历 Token 流，根据每个 Token 的类型进行还原：
  - `Token::Text(s)`：直接追加到输出字符串。
  - `Token::DictRef(token)`：通过 `dict.resolve(token)` 获取原始字符串，若失败则返回错误或 fallback。
  - `Token::Repeat { token, count }`：递归还原 `token`，然后重复 `count` 次追加。
  - `Token::Marker { kind, value }`：根据 `kind` 处理结构化标记（例如用于 AST 重建，MVP 阶段可忽略或直接追加 value）。
  - `Token::Diff { base, patch }`：先解析 `base`（通常为字典 token）得到基底文本，再应用 `patch`（`idx:old->new`）生成目标文本。
- **函数签名**：`pub fn rehydrate_tokens(&self, tokens: &[Token]) -> Result<String, RehydrationError>`
- **调用者**：外部调用者（如 CLI）。
- **被调用者**：内部递归处理 Repeat，调用 `dict.resolve`。
- **依赖**：无。
- **测试要点**：
  - 纯 Text Token → 拼接正确。
  - DictRef Token → 正确替换为原始字符串。
  - Repeat Token → 正确展开。
  - Marker Token → MVP 阶段可忽略或原样输出。
  - Diff Token → 基于字典基底正确应用 patch（普通还原与 AI 还原都一致）。
  - 混合类型 → 顺序正确。
- **优先级**：MVP

### 3.3 调用插件解压
- **功能描述**：对于某些由插件生成的特殊 Token 或整个切片，可能需要插件的 `decompress` 方法。例如，如果 Token 流中嵌入了需要插件处理的标记，或还原流程需要调用插件进行后处理。MVP 阶段可简化：假设 Token 流已足够还原，插件 `decompress` 仅在处理压缩后的文本块时调用（与 `CompressionOutput` 结构一致）。
- **函数签名**：`pub fn decompress_slice(&self, plugin_name: &str, compressed: &str) -> Result<String, RehydrationError>`
- **调用者**：当 Token 流中包含插件相关的标记时（未来），或从外部传入压缩片段时。
- **被调用者**：通过 `plugin_name` 查找插件，调用其 `decompress` 方法。
- **依赖**：插件需实现 `decompress`。
- **测试要点**：
  - 存在插件 → 返回解压结果。
  - 插件不存在 → 返回 PluginNotFound 错误。
  - 插件解压失败 → 返回 PluginDecompressFailed。
- **优先级**：未来（MVP 可暂缓，假设 Token 流自包含）

### 3.4 完整还原压缩输出
- **功能描述**：接收一个完整的 `CompressionOutput`，依次还原其 Token 流，并结合字典生成最终文本。如果存在元数据中的重排序信息，按需恢复原始顺序（未来）。
- **函数签名**：`pub fn rehydrate(&self, output: &CompressionOutput) -> Result<String, RehydrationError>`
- **调用者**：外部调用者。
- **被调用者**：`rehydrate_tokens`。
- **依赖**：无。
- **测试要点**：
  - 还原后的文本与原始文本完全一致（需有原始样本对比）。
  - 处理空输出。
- **优先级**：MVP

### 3.5 AI 导出与上下文感知过滤 (v6.1 新增)
- **功能描述**：为 LLM 推理专门设计的导出模式。提供 `rehydrate_for_ai` 方法，结合**上下文感知行过滤 (Context-Aware Line Filtering)**，去除无关噪声（如常规构建信息），同时保留包含 Error/Warning 的上下文窗口（前后 N 行）。在导出的文本首部还会**包含基础时间戳 (Base Timestamp Inclusion)** 以便计算相对耗时，极大优化了 AI 阅读 Token 消耗。
- **函数签名**：`pub fn rehydrate_for_ai(&self, output: &CompressionOutput) -> Result<String, RehydrationError>`
- **调用者**：CLI 导出为 AI 模式时（`--ai-export`）。
- **测试要点**：时间戳正确生成，无关噪音行被正确剔除，错误日志的上下文保留完整。
- **优先级**：MVP

### 3.6 处理重排序（未来）
- **功能描述**：如果压缩时启用了日志重排序（如 make -jN 按文件名排序），还原时需要根据 `CompressionMetadata` 中记录的顺序信息恢复原始乱序。MVP 阶段暂不支持。
- **函数签名**：`fn reorder(&self, text: &str, order_info: &OrderInfo) -> String`
- **优先级**：未来

### 3.7 错误处理与恢复
- **功能描述**：在还原过程中遇到无法解析的 token 或字典缺失时，根据配置决定是返回错误还是尝试跳过（例如将未知 token 原样保留）。MVP 阶段建议直接返回错误，保证可逆性。
- **内部逻辑**：在 `rehydrate_tokens` 中处理。
- **优先级**：MVP

---

## 4. 未来功能点清单（待定）

### 4.1 AST 还原
- **功能描述**：如果压缩时使用了 AST，还原时根据 AST 节点重建原始文本，优先使用节点中的 `raw_text` 保证可逆。
- **优先级**：未来

### 4.2 流式还原
- **功能描述**：支持边读取压缩结果边输出原始文本，适合超大文件。
- **优先级**：未来

### 4.3 增量还原
- **功能描述**：只还原部分切片（如调试时只查看某几行），提高效率。
- **优先级**：未来

---

## 5. 与其它模块的交互
- **输入**：接收 `CompressionOutput`（来自 CompressionPipeline），包含 Token 流、字典、元数据。
- **输出**：还原后的原始文本（`String`）。
- **依赖模块**：
  - `Dictionary`：用于解析 DictRef token。
  - `Plugin`：调用插件的 `decompress` 方法（未来）。
- **与 CompressionPipeline 对称**：确保压缩和还原的 Token 流格式一致。

---

## 6. 待办与注意事项
- **Token 流格式**：必须与 CompressionPipeline 输出的 Token 流完全一致，特别是 `Repeat` 的嵌套结构和 `Marker` 的含义。
- **Diff 一致性**：`rehydrate_tokens` 与 `rehydrate_tokens_for_ai` 必须对 `Token::Diff` 使用同等语义（先 resolve base，再 apply patch）。
- **可逆性验证**：每个 Token 类型都必须有对应的还原逻辑，且还原结果应与原始输入完全一致（包括空格、换行）。
- **性能**：还原应尽量高效，避免不必要的内存拷贝。`String` 拼接可使用 `String::with_capacity` 预分配。
- **错误处理**：字典缺失或未知 token 应视为不可恢复错误，因为这会破坏可逆性。
- **插件 decompress**：插件必须保证 `decompress(compress(slice)) == slice.raw`，且处理 `dict` 的方式与压缩时对称。
