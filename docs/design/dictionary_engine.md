# DictionaryEngine 模块功能点

## 1. 模块概述
DictionaryEngine 是微内核的基础服务模块，负责**可逆压缩中的字典化管理**。它维护多个命名空间（路径、包名、宏、文件名等），为文本中的重复长字符串生成唯一短 token（如 `$P1`、`$PK2`），并保证这些替换可逆。插件在压缩过程中调用 DictionaryEngine 的方法添加新的字典项，并在还原时通过 token 解析回原始字符串。

DictionaryEngine 不直接参与文本处理，而是作为插件和压缩流水线的工具，提供线程安全的字典操作、冲突检测、序列化/反序列化能力。

---

## 2. 数据结构
```rust
use std::collections::HashMap;
use std::sync::Arc;
use crate::core::dictionary_manager::DictionaryManager;

/// 字典项类型前缀
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DictType {
    Path,           // 文件完整路径，token 前缀 $P
    Package,        // 包名/命名空间，token 前缀 $PK
    Macro,          // 宏定义，token 前缀 $M
    File,           // 文件名，token 前缀 $F
    Directory,      // 公共父目录，token 前缀 $D
    Flag,           // 编译器标志（如 -I, -L），token 前缀 $FL
    Custom(String), // 自定义类型
}

/// 字典引擎主结构 (v6.1+)
/// 注意：实际采用了"局部 HashMap + 共享 Manager"的模式实现高性能并发
pub struct DictionaryEngine {
    pub(crate) paths: HashMap<String, String>,
    pub(crate) packages: HashMap<String, String>,
    pub(crate) macros: HashMap<String, String>,
    pub(crate) files: HashMap<String, String>,
    pub(crate) directories: HashMap<String, String>,
    pub(crate) flags: HashMap<String, String>,
    pub(crate) custom: HashMap<String, HashMap<String, String>>,
    pub(crate) custom_prefixes: HashMap<String, String>,
    pub(crate) next_ids: HashMap<DictType, usize>,
    pub(crate) semantic_aliases: HashMap<String, String>,
    pub(crate) manager: Option<Arc<DictionaryManager>>,
}

/// 对外暴露的字典快照（不可变，用于还原）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dictionary {
    pub paths: HashMap<String, String>,    // token -> original
    pub packages: HashMap<String, String>,
    pub macros: HashMap<String, String>,
    pub files: HashMap<String, String>,
    pub directories: HashMap<String, String>,
    pub flags: HashMap<String, String>,
    pub custom: HashMap<String, HashMap<String, String>>,
    pub aliases: HashMap<String, String>,
}

/// 错误类型
#[derive(Debug, thiserror::Error)]
pub enum DictError {
    #[error("Token conflict: {0}")]
    TokenConflict(String),
    #[error("Type not registered: {0}")]
    TypeNotRegistered(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
```

---

## 3. MVP 功能点清单

### 3.1 初始化引擎
- **功能描述**：创建新的 DictionaryEngine 实例，初始化各类型计数器。
- **函数签名**：`pub fn new() -> Self`
- **调用者**：CompressionPipeline 初始化时。
- **被调用者**：无。
- **依赖**：无。
- **测试要点**：所有计数器从 1 开始（或 0），各 map 为空。
- **优先级**：MVP

### 3.2 添加路径字典项
- **功能描述**：为给定的路径字符串生成唯一 token（如 `$P1`、`$P2`）。如果路径已存在，返回已有 token。支持最长匹配优先（路径包含公共前缀时可拆分为多个 token，但 MVP 阶段可简化：整个路径作为一个 token）。
- **函数签名**：`pub fn add_path(&mut self, original: &str) -> String`
- **调用者**：插件（如 gcc_log 插件的 compress 方法）。
- **被调用者**：内部检查 `paths` map，若不存在则生成新 token。
- **依赖**：无。
- **测试要点**：
  - 同一路径重复添加返回相同 token。
  - 不同路径返回递增 token（`$P1`、`$P2`）。
  - 路径中包含特殊字符应正确保留。
- **优先级**：MVP

### 3.3 添加包名字典项
- **功能描述**：为包名（如 `com.example.service`）生成 token，前缀 `$PK`。
- **函数签名**：`pub fn add_package(&mut self, original: &str) -> String`
- **调用者**：JavaStackPlugin 等。
- **被调用者**：同 add_path，但使用独立的 map 和计数器。
- **测试要点**：与路径 token 不冲突（`$P1` 和 `$PK1` 可共存）。
- **优先级**：MVP

### 3.4 添加宏字典项
- **功能描述**：为宏定义（如 `-DDEBUG`）生成 token，前缀 `$M`。
- **函数签名**：`pub fn add_macro(&mut self, original: &str) -> String`
- **调用者**：gcc_log 插件。
- **测试要点**：正确处理等号和值。
- **优先级**：MVP

### 3.5 添加文件名字典项
- **功能描述**：为文件名（如 `main.cpp`）生成 token，前缀 `$F`。注意文件名通常不包含路径。
- **函数签名**：`pub fn add_file(&mut self, original: &str) -> String`
- **调用者**：需要字典化文件名的插件。
- **优先级**：MVP

### 3.6 解析 token 获取原始字符串
- **功能描述**：给定 token（如 `$P1`），返回对应的原始字符串，用于还原。
- **函数签名**：`pub fn resolve(&self, token: &str) -> Option<&String>`
- **调用者**：RehydrationPipeline、插件的 decompress 方法。
- **被调用者**：在所有 map 中查找（注意 token 前缀区分类型）。
- **测试要点**：
  - 存在的 token 返回 Some。
  - 不存在的 token 返回 None。
  - 不同前缀的 token 互不干扰。
- **优先级**：MVP

### 3.7 生成只读字典快照（用于序列化）
- **功能描述**：将当前引擎的所有字典项转换为不可变的 `Dictionary` 结构，便于序列化输出。在 v6.1 中引入了 **Radix Trie 延迟路径提取**：在生成快照时对 `paths` 进行后处理，提取最大公共前缀。这种设计既保证了插入时 `DashMap` 的并发高性能，又能在序列化前实现最优的路径层级压缩（Path Layering Logic）。
- **函数签名**：`pub fn snapshot(&self) -> Dictionary`
- **调用者**：CompressionPipeline 在输出结果时调用。
- **被调用者**：遍历内部 DashMap，构建反向映射（token -> original），并执行 Radix Trie 压缩。
- **测试要点**：snapshot 包含所有已添加的项，且可序列化为 JSON。验证路径字典的嵌套压缩正确性。
- **优先级**：MVP

### 3.8 从快照重建引擎（用于还原）
- **功能描述**：根据之前保存的 `Dictionary` 重新构建 DictionaryEngine，以便在还原时使用 `resolve`。
- **函数签名**：`pub fn from_snapshot(dict: Dictionary) -> Self`
- **调用者**：RehydrationPipeline。
- **被调用者**：将传入的 map 转为正向映射（original -> token）并重建计数器（可选，计数器不影响还原）。
- **测试要点**：重建后 `resolve` 能正确返回原始字符串。
- **优先级**：MVP

### 3.9 冲突检测（内部）
- **功能描述**：生成新 token 时，确保不与已存在的 token 冲突（例如 `$P1` 和 `$P10` 可能前缀冲突）。MVP 阶段可使用固定宽度零填充（如 `$P001`）避免冲突，或简单递增数字（只要后续 resolve 时按最长匹配优先即可）。
- **函数签名**：内部逻辑。
- **优先级**：MVP（通过设计保证）

---

## 4. 未来功能点清单（待定）

### 4.1 支持自定义字典类型
- **功能描述**：允许插件注册新的字典类型（如 SQL 表名、HTML 标签），并指定 token 前缀。
- **函数签名**：`pub fn register_type(&mut self, type_name: &str, prefix: &str) -> Result<(), DictError>`
- **优先级**：未来

### 4.2 最长前缀匹配字典化
- **功能描述**：将长路径拆分为公共前缀 + 剩余部分，分别字典化，进一步提高压缩率。例如 `/home/user/project/src/main.c` 可拆分为 `/home/user/project`（`$P1`）和 `/src/main.c`（保留或二次字典化）。
- **优先级**：未来

### 4.3 字典项淘汰策略
- **功能描述**：当字典过大时，支持 LRU 等淘汰策略，但需保证可逆性（淘汰的项需记录或不再使用）。
- **优先级**：未来

### 4.4 字典加密/脱敏
- **功能描述**：支持对原始字符串进行加密或脱敏后再存储，用于敏感信息处理。
- **优先级**：未来

---

## 5. 与其它模块的交互
- **调用者**：插件的 `compress` 方法通过可变引用获取 DictionaryEngine，并调用 `add_*` 方法添加字典项。RehydrationPipeline 通过 `resolve` 方法获取原始字符串。
- **输出**：CompressionPipeline 在最终输出时通过 `snapshot` 获取 `Dictionary`，序列化后与 Token 流一起保存。
- **还原**：RehydrationPipeline 从保存的 `Dictionary` 重建引擎，供插件 `decompress` 使用。
- **线程安全**：DictionaryEngine 将在单线程流水线中使用，暂不需要 `Sync`；但若未来并行压缩多个切片，需加锁或使用线程局部字典。

---

## 6. 待办与注意事项
- **token 格式**：建议使用 `$` + 类型前缀 + 数字，如 `$P1`、`$PK2`。为避免前缀冲突（如 `$P1` 和 `$P10`），可在 resolve 时按 token 长度降序替换（最长匹配优先），这要求字典在还原时提供 token 列表并按长度排序。
- **冲突处理**：生成 token 时只需确保数字递增，不会重复；但若从快照重建，需恢复计数器至 max_id+1。
- **性能**：所有操作应为 O(1) 哈希表操作。
- **序列化格式**：`Dictionary` 结构应能直接序列化为 JSON，例如：
  ```json
  {
    "paths": { "$P1": "/home/user/project" },
    "packages": { "$PK1": "com.example" },
    ...
  }
  ```
- **可逆性**：必须保证 token 到原始字符串的一一映射，且原始字符串包含足够信息（如路径不应截断）。

