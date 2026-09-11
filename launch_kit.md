# TokenSlim Launch Kit (发布物料库)

这是为你准备的 TokenSlim 发版发帖物料。不同平台的受众口味完全不同，一定要“看客下菜碟”。以下是为你量身定制的四个平台的发帖模板。

---

## 1. V2EX 发帖模板
**受众画像**：国内技术极客、独立开发者。不喜欢套话，喜欢看痛点和硬核技术，对省钱/白嫖敏感。
**发帖节点**：`分享创造` 或 `程序员`

**标题**：
> [造轮子] 我用 Rust 写了个 AI 专用的上下文“脱水”引擎，把 Token 消耗降低了 50%-95%

**正文**：
> 大家好，我是个做 SCM 和 ALM 出身的老兵。最近在折腾 AI Coding Agent 时发现一个巨痛点：每次把一大坨 CI/CD 构建日志、堆栈报错、或者长配置文件丢给 ChatGPT/Claude 时，不仅极度浪费 Token（特别是用 API 的时候肉疼），而且经常爆上下文。
>
> 市面上没有趁手的工具，我就自己用 Rust 写了一个：**TokenSlim**。
>
> 简单来说，它的作用是**无损/微损地榨干文本里的水分**，只把最核心的语义骨架留给 AI。
> 
> **硬核指标：**
> - 纯 Rust 编写，无运行时依赖。速度大概在 **200MB/s**（并发模式下），基本就是瞬间完成。
> - 内置了 **60+ 个专有插件**。不管你是扔给它一堆 Git diff、Maven 报错、Webpack 日志，还是长篇的 JSON/YAML，它都会用 Aho-Corasick 算法自动路由到对应的插件进行精准裁剪。
> - 支持 `compress_whitelist` 和 `tty_support` 双名单分发，带 ConPTY 转发。
> - 为了方便使用，我还打包了 Chrome 扩展（无感压缩网页端的 AI 聊天框）、VS Code 扩展、JetBrains 插件，并且配了 Node.js / Python / Java 三种 SDK。
> 
> 这个项目自己一个人手敲了大概 2800 多个 commit，目前发布了 v0.4.1 版本。
> 
> **开源地址**：https://github.com/nuoyazhizhou/tokenslim
> **详细的性能和压缩率对比可以直接看 README。**
> 
> 大家如果平时经常要把大段代码和日志喂给大模型，可以装个试试，轻喷~

---

## 2. Hacker News (HN) 发帖模板
**受众画像**：全球最挑剔的极客。喜欢“Show HN”、硬核架构设计（SIMD/Rayon 等），讨厌过度营销。
**发帖格式**：必须以 `Show HN: ` 开头。

**标题**：
> Show HN: TokenSlim – A Rust engine that compresses AI context by 50-95% at 200MB/s

**正文**：
> Hi HN,
>
> I’ve spent the last few months (and ~2,800 commits) building TokenSlim, a blazing-fast context compression engine designed specifically for LLMs.
> 
> **The Problem:** Feeding raw CI/CD logs, massive JSONs, or verbose stack traces into models like Claude/GPT consumes a massive amount of tokens and often hits context limits. Existing naive truncation loses critical semantic info.
> 
> **The Solution:** TokenSlim uses a 7-stage pipeline written in Rust to intelligently strip out the "noise" (boilerplate, timestamps, repetitive JSON structures, etc.) while preserving the "semantic skeleton" that LLMs actually need to reason. 
> 
> **Under the hood:**
> *   **60+ Domain-Specific Plugins:** It uses the Aho-Corasick algorithm to route input (Git diffs, GCC logs, Maven, Webpack, etc.) to specific parsers.
> *   **Performance:** Built with `bumpalo` (arena allocation), `memchr` (SIMD line splitting), and `rayon` (parallel processing). It achieves ~200MB/s throughput.
> *   **Ecosystem:** It ships with 6-platform binary npm packages, a built-in Axum WebUI, and SDKs for Python, Node.js, and Java. I've also bundled extensions for Chrome, VS Code, and JetBrains.
> *   **Audit Pipeline:** To ensure compression doesn't degrade LLM reasoning, it runs through a 4-step semantic fidelity audit pipeline with over 2,200 sample cases.
> 
> I initially built this for my own AI coding agents, but realized it might be useful for anyone struggling with API costs or context limits.
> 
> Repo: https://github.com/nuoyazhizhou/tokenslim
> 
> Would love your feedback on the architecture, plugin system, or any edge cases you might throw at it!

---

## 3. Reddit - r/rust 发帖模板
**受众画像**：Rust 语言爱好者。关注底层实现、Crate 选择、内存管理、架构设计。

**标题**：
> I built a 7-stage AST/Log compression engine in Rust to save LLM API costs (TokenSlim)

**正文**：
> Hey fellow Rustaceans 🦀!
> 
> I wanted to share a project I've been working intensely on (almost 2.8k commits single-handedly!). It's called **TokenSlim** — a tool designed to compress logs, stack traces, and code before feeding them into LLMs, reducing token usage by 50-95%.
> 
> While the AI part is cool, I wanted to share some of the **Rust-specific engineering** behind it:
> 
> 1.  **Architecture**: It uses a 7-stage pipeline (`StreamReader` -> `TextSlicer` -> `ContentAnalyzer` -> `PluginDispatcher` -> `DictionaryEngine` -> `DedupEngine` -> `CompressionOutput`).
> 2.  **Plugin Routing**: Implemented via the `aho-corasick` crate for ultra-fast multi-pattern matching to route text to one of the 60+ domain-specific plugins.
> 3.  **Performance**: Used `bumpalo` for arena allocation to avoid massive allocation overheads when slicing large files. Combined with `memchr` for SIMD line-splitting and `rayon` for data-parallelism, it hits about 200MB/s throughput.
> 4.  **Distribution**: Used cross-compilation CI to distribute 6 platform binaries natively via an `npm` wrapper, so JS developers don't even know it's Rust under the hood.
> 5.  **Built-in Server**: Uses `axum` combined with `rust-embed` to pack a complete Single Page Application (Web UI) directly into the standalone binary.
> 
> Link: https://github.com/nuoyazhizhou/tokenslim
> 
> I've learned a ton about managing large workspaces and building robust CLI/Server dual-binaries. Happy to answer any questions about the Rust implementation!

---

## 4. Reddit - r/OpenAI 或 r/LocalLLaMA 发帖模板
**受众画像**：AI 使用者、提示词工程师、关注降低 API 成本和绕过上下文限制的人。

**标题**：
> Stop wasting API tokens on useless log boilerplate. I built a tool that compresses context by up to 95%.

**正文**：
> If you're building coding agents or just pasting massive error logs into ChatGPT/Claude, you know how fast you burn through your context window (and your wallet).
> 
> I built an open-source engine called **TokenSlim** to solve this. Instead of dumb truncation, it uses 60+ smart parsers to identify *what* you're pasting (e.g., a Git diff, a Python traceback, a Maven build log, or a huge JSON file) and removes all the fluff that the LLM doesn't need to understand the problem.
> 
> **Why it's useful:**
> - Fits massive CI/CD failure logs into standard context windows.
> - Drastically reduces your OpenAI/Anthropic API bills.
> - Includes a Chrome Extension that silently compresses text in the background before you hit "send" on the ChatGPT/Claude web interface.
> - Has VS Code and JetBrains extensions.
> 
> It's blazing fast (written in Rust) and runs entirely locally, so your data stays private until you decide to send the compressed version to the AI.
> 
> Check it out here: https://github.com/nuoyazhizhou/tokenslim
> Let me know if it helps with your token limits!
