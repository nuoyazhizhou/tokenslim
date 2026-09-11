<!--
本文件由 AI 工具自动迁移自 .qoder/repowiki/, 用于补充 docs/ 的视角。
- 生成器: Qoder (阿里云 AI IDE)
- 生成日期: 2026-06-23
- 原文件: .qoder/repowiki/knowledge/en/Error Handling and Isolation Architecture/error_handling.md
- 适用版本: TokenSlim v0.3.5 (扫描基线: C:\git_work\TokenSlim-publish2 当时快照)
- 维护说明: 内容已与项目 docs/ 现有文件去重; 若代码变更需人工校对。
-->

The TokenSlim platform employs a robust, multi-layered error handling strategy centered on **Rust's type-safe `Result` paradigm**, **structured error codes**, and **runtime isolation** for its dynamic plugin ecosystem. 

### 1. Core Error System: `thiserror` and Structured Codes
The codebase uses the `thiserror` crate to define granular, domain-specific error enums across all core modules. A key convention is the use of **standardized error codes** (e.g., `E_PIPELINE_STREAM`, `E_EXECUTION_PLUGIN_PANIC`) embedded directly into the `#[error(...)]` attribute. This ensures consistent error identification for logging, metrics, and API responses.

*   **Error Propagation:** Errors are propagated using the `?` operator, with automatic conversion between module-level errors (e.g., `StreamError` -> `PipelineError`) via `#[from]` attributes.
*   **Key Error Types:**
    *   `PipelineError`: Aggregates errors from slicing, analysis, dispatching, and dictionary engines.
    *   `ExecutionError`: Specific to the `error_isolation` module, capturing panics and timeouts.
    *   `CliError` & `ApiError`: Top-level errors for CLI and HTTP interfaces, respectively.

### 2. Plugin Safety: The `ErrorIsolation` Module
To prevent a single misbehaving plugin from crashing the entire compression pipeline, TokenSlim implements a `SafeExecutor` in `src/core/error_isolation/`. 
*   **Panic Catching:** Uses `std::panic::catch_unwind` to intercept panics within plugin `detect` or `compress` calls.
*   **Timeout Enforcement:** Executes plugins in scoped threads with `recv_timeout` to enforce execution limits (default 1000ms). If a plugin exceeds this, it returns an `ExecutionError::Timeout`.
*   **Fallback Strategy:** The `PluginDispatcher` uses these isolated results to trigger fallback plugins (e.g., generic text) if a specialized plugin fails or times out.

### 3. Observability and Metrics
Errors are not just returned; they are tracked via the `MetricsCollector` (`src/core/metrics/`).
*   **Error Logging:** The `ErrorLog` struct captures timestamps, module names, and error types, exposed via the `/metrics/detail` API endpoint.
*   **Plugin Stats:** Tracks `panic_count`, `timeout_count`, and `fallback_count` per plugin to identify unstable components.
*   **Structured Monitoring:** The `observability` module provides `ScopeProbe` for RAII-based performance and memory monitoring, emitting structured `[TS_MON]` logs for long-running operations.

### 4. API and CLI Presentation
*   **HTTP API:** The `tokenslim-server` uses a custom `ApiError` type that maps internal errors to HTTP status codes (401, 500, 503) and returns localized JSON bodies (`message_zh`, `message_en`) with hints.
*   **CLI:** The CLI layer wraps pipeline errors into `CliError`, providing user-friendly messages for I/O, serialization, or compression failures.

### Developer Rules
1.  **Use `thiserror`:** Always define new error types using `thiserror::Error` with explicit `E_MODULE_REASON` codes.
2.  **Isolate Plugins:** Any code executing user-defined or dynamic logic must be wrapped by `SafeExecutor::execute`.
3.  **Avoid `unwrap()`:** In library and server code, prefer `map_err` to convert standard errors (like `std::io::Error`) into domain-specific types. `unwrap()` is reserved for examples and tests.
4.  **Track Failures:** Ensure significant errors are recorded via the `MetricsCollector` to support post-mortem analysis via the Web UI.
---

<!--
来源: knowledge/en/Error Handling and Isolation Architecture/error_handling.md  |  生成器: Qoder (阿里云 AI IDE)  |  扫描基线: publish2 @ 2026-06-23
-->
