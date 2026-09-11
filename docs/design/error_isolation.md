是的，目前还剩下这四个模块。其中 **ErrorIsolation** 和 **Metrics** 属于支持性模块，MVP 阶段可以简化处理；**CLI** 是 MVP 必需的，用于调用核心引擎；**VSCode 插件** 是 P1，可以后续再做。

接下来我们按顺序设计这些模块。先设计 **ErrorIsolation** 模块，因为它为插件执行提供安全保障，已经在 PluginDispatcher 中用到，需要明确定义。

---

# ErrorIsolation 模块功能点

## 1. 模块概述
ErrorIsolation 是微内核的辅助模块，负责**安全执行插件代码**，捕获可能发生的 panic 和超时，防止单个插件崩溃导致整个流水线失败。它提供统一的执行包装器，供 PluginDispatcher 或其他需要调用外部代码的模块使用。

该模块不关心具体业务逻辑，只专注于异常处理、超时控制和错误转换。

---

## 2. 数据结构
```rust
use std::time::Duration;

/// 安全执行器配置
#[derive(Clone)]
pub struct SafeExecutorConfig {
    pub default_timeout: Duration,      // 默认超时时间
    pub catch_panic: bool,               // 是否捕获 panic（默认 true）
}

/// 安全执行器主结构
pub struct SafeExecutor {
    config: SafeExecutorConfig,
}

/// 执行错误类型
#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    #[error("Plugin panicked")]
    Panic,
    #[error("Execution timed out after {0:?}")]
    Timeout(Duration),
    #[error("Invalid result")]
    InvalidResult,
    #[error("Other error: {0}")]
    Other(String),
}
```

---

## 3. MVP 功能点清单

### 3.1 初始化执行器
- **功能描述**：创建 SafeExecutor 实例，接受配置。
- **函数签名**：`pub fn new(config: SafeExecutorConfig) -> Self`
- **调用者**：PluginDispatcher 或其他模块。
- **被调用者**：无。
- **依赖**：无。
- **测试要点**：配置正确保存。
- **优先级**：MVP

### 3.2 执行闭包并捕获 panic
- **功能描述**：安全地执行一个闭包，使用 `std::panic::catch_unwind` 捕获 panic。若发生 panic，返回 `ExecutionError::Panic`。
- **函数签名**：`pub fn catch_panic<F, R>(&self, f: F) -> Result<R, ExecutionError> where F: FnOnce() -> R + panic::UnwindSafe`
- **调用者**：内部被其他方法调用。
- **被调用者**：`std::panic::catch_unwind`。
- **依赖**：`std::panic`。
- **测试要点**：
  - 正常执行返回结果。
  - panic 时返回 Panic 错误。
- **优先级**：MVP

### 3.3 执行闭包并设置超时
- **功能描述**：在单独的线程中执行闭包，并设置超时。若超时，线程被丢弃（或通过 channel 通知），返回 `ExecutionError::Timeout`。
- **函数签名**：`pub fn with_timeout<F, R>(&self, f: F, timeout: Duration) -> Result<R, ExecutionError> where F: FnOnce() -> R + Send + 'static, R: Send + 'static`
- **调用者**：PluginDispatcher 的 `execute_plugin`。
- **被调用者**：`std::thread::spawn` 和 `std::sync::mpsc::channel`。
- **依赖**：`std::thread`。
- **测试要点**：
  - 正常执行返回结果。
  - 超时返回 Timeout 错误。
  - 线程被正确终止（Rust 中不能强制终止，但可丢弃 JoinHandle，线程会在任务完成后结束）。
- **优先级**：MVP

### 3.4 组合捕获 panic 和超时
- **功能描述**：同时捕获 panic 和超时，先启动带超时的线程，并在线程内部捕获 panic。
- **函数签名**：`pub fn execute<F, R>(&self, f: F, timeout: Option<Duration>) -> Result<R, ExecutionError> where F: FnOnce() -> R + Send + panic::UnwindSafe + 'static, R: Send + 'static`
- **调用者**：PluginDispatcher 等。
- **被调用者**：内部组合 `with_timeout` 和 `catch_panic`。
- **测试要点**：
  - 正常执行返回结果。
  - panic 被捕获。
  - 超时被捕获。
- **优先级**：MVP

### 3.5 验证结果有效性（可选）
- **功能描述**：对返回的结果进行基本校验，例如检查是否为空、是否符合预期格式。由调用者提供校验闭包。
- **函数签名**：`pub fn validate<F, R, V>(&self, result: R, validator: V) -> Result<R, ExecutionError> where V: FnOnce(&R) -> bool`
- **调用者**：调用者可在执行后调用此方法进一步校验。
- **测试要点**：校验通过返回结果，否则返回 InvalidResult。
- **优先级**：未来

---

## 4. 未来功能点清单（待定）
- **异步运行时支持**：集成 tokio 等异步运行时，实现更高效的超时控制。
- **资源限制**：限制插件内存使用、CPU 时间等。
- **结果缓存**：对相同输入缓存执行结果，避免重复执行（需谨慎，因插件可能有副作用）。

---

## 5. 与其它模块的交互
- **调用者**：PluginDispatcher 使用 `execute` 方法安全调用插件的 `compress` 和 `detect`。
- **输出**：返回 `Result<R, ExecutionError>`，调用者根据错误决定是否 fallback。

---

## 6. 待办与注意事项
- **超时实现**：Rust 标准库不支持强制终止线程，`with_timeout` 只能通过 channel 在主线程等待，超时后丢弃 `JoinHandle`，线程仍会继续运行直到完成。这可能导致资源浪费，但通常可接受，因为超时时间较短。更优雅的方式是使用异步运行时，但会增加复杂度。MVP 阶段可采用此简单方案。
- **panic 捕获**：`catch_unwind` 不能捕获某些严重错误（如栈溢出），但通常够用。
- **线程安全**：传递的闭包必须满足 `Send + 'static`，因为需要跨线程移动。
- **错误日志**：发生 panic 或超时应记录日志，便于调试。
