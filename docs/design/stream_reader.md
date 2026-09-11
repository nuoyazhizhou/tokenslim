# StreamReader 模块功能点

## 1. 模块概述
StreamReader 是微内核的最底层模块，负责从不同来源（文件、内存字符串等）以流式方式读取文本数据，并提供统一的迭代器接口。其主要职责包括：

- 打开文件并读取其元数据（路径、大小、时间、权限等）。
- 检测文件是否为二进制文件，避免处理不可读内容。
- [x] 使用内存映射（mmap）实现零拷贝大文件读取，为上层 TextSlicer 提供 `SliceInput` 流。
- [x] 使用 SIMD（通过 `memchr` 库）实现极速换行符扫描与二进制文件检测。
- [x] 支持逐行读取和按块读取，记录字节偏移和行号。
- [x] 处理 UTF-8 编码错误（自动替换为 ``），并识别换行符（LF/CRLF）。
- [x] 自动识别 BOM (Byte Order Mark)，支持 UTF-8, UTF-16LE, UTF-16BE 检测。

StreamReader 的输出是 `SliceInput` 流，每个 `SliceInput` 包含文本切片（可能借用或拥有）、偏移量、行号以及可选的元数据引用。

---

## 2. 数据结构
```rust
use std::borrow::Cow;

pub struct StreamReader {
    // 内部持有读取源（内存映射或字符串）的抽象
    // 以及已获取的元数据
}

pub struct LineIterator<'a> {
    // 逐行迭代器，每次返回一行文本及其位置信息
}

pub struct BlockIterator<'a> {
    // 按块迭代器，每次返回一个指定大小的文本块
}

pub struct FileMetadata {
    pub path: Option<PathBuf>,          // 文件路径（如果是文件）
    pub size: u64,                       // 文件大小（字节）
    pub file_type: FileType,              // 文本/二进制/未知
    pub created: Option<SystemTime>,      // 创建时间
    pub modified: Option<SystemTime>,     // 修改时间
    pub permissions: Option<Permissions>, // 权限
    pub owner: Option<String>,             // 属主（平台相关）
    pub bom: Option<Bom>,                  // BOM 类型
}

pub enum FileType {
    Text,
    Binary,
    Unknown,
}

pub enum Bom {
    Utf8,
    Utf16Le,
    Utf16Be,
    // 其他编码 BOM
}

pub enum StreamError {
    Io(std::io::Error),                    // 文件 I/O 错误
    Utf8(std::str::Utf8Error),              // UTF-8 解码错误
    BinaryDetected,                         // 检测到二进制文件
    InvalidPath,                             // 路径无效
    UnsupportedEncoding,
    TooLongLine,
}

/// 输入片段，由 StreamReader 产生，供 TextSlicer 消费
pub struct SliceInput<'a> {
    pub raw: Cow<'a, str>,          // 借用或拥有的文本（UTF-8 错误时拥有新 String）
    pub offset: usize,
    pub line_number: usize,
    pub file_metadata: Option<&'a FileMetadata>,
}
```

---

## 3. MVP 功能点清单

### 3.1 从文件路径创建 Reader（附带元数据）
- **功能描述**：打开指定路径的文件，使用内存映射（mmap）实现零拷贝读取。读取文件元数据并检测是否为二进制文件。如果文件是二进制，返回 `BinaryDetected`；如果文件不存在或权限不足，返回 `Io` 错误。支持大文件，仅映射不加载全部内存。
- **函数签名**：`pub fn from_file(path: &Path) -> Result<Self, StreamError>`
- **调用者**：CLI、VSCode 插件、其他需要读取文件的模块。
- **被调用者**：`std::fs::File::open`、`memmap2::Mmap::map`、元数据获取函数、二进制检测函数。
- **依赖**：`memmap2`、`std::fs`、`std::path`。
- **测试要点**：
  - 正常文本文件（含 BOM 或无 BOM）→ 返回 Reader，元数据正确。
  - 不存在的文件 → Io 错误。
  - 权限不足的文件 → Io 错误。
  - 二进制文件（如 .exe）→ BinaryDetected。
  - 空文件 → 返回 Reader（空内容）。
- **优先级**：MVP

### 3.2 从字符串创建 Reader
- **功能描述**：直接从内存中的字符串切片创建 Reader，用于处理编辑器内容或测试。不涉及文件元数据。
- **函数签名**：`pub fn from_str(text: &str) -> Self`
- **调用者**：测试代码、内部预览功能。
- **被调用者**：无（直接持有字符串引用）。
- **依赖**：无。
- **测试要点**：
  - 普通字符串。
  - 空字符串。
  - 包含非法 UTF-8 的字符串（将在迭代时处理）。
- **优先级**：MVP

### 3.3 获取文件元数据
- **功能描述**：返回之前打开文件的元数据（如果是从文件创建的 Reader）；否则返回 `None`。
- **函数签名**：`pub fn metadata(&self) -> Option<&FileMetadata>`
- **调用者**：TextSlicer（用于记录来源）、错误报告模块。
- **被调用者**：无。
- **依赖**：无。
- **测试要点**：
  - 从文件创建的 Reader → 返回 Some(metadata)。
  - 从字符串创建的 Reader → 返回 None。
- **优先级**：MVP

### 3.4 判断是否为文本文件
- **功能描述**：根据二进制检测结果或创建来源，返回当前读取源是否被视为文本文件。
- **函数签名**：`pub fn is_text(&self) -> bool`
- **调用者**：上层模块可据此决定是否继续处理。
- **被调用者**：内部状态。
- **依赖**：无。
- **测试要点**：
  - 文本文件 → true。
  - 二进制文件 → false。
  - 字符串创建的 Reader → true（假设字符串总是文本）。
- **优先级**：MVP

### 3.5 逐行迭代器
- **功能描述**：返回一个迭代器，按行遍历文本。行以 LF (`\n`) 或 CRLF (`\r\n`) 分割，自动识别。每行返回文本切片、字节偏移量和行号。
  - 如果行是合法 UTF-8，返回 `Cow::Borrowed` 直接借用内存映射数据。
  - 如果行包含非法 UTF-8 序列，将其替换为 `�`，并返回 `Cow::Owned`（新分配的字符串）。
- **函数签名**：`pub fn iter_lines(&self) -> LineIterator`
- **调用者**：TextSlicer 的行切片策略。
- **被调用者**：内部扫描函数（使用 `memchr` 或手动查找换行符）。
- **依赖**：`memchr`（可选，用于性能）。
- **测试要点**：
  - 空文件 → 迭代器无元素。
  - 混合换行符（LF 和 CRLF）→ 正确分割。
  - 最后一行无换行符 → 仍作为一行返回。
  - 超长行（>10KB）→ 正常返回，不截断。
  - 包含非法 UTF-8 的字节 → 在迭代时自动替换为 `�`（需在内部处理）。
- **优先级**：MVP

### 3.6 按块迭代器
- **功能描述**：返回一个迭代器，按指定大小（字节）将文本分割成块。每块返回文本切片和偏移量。若块边界落在多字节字符中间，则调整到合法 UTF-8 边界，避免乱码。适用于无换行符的大块文本（如 Base64 编码）或流式传输。
- **函数签名**：`pub fn iter_blocks(&self, block_size: usize) -> Result<BlockIterator, StreamError>`
- **调用者**：TextSlicer 的按块切片策略。
- **被调用者**：内部按字节分割逻辑。
- **依赖**：无。
- **测试要点**：
  - 块大小小于文件长度 → 生成多个块。
  - 块大小大于文件长度 → 生成一个块。
  - 块大小为 0 → 返回错误。
  - 空文件 → 迭代器无元素。
  - 多字节字符边界 → 正确处理。
- **优先级**：MVP

### 3.7 处理 UTF-8 错误（内部）
- **功能描述**：在迭代过程中，如果遇到无效的 UTF-8 序列，将其替换为 Unicode 替换字符 `�`（U+FFFD），此功能内置于迭代器中，使用 `String::from_utf8_lossy` 或手动处理。并继续处理。此功能内置于迭代器中，不对外暴露单独函数。
- **函数签名**：无（内部实现）。
- **调用者**：迭代器内部。
- **被调用者**：`std::str::from_utf8` 或自定义解码器。
- **依赖**：无。
- **测试要点**：
  - 纯 ASCII 文本 → 无替换。
  - 包含非法 UTF-8 字节 → 替换为 `�`。
- **优先级**：MVP

### 3.8 二进制文件检测（内部）
- **功能描述**：在打开文件后，读取前 4KB 数据，统计 NULL 字节（`\0`）的比例。如果比例超过阈值（例如 10%），则判定为二进制文件。此函数在 `from_file` 内部调用。
- **函数签名**：`fn detect_binary(file: &File) -> bool`（私有）
- **调用者**：`from_file`。
- **被调用者**：`std::fs::File::read`。
- **依赖**：无。
- **测试要点**：
  - 纯文本文件（无 NULL）→ false。
  - 包含少量 NULL 的文本文件（如某些日志）→ 低于阈值，返回 false。
  - 二进制文件（如 ELF、PE）→ 高于阈值，返回 true。
- **优先级**：MVP

---

## 4. 未来功能点清单（待定）

### 4.1 从管道/网络流读取
- **功能描述**：支持从实现了 `Read` trait 的任意输入流（如管道、TCP 连接）创建 Reader，元数据可能缺失。
- **函数签名**：`pub fn from_reader<R: Read>(reader: R) -> Self`
- **优先级**：未来

### 4.2 自动 BOM 处理
- **功能描述**：检测文件开头的 BOM（字节顺序标记），并相应处理编码（如 UTF-16、UTF-32）。在迭代时自动转换或忽略 BOM。
- **函数签名**：扩展 `FileMetadata` 增加 `bom` 字段；`from_file` 内部检测并记录。
- **优先级**：未来

### 4.3 超长行自动分块
- **功能描述**：当某一行超过预设长度（如 1MB）时，自动将其拆分为多个块，避免内存压力。可作为迭代器的配置选项。
- **函数签名**：在 `iter_lines` 中增加配置参数。
- **优先级**：未来

### 4.4 从字节流创建 Reader
- **功能描述**：直接从一个字节切片创建 Reader，用于底层数据处理。
- **函数签名**：`pub fn from_bytes(bytes: &[u8]) -> Result<Self, StreamError>`
- **优先级**：未来

### 4.5 自定义换行符
- **功能描述**：允许用户指定除 LF/CRLF 之外的行分隔符（如 CR 或自定义字符）。
- **函数签名**：`pub fn with_line_delimiter(self, delimiter: u8) -> Self`
- **优先级**：未来

---

## 5. 与其它模块的交互
- **输出数据**：StreamReader 通过迭代器输出 `SliceInput`，其结构包含：
  ```rust
  struct SliceInput<'a> {
      raw: Cow<'a, str>,
      offset: usize,
      line_number: usize,
      file_metadata: Option<&'a FileMetadata>,
  }
  ```
- **下游模块**：TextSlicer 消费 `SliceInput` 流，根据切片策略生成 `Slice`。TextSlicer 可能会利用 `file_metadata` 中的路径信息进行字典化。
- **错误传播**：StreamReader 产生的错误（如 `BinaryDetected`）会直接返回给调用者，上层可据此提示用户。

---

## 6. 待办与注意事项
- **二进制检测阈值**：当前定为 10% 是否合理？可能需要支持配置或根据不同文件类型调整。
- **UTF-8 替换策略**：采用标准替换字符 `�`，使用 `Cow` 在非法时分配新字符串，未来可考虑保留原始字节作为自定义 token 以支持可逆性。
- **性能优化**：逐行扫描可使用 SIMD（如 `memchr`）加速，但 MVP 阶段可先采用简单方式。
- **mmap 跨平台**：`memmap2` 已处理跨平台差异。
- **BOM 处理**：BOM 可能出现在 UTF-8 文件中，MVP 阶段可先忽略（当作普通字符），未来再处理。
- **错误类型扩展**：未来可能增加 `UnsupportedEncoding`、`TooLongLine` 等错误。

