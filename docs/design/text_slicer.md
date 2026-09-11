# TextSlicer 模块功能点

## 1. 模块概述
TextSlicer 是微内核的第二层模块，位于 StreamReader 之后、ContentAnalyzer 之前。它的职责是**将流式输入的文本片段（`SliceInput`）按照预定义的策略切分成独立的、有意义的单元 `Slice`**，每个 `Slice` 代表一个可独立识别和处理的内容块（如一行日志、一个段落、一个代码块、一个 HTML 标签块等）。

TextSlicer 的输入是来自 StreamReader 的 `SliceInput` 流，输出是 `Slice` 流。它负责维护切片过程中的状态（例如跨行累积），确保每个输出的 `Slice` 包含完整的文本内容、位置信息（起始/结束行号、偏移量）以及初步的类型标记（可选）。切片过程必须保证可逆性，即能从 `Slice` 流恢复出原始文本顺序。

---

## 2. 数据结构
```rust
use std::borrow::Cow;

/// 切片唯一标识，全局递增
pub type SliceId = u64;

/// 切片初步类型（由切片器推断，并非最终识别结果）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceType {
    Line,          // 单行
    Paragraph,     // 空行分隔的段落
    CodeBlock,     // 缩进或标记的代码块
    HtmlBlock,     // HTML/XML 标签块
    JsonBlock,     // JSON/YAML 块
    StackTrace,    // 堆栈跟踪片段
    LogBlock,      // 日志片段
    Binary,        // 二进制内容（来自 StreamReader）
    Unknown,
}

/// 切片输出结构，text 使用 Cow 以支持借用或拥有数据
#[derive(Debug)]
pub struct Slice<'a> {
    pub id: SliceId,
    pub text: Cow<'a, str>,          // 借用或拥有的文本
    pub slice_type: SliceType,
    pub offset: usize,                // 在整个文件中的字节偏移（通常为第一行的偏移）
    pub line_start: usize,            // 起始行号（从1开始）
    pub line_end: usize,              // 结束行号
    pub file_metadata: Option<&'a FileMetadata>, // 文件元数据（可选）
}

/// 切片器配置
#[derive(Clone)]
pub struct SlicerConfig {
    pub enable_line: bool,
    pub enable_paragraph: bool,
    pub enable_indent: bool,
    pub enable_tags: bool,
    pub enable_regex: bool,
    pub enable_hybrid: bool,
    // 可选的阈值参数等
}

/// 切片器本身（可持有状态）
pub struct TextSlicer {
    config: SlicerConfig,
    next_id: SliceId,
    // 根据不同策略可能维护的内部状态
    paragraph_buffer: Vec<String>,  // 示例：段落累积器
    // 其他策略的内部状态...
}
```

---

## 3. 已实现的核心功能

### 3.1 混合切片策略 (Hybrid Slicing)
- [x] **按行切片**: 100% 零拷贝，直接引用原始内存。
- [x] **按段落切片**: 智能识别空行，自动聚合相关行。
- [x] **按标签切片**: 识别 HTML/XML 及 MyBatis 风格的标签块。
- [x] **按缩进切片**: 自动识别 Python/YAML 风格的逻辑缩进块。

### 3.2 硬件级加速探测 (SIMD Acceleration)
- [x] **Aho-Corasick 扫描**: 集成 `aho-corasick` 库，利用 SIMD 指令集一次性完成对 HTML 实体、XML 标记和特定触发词的扫描。
- [x] **延迟初始化正则**: 所有的分片正则（Macro, StackTrace, LogHeader）均采用 `Lazy` 静态初始化，确保匹配效率。
- [x] **快速路径优化**: 在进入复杂匹配前，预先进行基于 AC 算法的快速筛选，无命中则直接跳过，解析性能大幅提升。

### 3.3 按缩进切片（Indent Slicing）
- **功能描述**：根据行首缩进的变化来切分代码块。适用于 Python、YAML 等。维护当前缩进级别，当缩进增加或减少时，认为开始或结束一个代码块。连续相同缩进的行属于同一块。输出每个代码块作为一个 Slice，文本为块内所有行拼接（保留换行），记录起始/结束行号和偏移。
- **函数签名**：`pub fn slice_by_indent(&mut self, input: &SliceInput) -> Option<Slice>`
- **调用者**：主循环。
- **被调用者**：需要缩进检测函数（计算行首空格数）。
- **依赖**：无。
- **测试要点**：
  - 连续相同缩进的多行 → 累积，直到缩进变化时输出前一块。
  - 缩进增加 → 开始新块，旧块结束。
  - 缩进减少 → 结束当前块，可能也结束上层块（取决于设计）。
- **优先级**：未来（MVP 可暂缓）

### 3.4 按标签切片（Tag Slicing）
- **功能描述**：识别 HTML/XML 标签，将完整的标签块（包括嵌套）作为一个 Slice。例如 `<div>...</div>` 整个作为一个 Slice。此策略需要解析标签结构，维护一个标签栈。
- **函数签名**：`pub fn slice_by_tags(&mut self, input: &SliceInput) -> Option<Slice>`
- **调用者**：主循环。
- **被调用者**：轻量 HTML 解析器。
- **依赖**：可能需要引入 `html5gum` 或类似 crate。
- **测试要点**：
  - 简单标签 `<p>text</p>` → 整个作为一个 Slice。
  - 嵌套标签 `<div><span>...</span></div>` → 整个作为一个 Slice。
  - 自闭合标签 `<img />` → 作为一个 Slice。
- **优先级**：未来

### 3.5 按正则切片（Regex Slicing）
- **功能描述**：使用用户定义的正则表达式来切分文本。例如，对于 Java 堆栈，可以用正则 `^at [\\w.]+\\(` 匹配堆栈帧开始行，将连续匹配的行合并为一个 Slice。每个策略可配置多个正则，优先级决定如何分组。
- **函数签名**：`pub fn slice_by_regex(&mut self, input: &SliceInput) -> Option<Slice>`
- **调用者**：主循环。
- **被调用者**：正则引擎。
- **依赖**：`regex` crate。
- **测试要点**：
  - 连续多行匹配正则 → 累积为一个 Slice。
  - 不匹配的行 → 单独作为一个 Slice。
- **优先级**：未来

### 3.6 混合切片（Hybrid Slicing）
- **功能描述**：组合多种策略，按优先级依次尝试。例如，先尝试正则匹配堆栈，若命中则按堆栈规则分组；否则尝试标签，否则按缩进，否则按空行，最后默认按行。此策略是其他策略的调度器。
- **函数签名**：`pub fn slice_hybrid(&mut self, input: &SliceInput) -> Option<Slice>`
- **调用者**：主循环。
- **被调用者**：各具体策略的 `slice_*` 方法。
- **依赖**：所有已启用的策略。
- **测试要点**：验证优先级和回退逻辑。
- **优先级**：未来（MVP 可暂缓，先用按行切片）

### 3.7 切片器初始化与配置
- **功能描述**：创建 `TextSlicer` 实例，接受 `SlicerConfig`，初始化内部状态（如 ID 计数器、缓冲区）。
- **函数签名**：`pub fn new(config: SlicerConfig) -> Self`
- **调用者**：上层模块（如 CompressionPipeline）。
- **被调用者**：无。
- **依赖**：无。
- **测试要点**：配置正确应用。
- **优先级**：MVP

### 3.8 刷新剩余缓冲区
- **功能描述**：当输入流结束时，调用此方法强制输出所有尚未完成的 Slice（例如最后一个段落）。返回的 `Vec<Slice>` 中可能包含 `Owned` 文本。
- **函数签名**：`pub fn flush(&mut self) -> Vec<Slice>`
- **调用者**：主循环结束后。
- **被调用者**：各策略的内部状态清理。
- **依赖**：无。
- **测试要点**：确保所有累积数据被输出，文本正确。
- **优先级**：MVP（如果实现了有状态策略）

---

## 4. 未来功能点清单（待定）

### 4.1 上下文感知切片
- **功能描述**：在切片时不仅考虑当前行，还参考前后行内容（例如识别多行错误消息）。可能需要滑动窗口。
- **优先级**：未来

### 4.2 基于 embedding 的切片
- **功能描述**：利用 embedding 相似度判断内容边界，用于自然语言段落分割。
- **优先级**：未来

### 4.3 自定义切片规则插件
- **功能描述**：允许用户通过配置文件或插件注册新的切片策略。
- **优先级**：未来

---

## 5. 与其它模块的交互
- **输入**：从 StreamReader 接收 `SliceInput` 流。StreamReader 的 `iter_lines()` 或 `iter_blocks()` 产生 `SliceInput`，TextSlicer 通过调用其方法逐个处理这些输入。
- **输出**：产生 `Slice` 流，每个 `Slice` 包含文本、位置、初步类型和元数据引用。这些 `Slice` 将传递给 ContentAnalyzer 进行进一步识别。
- **元数据传递**：`Slice` 中的 `file_metadata` 直接引用自输入，可供下游使用。

---

## 6. 待办与注意事项
- **MVP 切片策略的选择**：建议 MVP 只实现 **按行切片**（最简单）和 **按空行切片**（有状态但常见），其他策略可后续添加。按行切片已足够与 ContentAnalyzer 配合，因为后续内容分析可以识别类型。
- **ID 生成**：`SliceId` 需要在全局唯一，可以使用 `AtomicU64` 或简单递增。注意线程安全（如果并行处理切片）。
- **零拷贝与 `Cow` 的使用**：`Slice` 中的 `text` 使用 `Cow<'a, str>` 以兼顾借用和拥有。简单行切片用 `Borrowed` 直接引用输入数据（要求输入数据生命周期足够长，例如整个文件内容由 StreamReader 持有）；需要合并的场景（如段落切片）用 `Owned` 分配新字符串。`Cow` 会自动解引用为 `&str`，后续使用方便。
- **生命周期**：`Slice` 的生命周期 `'a` 源自输入数据。对于 `Owned` 文本，生命周期实际上不受输入限制，但为了统一接口，仍保留泛型参数。实际使用中，`'a` 在 `Owned` 场景下可以是 `'static` 或与拥有者绑定。
- **错误处理**：切片过程中不应失败（除非配置错误），所有输入都应被处理。可能的错误（如正则编译失败）应在初始化时检查。
- **性能**：按行切片极快（零拷贝）；按空行切片需累积字符串，涉及内存分配，但通常可接受。`Cow` 仅在需要时分配，几乎无额外开销。
- **内存管理**：`flush` 后，内部缓冲区应清空，避免重复输出。确保所有累积数据被正确释放。
