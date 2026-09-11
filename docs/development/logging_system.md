<!--
本文件由 AI 工具自动迁移自 .qoder/repowiki/, 用于补充 docs/ 的视角。
- 生成器: Qoder (阿里云 AI IDE)
- 生成日期: 2026-06-23
- 原文件: .qoder/repowiki/knowledge/zh/TokenSlim 日志与可观测性系统/logging_system.md
- 适用版本: TokenSlim v0.3.5 (扫描基线: C:\git_work\TokenSlim-publish2 当时快照)
- 维护说明: 内容已与项目 docs/ 现有文件去重; 若代码变更需人工校对。
-->

## 1. 系统概述
TokenSlim 采用 **混合日志架构**，结合了 Rust 生态中两种主流的日志方案：
- **`env_logger` + `log` crate**：用于传统的、基于文本的运行时日志输出（如服务器启动、配置加载、错误提示）。
- **`tracing` + `tracing-subscriber`**：用于高性能的结构化追踪和诊断，特别是在核心压缩流水线和插件系统中，通过 `#[tracing::instrument]` 宏实现细粒度的函数级监控。

这种设计兼顾了运维人员的可读性需求（传统日志）和开发者的深度调试需求（结构化追踪）。

## 2. 核心组件与文件
- **初始化逻辑**：
  - `src/main.rs`：CLI 入口，同时初始化 `env_logger` 和 `tracing`。支持通过 `-v/--verbose` 参数动态提升日志级别至 `debug`。
  - `src/core/tracing_init.rs`：封装了 `tracing` 订阅者的配置逻辑，支持通过 `TOKENSLIM_LOG` 或 `RUST_LOG` 环境变量控制过滤规则。
  - `src/bin/tokenslim-server.rs`：Server 入口，仅使用 `env_logger` 进行基础生命周期日志记录。
- **依赖配置**：
  - `Cargo.toml`：引入了 `log = "0.4"`, `env_logger = "0.11.9"`, `tracing = "0.1"`, `tracing-subscriber = "0.3"`。
- **指标收集**：
  - `src/core/metrics/`：实现了内存级的性能指标收集器（`MetricsCollector`），记录模块耗时、插件调用次数、错误日志等，并通过 `/metrics` 端点暴露 Prometheus 格式数据。

## 3. 架构约定与设计决策
### 3.1 日志级别策略
- **Info**：默认级别。记录服务器启动、API 请求概览、配置重载等关键状态变更。
- **Debug**：通过 `-v` 开启。记录详细的处理流程、插件分发决策、字典构建过程。
- **Warn/Error**：记录认证失败、文件监听异常、压缩/还原失败等需要人工干预的问题。

### 3.2 结构化追踪 (Tracing)
在核心引擎（`src/core/`）和插件（`src/plugins/`）中，广泛使用了 `#[tracing::instrument]` 宏：
- **Level 映射**：常规操作使用 `level = "debug"`，高频或底层操作（如 Trie 遍历、正则匹配）使用 `level = "trace"`。
- **字段捕获**：自动捕获函数参数和返回值，便于在分布式或并发环境下追踪特定文本切片的处理路径。

### 3.3 国际化集成
日志消息通过 `tokenslim::utils::i18n` 模块进行本地化处理（如 `t("server_starting")`），确保在不同语言环境下，运维日志依然具备可读性。

## 4. 开发者指南
### 4.1 如何添加日志
- **简单文本日志**：在 Server 或 CLI 逻辑中使用 `log::info!("...")` 或 `log::error!("...")`。
- **深度诊断日志**：在核心算法或插件方法上添加 `#[tracing::instrument(level = "debug", skip_all)]`。
  - 注意：对于包含大量文本数据的参数，务必使用 `skip_all` 或在 `fields` 中显式指定，避免日志爆炸。

### 4.2 环境变量控制
- `RUST_LOG` / `TOKENSLIM_LOG`：支持标准的 `env_filter` 语法，例如 `tokenslim=debug,tower_http=info`。
- `TOKENSLIM_LOG_STYLE`：控制日志颜色输出（`always`, `auto`, `never`）。

### 4.3 性能考量
- `tracing` 在 `release_max_level_info` 特性开启时，`debug/trace` 级别的代码会被编译器优化掉，因此生产环境建议保持该特性以零成本运行。
- 避免在高频循环中执行复杂的字符串格式化日志操作，优先使用 `tracing` 的懒求值特性。
---

<!--
来源: knowledge/zh/TokenSlim 日志与可观测性系统/logging_system.md  |  生成器: Qoder (阿里云 AI IDE)  |  扫描基线: publish2 @ 2026-06-23
-->
