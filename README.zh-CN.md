<p align="center">
  <h1 align="center">TokenSlim</h1>
  <p align="center">
    高性能 Rust 插件化 AI 文本输入压缩引擎。<br>
    插件化架构 · 节省 50%–95% Token · AI 诊断导出 · CLI / Server / IDE / SDK
  </p>
</p>

<p align="center">
  <a href="#-什么是-tokenslim">什么是 TokenSlim</a> ·
  <a href="#-为什么选择-tokenslim">为什么</a> ·
  <a href="#-核心特性">核心特性</a> ·
  <a href="#-安装">安装</a> ·
  <a href="#-使用方式">使用方式</a> ·
  <a href="#-插件">插件</a> ·
  <a href="#-集成">集成</a> ·
  <a href="#-许可证">许可证</a>
</p>

<p align="center">
  <a href="./README.md">English</a> · <strong>简体中文</strong> · <a href="./README.ja.md">日本語</a> · <a href="./README.ko.md">한국어</a> · <a href="./README.es.md">Español</a> · <a href="./README.fr.md">Français</a> · <a href="./README.de.md">Deutsch</a> · <a href="./README.ar.md">العربية</a>
</p>

---

## ⚡ 什么是 TokenSlim？

TokenSlim 是一款用 Rust 编写的高性能、插件化文本压缩引擎。核心使命是**大幅降低 LLM 输入的 Token 成本**，让冗长、嘈杂的真实日志（构建流水线、CI 运行、Web 访问日志、数据库 trace、云日志、VCS 输出、堆栈跟踪等）能够装进 LLM 上下文窗口，又不丢失模型需要的诊断信号。

对高度结构化、高度重复的输入（编译器日志、构建输出、CI 日志、访问日志等），TokenSlim 通常能达成 **50%–90%** 的体积缩减，同时保留 100% 的原始信息。在专门面向 LLM 消费的 **AI Export 模式** 下，缩减率可达 **90%–95%**，并通过上下文感知降噪保留模型真正需要的 error/warning 上下文。

除压缩外，TokenSlim 还提供环境诊断工具（`workspace`、`encoding`、`rule`、`env` 子命令），自动检测操作系统、Shell、代码页、Python/Node/JDK 编码配置，标记 mojibake 风险并输出可执行的修复建议。结合子进程解码回退链（UTF-8 优先，codepage 候选），在多语言环境下保持稳定可靠。

## 🚀 为什么选择 TokenSlim？

### 1. 真正省钱

LLM API 成本主要被输入 Token 数主导。TokenSlim 直接砍掉 50%–95%：

- **更低的 API 账单** — 输入 Token 减少 50%–95%。
- **上下文感知的 AI Export（`--ai-export`）** — 剥离流水行，保留 error/warning 窗口，模型最需要的部分。
- **更长的有效上下文** — 同样的窗口装下更多真实信号。
- **更快的 prefill** — 更短的输入通常意味着更快的模型 prefill 与更低的 TTFT。

### 2. 工业级性能

- **零拷贝流水线** — 基于 Rust `Cow<'a, str>`、`rayon` 并行块处理、`Bump` 内存池。100 MB 工业级日志约 **250 ms** 处理完，吞吐量约 400 MB/s。
- **确定性全局重排** — 流式构建目标追踪器修齐 `make -jN` / `Ninja` 产生的乱序交错。两次相同的并发构建产出同样的错误栈顺序。
- **Sidecar 模式** — 高吞吐 REST API Server，可嵌入 IDE / CI / Agent 流程，零启动开销。

### 3. 数据驱动提取

- **Radix-trie 路径提取** — TokenSlim 不按行切。扫描完 100 MB 输入后，在内存里构建一棵工程级 radix-trie，只在热分支（权重 > 10）发射目录字典（`$D`），彻底消除碎片。
- **语义标记** — 面向 Android、iOS、GCC、MSVC、链接器的环境感知替换。
- **全构建生态检测** — C/C++、Rust、Go、Java、Android、iOS/Xcode、MSVC、Swift 与主流链接器，上下文感知折叠与错误去重。

## ✨ 核心特性

- **三种运行时**
  - **CLI** — 可脚本化的批处理
  - **Server** — 常驻 REST API
  - **SDK** — Java、Python（PyO3）、Node.js
- **插件生态**（60+ 插件，覆盖最常见的 LLM 输入源）
  - **移动端** — `android_gradle`, `xcode_log`
  - **通用开发** — `gcc_log`, `java_stack`, `python_traceback`, `dotnet`, `rust_go`, `maven`, `gradle`, `node_error`, `nodejs`, `php_ruby`, `unity_unreal`
  - **结构化数据** — `json`, `yaml`, `xml_html`, `ndjson`, `protobuf`
  - **构建产物** — `artifact_summary`（SARIF / JUnit XML），保留测试状态、SARIF level/rule/location/tool
  - **云与运维** — `cloud_log`（AWS / GCP / Azure / 阿里云 / OCI / 腾讯云 / 华为云 / Cloudflare）、`web_log`（Nginx / Apache / ingress / Envoy / CloudFront / IIS / ALB / Cloudflare）、`db_log`（PostgreSQL / MySQL / MongoDB / Redis）、`syslog`
  - **CI/CD** — `ci_log`（GitHub Actions / GitLab CI / Jenkins / Azure Pipelines / CircleCI / Buildkite / 本地 `act` / TeamCity / Travis CI）
  - **VCS** — 统一 `vcs_plugin` 覆盖 git / svn / hg / p4 / cvs / bzr / fossil / darcs，外加 `git_diff`、`smart_code`（AST 级别）、`smart_path`
- **环境诊断** — `workspace`、`encoding`、`rule`、`env` 子命令检测 mojibake 风险并输出修复建议。
- **AI 原生输出模式**
  - `--ai-export` — 上下文感知降噪，保留 error/warning 窗口
  - `--ai-signal` — 有损但高信号，保留最决策相关的字段
- **插件内省** — `tokenslim explain-plugin` 和 `tokenslim run --explain-route` 解释路由选择、回退、置信度、备选，并可回放误分类供审计。

## 🛠️ 安装

### 从源码（Rust toolchain ≥ 1.75）

```bash
git clone https://github.com/nuoyazhizhou/tokenslim.git
cd tokenslim
cargo build --release
```

可执行文件位于 `./target/release/tokenslim`（Windows 上是 `tokenslim.exe`）。

### 预编译二进制

从 [Releases](https://github.com/nuoyazhizhou/tokenslim/releases) 页面下载。

### 配置（可选）

所有运行时配置都走环境变量。复制 [`.env.example`](./.env.example) 为 `.env` 并填入本地值。`.env` 默认被 git 忽略，只有 example 模板会被跟踪。

大多数用户只需要设 `RUST_LOG=info`（debug 看到更详细的 tracing）。LLM 审计相关变量（`OPENAI_API_KEY`、`OPENAI_BASE_URL`、`OPENAI_MODEL`）仅在跑 `scripts/audit_*.py --llm-audit` 时才需要；不填的话审计会降级为 lint-only 模式。

### 编辑器 / IDE 集成

- **VS Code** — 见 `vscode-extension/`
- **Chrome** — 见 `chrome-extension/`
- **JetBrains** — 见 `jetbrains-plugin/`

### SDK

- **Python** — `pip install tokenslim`（来自 `crates/tokenslim-py`）
- **Node.js** — `npm i tokenslim`（见 `sdk/nodejs/`）
- **Java** — `sdk/java/`

## 🛠️ 使用方式

### CLI

```bash
# 压缩构建日志
tokenslim -i build.log -o output.json --reorder

# AI 友好的降噪诊断报告
tokenslim decompress -i output.json -o ai_report.txt --ai-export

# 高信号有损模式（保留 error 窗口 + 关键元数据）
tokenslim decompress -i output.json -o ai_signal.txt --ai-signal

# 静态规则校验（单文件）
tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule/sample_fixture.log \
  --verify-expected tests/fixtures/static_rule/sample_expected.txt

# 静态规则校验（批量、目录模式）
tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule \
  --verify-expected tests/fixtures/static_rule

# 项目引导 & Shell hooks
tokenslim init
tokenslim workspace
tokenslim --dry-run workspace --inject
tokenslim workspace --inject
tokenslim hooks install
tokenslim hooks status
tokenslim hooks uninstall
```

### Server (Sidecar)

```bash
tokenslim-server
# 监听 127.0.0.1:<port>，见 /health, /compress, /decompress
```

### SDK

```python
# Python
from tokenslim import compress, decompress
compressed = compress(open("build.log").read())
print(decompress(compressed, mode="ai-export"))
```

```javascript
// Node.js
const { compress, decompress } = require("tokenslim");
const compressed = compress(fs.readFileSync("build.log", "utf8"));
console.log(decompress(compressed, { mode: "ai-export" }));
```

```java
// Java
TokenSlimClient client = new TokenSlimClient("http://127.0.0.1:8080");
String compressed = client.compress(logText);
String report = client.decompress(compressed, "ai-export");
```

## 🔌 插件

TokenSlim 自带 **60+ 插件**，覆盖 LLM 真实流量里最常见的输入源。每个插件都是数据驱动的（JSON / TOML 配置在 `config/plugins/` 下），分派走路由匹配，因此添加新源格式在大多数情况下是**纯配置改动**。

完整注册表见 [`config/plugins/`](./config/plugins/)，或运行：

```bash
tokenslim plugins list
tokenslim explain-plugin --explain-command "cargo build"
```

## 🔗 集成

| 入口 | 路径 | 状态 |
|---|---|---|
| CLI | `src/bin/tokenslim-server.rs`, `src/cli/` | 稳定 |
| REST Server | `src/bin/tokenslim-server.rs` | 稳定 |
| VS Code | `vscode-extension/` | 稳定 |
| Chrome | `chrome-extension/` | 稳定 |
| JetBrains | `jetbrains-plugin/` | 稳定 |
| Python SDK | `crates/tokenslim-py/` | 稳定 |
| Node.js SDK | `sdk/nodejs/` | 稳定 |
| Java SDK | `sdk/java/` | 稳定 |

## 🏗 架构

TokenSlim 走分层流水线：

1. **路由分派** — 按命令 / 内容签名选择插件。
2. **插件链** — 每个插件自管抽取、折叠、语义替换。
3. **压缩核心** — radix-trie 路径抽取、字典分层、全局去重。
4. **回放（rehydration）** — 圆 trip 安全，原始输入可以从压缩形式完整恢复。
5. **AI Export / Signal** — 为 LLM 消费而设计的上下文感知后处理。

完整设计见 `docs/development/ARCHITECTURE.md`。

## 🤝 贡献

欢迎贡献。请先开 issue 讨论大改动；小修复与新插件配置可以直接发 PR。

```bash
# 跑测试
cargo test

# 用样本运行
tokenslim -i samples/web_log_plugin/case_001_access.log -o out.json --reorder
```

详细贡献指南见 [CONTRIBUTING.md](./CONTRIBUTING.md)。

## 📄 许可证

[MIT](./LICENSE)
