<!--
本文件由 AI 工具自动迁移自 .qoder/repowiki/, 用于补充 docs/ 的视角。
- 生成器: Qoder (阿里云 AI IDE)
- 生成日期: 2026-06-23
- 原文件: .qoder/repowiki/knowledge/en/TokenSlim Configuration System/configuration_system.md
- 适用版本: TokenSlim v0.3.5 (扫描基线: C:\git_work\TokenSlim-publish2 当时快照)
- 维护说明: 内容已与项目 docs/ 现有文件去重; 若代码变更需人工校对。
-->

The TokenSlim configuration system is a multi-layered, file-driven architecture primarily using **TOML** for global and project-specific settings, and **JSON** for granular plugin definitions. It supports environment variable overrides for runtime behavior and external service integration.

### 1. Configuration Layers & Precedence
The system loads configuration in the following order (later layers override earlier ones):
1.  **Built-in Defaults**: Hardcoded in Rust source (`src/core/...`).
2.  **Global/Project Config (`.tokenslim.toml`)**: Located in the project root or user home. Generated via `tokenslim init` which auto-detects project type (Rust, Node, Python, etc.) and framework.
3.  **Plugin Registry (`config/plugins.toml`)**: Defines enabled plugins, static/dynamic loading strategies, priorities, and plugin-specific toggles (e.g., `compress_paths`, `dedup_threshold`).
4.  **Plugin Definitions (`config/plugins/*.json`)**: Individual JSON files for each plugin (e.g., `gcc_log.json`, `vcs_git.json`) containing detection rules (regex/patterns), compression tokens, and decompression prefixes.
5.  **Environment Variables (`.env` / System Env)**: Used for sensitive data (LLM API keys) and runtime debugging (`RUST_LOG`, `TOKENSLIM_LOG`).

### 2. Key Files & Packages
- **`.tokenslim.toml`**: Project-level entry point. Controls high-level behaviors like `reorder`, `ai_export`, `preset` (fast/balanced/ai), and `semantic_fallback`.
- **`config/plugins.toml`**: Central plugin registry. Manages the `enabled` list, `static_plugins` (compiled in), and `dynamic_plugins` (WASM/external). Also holds global engine thresholds (e.g., `dictionary_threshold`, `parallel_threshold`).
- **`config/plugins/*.json`**: ~60+ plugin-specific configs. Each defines `detect.rules` (how to identify log types), `compress` patterns (what to tokenize), and `decompress` prefixes.
- **`src/core/plugin_config_loader/mod.rs`**: The core loader. It scans `config/plugins/` for JSON files, compiles regex patterns into `Arc<Regex>` for performance, and merges them with `plugins.toml` settings.
- **`src/core/init_command/methods.rs`**: Handles `tokenslim init`. Detects project structure (e.g., `Cargo.toml`, `package.json`) to generate a tailored `.tokenslim.toml`.
- **`.env.example`**: Template for environment variables. Supports `OPENAI_API_KEY`, `RUST_LOG`, and `TOKENSLIM_LOG` for auditing and debugging.

### 3. Architecture & Conventions
- **Pre-compiled Regex**: Plugin configs are not parsed at every request. `PluginConfigLoader` compiles all regex patterns at startup into `CompiledPluginConfig` structures to minimize latency.
- **Static vs. Dynamic Plugins**: Core plugins are "static" (linked into the binary) for performance. The system supports a "dynamic" plugin architecture (via WASM or external binaries) defined in `plugins.toml`, though currently mostly static.
- **Route-Based Dispatch**: `config/plugins/*_route.json` files define command-line routing logic (e.g., mapping `git status` to the `vcs` plugin). This allows the CLI to intercept external commands transparently.
- **Preset System**: Compression behavior is layered into presets (`fast`, `balanced`, `ai`) defined in `.tokenslim.toml`. These presets adjust token optimization costs (e.g., `path_parse_cost`, `nested_parse_cost`) to balance speed vs. compression ratio.

### 4. Developer Rules
- **Adding a Plugin**: 
  1. Create a new JSON file in `config/plugins/` (e.g., `my_plugin.json`).
  2. Define `detect.rules` using `any` (string match) or `regex` types.
  3. Add the plugin name to the `enabled` list in `config/plugins.toml`.
  4. Implement the corresponding Rust logic in `src/plugins/`.
- **Configuration Changes**: Modifying `config/plugins.toml` or `.tokenslim.toml` requires a restart of the CLI/Server to reload. Plugin JSON changes are also loaded at startup.
- **Secrets Management**: Never commit API keys. Use `.env` (ignored by git) for `OPENAI_API_KEY` or other LLM credentials. The system reads these via `std::env::var`.
- **Routing Logic**: To add support for a new CLI tool interception, create a `*_route.json` file in `config/plugins/` defining `command_keywords` and `route_group`.
---

<!--
来源: knowledge/en/TokenSlim Configuration System/configuration_system.md  |  生成器: Qoder (阿里云 AI IDE)  |  扫描基线: publish2 @ 2026-06-23
-->
