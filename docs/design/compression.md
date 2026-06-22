# Compression 模块功能点

## 1. 模块概述
Compression 模块是微内核的基础共享模块，负责定义压缩过程中使用的核心数据结构和类型，包括 Token 流表示、压缩输出结果、压缩元数据等。该模块不包含业务逻辑，仅作为类型定义仓库，供其他模块（如 `plugin_dispatcher`、`compression_pipeline`、`rehydration_pipeline`）引用，避免循环依赖和重复定义。

Compression 模块提供：
- Token 枚举：表示压缩后的中间表示形式。
- CompressionOutput 结构：包含最终的 Token 流、字典和元数据。
- CompressionMetadata 结构：记录压缩统计信息（原大小、压缩后大小、压缩率等）。
- 其他辅助类型（如 MarkerKind、OrderInfo 等，未来扩展）。

---

## 2. 数据结构
```rust
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Token 类型，表示压缩后的文本片段
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Token {
    /// 普通文本
    Text(String),
    /// 字典引用，例如 "$P1"
    DictRef(String),
    /// 重复结构，例如重复的行或堆栈帧
    Repeat {
        token: Box<Token>,
        count: usize,
    },
    /// 结构标记，用于标识特殊内容（如堆栈帧、日志行等）
    Marker {
        kind: MarkerKind,
        value: String,
    },
    /// 差分 token：基于 base（通常是字典引用）应用 patch 还原
    /// patch 采用轻量 "idx:old->new" 逗号分隔格式。
    Diff {
        base: String,
        patch: String,
    },
}

/// 标记类型，用于增强 Token 的语义
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarkerKind {
    StackFrame,
    LogLine,
    HtmlBlock,
    CodeBlock,
    JsonBlock,
    // 未来可扩展
}

/// 压缩输出结果，由 CompressionPipeline 生成
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionOutput {
    pub tokens: Vec<Token>,
    pub dictionary: crate::core::dictionary_engine::Dictionary,
    pub metadata: CompressionMetadata,
}

/// 压缩元数据，包含统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionMetadata {
    pub original_size: usize,
    pub compressed_size: usize,
    pub compression_ratio: f32,
    pub slice_count: usize,
    pub processing_time_ms: u128,
    /// 可选的重排序信息（用于并行日志排序）
    pub order_info: Option<OrderInfo>,
}

/// 重排序信息（未来扩展）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderInfo {
    // 根据需求定义
}
```

---

## 3. MVP 功能点清单
Compression 模块在 MVP 阶段仅包含类型定义，不包含任何函数。因此功能点主要描述类型定义和其用途。

### 3.1 定义 Token 枚举
- **功能描述**：定义压缩过程中使用的 Token 类型，支持文本、字典引用、重复结构、标记和差分。
- **用途**：插件在 `compress` 方法中返回 `Vec<Token>`，CompressionPipeline 合并所有 Token，RehydrationPipeline 据此还原。
- **依赖**：无。
- **优先级**：MVP

#### Diff Token 语义补充（v6.2）
- `Token::Diff { base, patch }` 用于在已知基底文本上做轻量替换，降低重复长文本的表达成本。
- `base` 一般是字典 token（如 `$M1`），还原阶段先 `resolve` 得到基底文本。
- `patch` 采用 `idx:old->new` 逗号分隔格式（例如 `2:failed->ok`），按词位替换后输出。
- 还原约束：`rehydration_pipeline` 与 `rehydration_for_ai` 都必须支持 Diff 路径，保证行为一致。

### 3.2 定义 MarkerKind 枚举
- **功能描述**：定义 Token 标记的具体类型，用于标识内容语义（如堆栈帧、日志行等）。
- **用途**：为还原阶段提供上下文，可选，MVP 阶段可简化使用。
- **依赖**：无。
- **优先级**：MVP

### 3.3 定义 CompressionOutput 结构
- **功能描述**：封装完整的压缩结果，包括 Token 流、字典和元数据。
- **用途**：作为 CompressionPipeline 的返回类型，RehydrationPipeline 的输入。
- **依赖**：需要引用 `dictionary_engine::Dictionary`。
- **优先级**：MVP

### 3.4 定义 CompressionMetadata 结构
- **功能描述**：记录压缩相关的统计信息，用于性能分析和调试。
- **用途**：附加在 CompressionOutput 中，上层可展示给用户。
- **依赖**：无。
- **优先级**：MVP

### 3.5 定义 OrderInfo 结构（可选）
- **功能描述**：预留重排序信息结构，用于未来并行日志排序后的顺序恢复。
- **用途**：暂不实现，仅占位。
- **优先级**：未来

---

## 4. 未来功能点清单（待定）
- **序列化/反序列化辅助函数**：提供从 CompressionOutput 到 JSON 等格式的转换。
- **Token 流优化工具**：如合并连续的 Text Token 等。

---

## 5. 与其它模块的交互
- **被引用方**：`plugin_dispatcher` 使用 `Token` 类型；`compression_pipeline` 使用 `CompressionOutput`、`CompressionMetadata`；`rehydration_pipeline` 使用 `Token` 和 `CompressionOutput`。
- **依赖关系**：本模块依赖 `dictionary_engine::Dictionary`（但可通过路径引用，不构成循环依赖）。

---

## 6. 待办与注意事项
- **避免循环依赖**：确保本模块不依赖其他模块的业务逻辑，只做类型定义。
- **序列化支持**：所有类型应派生 `Serialize` 和 `Deserialize`，以便输出 JSON。
- **生命周期**：Token 等类型不包含引用，所有字段均为 owned，简化使用。
