# Changelog

All notable changes to TokenSlim are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **Legend**: `+` Added · `~` Changed · `-` Removed · `!` Fixed · `^` Security
>
> **Range covered**: v0.2.6 → v0.3.7 → v0.4.0 → HEAD. 0.2.6 / 0.3.0 are scaffold releases (no user-facing changes).

---

## [0.4.0] — 2026-06-24 (双清单 + ConPTY 转发, 替代启发式黑名单)

### Added
- **`compress_whitelist` + `tty_support_list` 双清单机制** — v0.4.0 核心新设计. `run` 入口的 3 路分发不再依赖 v0.3.7 的启发式黑名单 (`is_git_program` / `detect_git_interactive`), 改用"已知可压缩 / 已知支持 tty"两个白名单 + 三层配置合并 (L1 代码默认 / L2 项目 `config/whitelist.toml` / L3 用户 `~/.tokenslim-whitelist.toml`). 命令在 `compress_whitelist` → 走 plugin 压缩; 命令在 `tty_support_list` 且 ConPTY 可用 → 走 ConPTY 转发; 其余命令 → 走 passthrough 兜底.
- **L1 默认 compress 清单 (~50 个)** — `git` / `svn` / `hg` / `fossil` / `p4` / `bzr` / `cvs` / `darcs` / `git-lfs` / `glab` / `gh` / `make` / `cmake` / `ninja` / `meson` / `gradle` / `mvn` / `ant` / `sbt` / `msbuild` / `dotnet` / `cargo` / `rustc` / `npm` / `yarn` / `pnpm` / `npx` / `pip` / `go` / `javac` / `ls` / `dir` / `cat` / `type` / `head` / `tail` / `wc` / `grep` / `find` / `where` / `which` / `tree` / `du` / `df` / `sort` 等.
- **L1 默认 tty 清单 (58 个)** — 编辑器 (vim / vi / nvim / emacs / nano / pico / code / subl / micro / helix / hx / kak / kakoune / neovide) + REPL/脚本语言 (python / python3 / ipython / node / deno / bun / irb / ruby / pry / scala / ghci / ghcup / julia / R / Rscript / lua / perl / php / sqlite3 / mysql / psql / mongosh / redis-cli) + 远程 (ssh / telnet / ftp / sftp / scp / rsync / mosh) + 分页器 (less / more / most) + Subshell (bash / zsh / fish / sh / dash / ksh / csh / tcsh / powershell / pwsh / cmd / wsl).
- **L2 项目配置 `config/whitelist.toml`** — 模板包含 4 段 (`[compress.extra]` / `[compress.remove]` / `[tty.extra]` / `[tty.remove]`), 通过 `include_str!` 嵌入 binary, 编译期固定.
- **L3 用户配置 `~/.tokenslim-whitelist.toml`** — 用户级配置, 启动时动态读盘, 解析失败自动降级为空 (fail-soft).
- **ConPTY 转发 (`portable-pty`)** — 新增 `src/cli/pty_runner.rs`, 用 `portable-pty` (Windows ConPTY / Unix pty) 启动子进程, 把 stdio 桥接到真 tty, 支持 vim / ssh / REPL 等全屏交互. 主线程做 stdio 桥接, 用 mpsc 把 reader 线程字节发回主线程, 解决 Windows `StdoutLock` 的 `!Send` 问题.
- **ConPTY 可用性探测** — 新增 `src/cli/conpty_probe.rs`, 启动时一次性探测 (`cmd /c ver` + 1 秒超时, `OnceLock` 缓存结果), 沙箱环境 (Trae IDE / Windows Server Core) 自动返回 `false`, 触发 fallback.
- **`tokenslim-whitelist.toml.example`** 模板 — 仓库内提供 L3 配置示例 (`tokenslim-whitelist.toml.example`), 列出 `k9s` / `lazygit` / `tig` 等典型 L3 扩展项.
- **`config/whitelist.toml`** 模板 — 仓库内提供 L2 项目级双清单模板, 默认全空, 4 段结构完整.

### Changed
- **`run_run_mode` 重写为 3 路分发** — `src/cli/commands/run.rs` 内的 `run_run_mode` 不再有 `if detect_git_interactive(...)` 启发式短路, 改为先查 `compress_whitelist` → 再查 `tty_support_list` → 兜底 passthrough. 决策逻辑从"我们不支持什么"变成"我们支持什么", 维护负担大幅降低.
- **拆分 `run_compress_route` + `run_tty_route` 子函数** — 3 路分发逻辑提到 `run_run_mode` 主体, 两个 route 函数分别负责压缩路径与 tty 路径, 关注点分离.
- **CLI 入口处理 `unknown` 命令** — 任何不在两个清单的命令 (例如 `my-random-tool` / `foobar`) 自动走 passthrough, 退出码 1:1 透传.

### Removed
- **`is_git_program` 函数** — v0.3.7 启发式黑名单的 git 路径识别函数, 12 个 unit test 随之一并删除.
- **`detect_git_interactive` 函数** — v0.3.7 启发式黑名单的交互式参数检测函数 (`commit` / `rebase` / `tag` / `add -p` / `checkout -p` / `clean -i` 等), 12 个 unit test 随之一并删除.
- **v0.3.7 黑名单相关的 `eprintln!` 调试输出** — `run_external_command_passthrough` 注释从"git 交互式子命令的 fallback"更新为"未知命令的通用 fallback".

### Fixed
- **Trae IDE 沙箱下 ConPTY 不可用** — 之前 `tokenslim run vim` 在 Trae IDE 终端会卡死, 现在自动降级为 stdio passthrough, 至少不卡死.
- **未知命令的"我们不支持什么"漏洞** — v0.3.7 黑名单只能枚举已知坏命令, 新版白名单 + fallback 反向安全: 不知道的命令永远不会硬性走错路线.

### Security
- **白名单思想** — "我们支持什么自己知道, 我们不支持什么不知道". 任何不在 L1/L2/L3 清单的命令永远走 passthrough, 退出码 1:1 透传, 不会因为"猜错分类"而丢失用户数据.

---

## [0.3.7] — 2026-06-24 (interactive git fallback hotfix)

### Fixed
- **`tokenslim run git <subcmd>` 卡死 bug** — 之前 `run` 接管 stdout/stderr 时不转发 tty, 任何交互式 git 子命令 (`commit` 无 `-m`、`rebase -i`、`tag -a` 无 `-m`、`add -p`、`checkout -p`、`clean -i` 等) 都会因为子进程读不到 tty 而卡住. 新增启发式检测 + 透明 fallback: 命中黑名单时放弃压缩, 直接 `Command::new("git").args(...).stdin/stdout/stderr(Stdio::inherit())` 透传 stdio, 退出码 1:1 透传给调用方. 非 git 命令完全不受影响, `git status` / `git log` / `git diff` 等非交互子命令照常走压缩.

---

## [0.3.6] — 2026-06-24 (vcs_git unmerged fix + MCP server)

### Added
- **`tokenslim-mcp-server`** — a built-in [Model Context Protocol](https://modelcontextprotocol.io) server under `mcp-server/`. Exposes `compress` / `decompress` / `smart-compress` / `stats` / `compress-file` tools and `config` / `plugins` resources so any MCP-compatible agent (Claude Code, Cursor, Windsurf, Qoder, OpenCode) can drive TokenSlim through the standard protocol. See `mcp-server/README.md`.
- **MCP Server integration guide** — new README subsection in `README.md` and `README.zh-CN.md` with `npm install && npm run build` quick start and Cursor `.cursor/mcp.json` example.
- **CLI per-command modules** — split the 8 800-line `src/cli/methods.rs` into `src/cli/commands/{benchmark,compress,config,decompress,doctor,export,repair,run}.rs` + `src/cli/app.rs` + `src/cli/common.rs`. `methods.rs` is removed. Compilation is unchanged; per-command hot paths are now independently editable.
- **`--json` global flag** — new CLI flag that emits structured JSON output for machine consumption.

### Changed
- **Embedding engine now gated behind `experimental` feature** — the `ml` feature stack (`candle-core`, `candle-nn`, `tokenizers`, `hf-hub`, `candle-transformers`) is now reachable only via the new `experimental = ["ml"]` umbrella. `candle-core` cfg-gates in `src/core/embedding_engine/mod.rs` are renamed to `experimental` to match. The default build no longer carries the heavy ML toolchain.
- **PyPI build matrix expanded** — `crates/tokenslim-py/pyproject.toml` and `.github/workflows/pypi-publish.yml` now build wheels for Python 3.9, 3.10, 3.11, 3.12, 3.13 on linux / macOS / Windows, replacing the previous 3.10-only setup.

### Removed
- **`crossbeam-channel` dependency** — workspace + member entries dropped; no module referenced it after the CLI refactor.

### Fixed
- **`vcs_git_plugin` lost unmerged-path conflicts** — `git status` output containing the "Unmerged paths:" section was previously compressed into an empty file. `VcsRecord::File.status` now stores a two-character git porcelain v1 code (`UU` / `AU` / `UA` / `DU` / `UD`) instead of a single `char`, so the five distinct conflict flavours (both modified, added-by-us, added-by-them, deleted-by-us, deleted-by-them) all survive the round-trip. Adds a physical sample (`case_330_git_status_unmerged`) and a regression test (`git_status_preserves_unmerged_paths`); 4-step audit pipeline (case_quality → case_metrics → all_metrics → capability_index) reports `semantic_gate_passed=88/88` for vcs_git.

---

## [0.3.3] — 2026-06-23 (WebUI + i18n + WebSocket tail)

### Added
- **`tokenslim-server` Web UI** — a built-in single-page UI embedded into the binary via `rust-embed`. It serves `webui/` at `/` (with a graceful no-404 fallback if the directory is absent) and supports interactive compression, SSE streaming preview, and live WebSocket log tail.
- **SSE streaming compression** — new endpoint `POST /compress/stream` pushes progress events to the client before the final result, so very large inputs no longer block on a single response.
- **WebSocket tail endpoint** — `GET /ws/tail` for the WebUI's live tail mode.
- **`/plugins` metadata endpoint** — list all registered plugins (id, version, endpoints, enabled flag).
- **WebUI assets, e2e tests, 9-locale translations** — new `webui/` directory, Playwright e2e harness, and complete translation of every UI string into `ar / de / en / es / fr / ja / ko / zh-CN / zh-TW`.
- **WebUI screenshots + audit-pipeline notes** — `docs/webui-screenshots/` captures for home, results, diff, AI export, responsive view; `docs/audit/` gains a one-page pipeline overview.
- **Log Reordering engine** — deterministic global reordering via `--reorder` (CLI) / `reorder: true` (Server) / "Enable reorder" (WebUI) / `log_reorder` standalone binary. Fixes the non-deterministic interleaving produced by `make -jN`, `ninja`, Bazel, MSBuild, etc.
- **`log_reorder` standalone binary** — pure log→log diff tool with `--deterministic` / `-n` (normalize) / `-p` (shorten-paths) flags.
- **i18n expansion to 9 locales** — server webui strings translated into `ar / de / en / es / fr / ja / ko / zh-CN / zh-TW`; coverage reporting automated via `audit_i18n_coverage.ps1`.
- **Plugin audit dashboard** — `scripts/aggregate_audit_health.py` aggregates per-plugin `audit_state.json` into `docs/audit/audit_health.md` (58 plugins × 1128 cases).
- **Editor / IDE integrations** — VS Code, JetBrains, and Chrome extension skeletons (`vscode-extension/`, `jetbrains-plugin/`, `chrome-extension/`) at v0.1.0.

### Changed
- **README overhaul** — 8 locales unified to a feature-first layout (English master) with badges, "See It in Action" section, dedicated **Log Reordering** subsection before Plugins, and updated architecture / audit pipeline diagrams.
- **Documentation sync** — `docs/development/ARCHITECTURE.md` and `docs/design/server.md` brought in line with the actual code (58 plugins, 12 server endpoints).
- **i18n keys** — 3 new server webui keys (`ui.reorder`, `ui.ai_export`, `ui.ai_signal`) translated into 9 locales via the new `translate_messages_fields.py` Google-Translate helper.
- **Benchmark refresh (2026-06-24)** — `non_mmap+parallel` remains the fastest path at ~176 MB/s on the 20MB reference input.

### Fixed
- **`gcc_log_plugin/audit_state.json` UTF-8 BOM** — aggregator script now uses `utf-8-sig` so all 58 plugin states are readable.
- **Resource file CRLF drift** — `crlf_to_lf.py` normalises `resources/messages.*.json` to LF so `core.autocrlf=true` does not corrupt the working tree.
- **Empty `TOKENSLIM_API_KEY` no longer locks out the WebUI** — the server treats an empty env var the same as unset.
- **C2 pipeline benchmark reproducibility** — removed `mmap+serial` shortcut and made scenarios comparable across runs.
- **Bare `-v` stdin hang** — `-V` / `--version` / `version` subcommand no longer blocks on stdin.

### Security
- **i18n `ar` locale encoding** — verified UTF-8 to remove mojibake.

---

## [0.3.2] — 2026-06-23

> 仅 release 脚本与 `plugin-interface` 升级，**无用户可见功能变更**。

---

## [0.3.1] — 2026-06-23 (Python SDK + README badges + version fix)

### Added
- **Python wheel via maturin** — `crates/tokenslim-py/pyproject.toml` with a GitHub Actions release workflow that builds and uploads wheels to PyPI.
- **Project badges & "See it in action" sections** — added to all 8 README locales for parity with the English master.

### Fixed
- **Bare `-v` / `--verbose` stdin hang** — `-V` / `--version` / `version` subcommand intercepted at startup so it never falls through to the pipeline (later re-applied in 0.3.3 to the new `app.rs` after the CLI split).
- **`tokenslim serve --port` → `TOKENSLIM_PORT=… tokenslim-server`** — README example corrected to match the actual binary.
- **Auto-regenerate lockfile in CI** — release job runs `npm install` before publish so `package-lock.json` is always in sync with the resolved versions.

---

## [0.3.0] — 2026-06-23

### Fixed
- **README examples use `tokenslim` consistently** — earlier docs had a mix of `tokenslim` and the internal command names; all reader-facing examples now use the public CLI form.
- **CI auto-regenerates the npm lockfile** before publish so the shipped lockfile matches the resolved tree.

---

## [0.2.9] — 2026-06-23 (npm hygiene)

### Fixed
- **`package-lock.json` regenerated** so the `optionalDependencies` (per-platform CLI binaries) are fully resolved and reproducible.
- **`preuninstall` script** — npm uninstall now cleans up the shell hook (`hooks uninstall`) so removing the package leaves no dangling `tokenslim` shim.
- **README example** — `tokenslim serve --port` corrected to the actual `TOKENSLIM_PORT=10086 tokenslim-server` form.

---

## [0.2.8] — 2026-06-23 (npm rename + CI fix)

### Changed
- **npm package renamed** from `tokenslim-sdk` to `tokenslim`. Existing users on `tokenslim-sdk` are advised to upgrade in one step (`npm uninstall tokenslim-sdk && npm install tokenslim`).

### Fixed
- **CI `head` under `pipefail`** — replaced `... | head` with `sed -n '1,5p'` so the build script no longer trips on SIGPIPE.

---

## [0.2.7] — 2026-06-22

> 版本号 bump + lockfile 同步，**无用户可见功能变更**。

---

## [0.2.6] — 2026-06-22 (Initial tagged Python prototype)

> 首个对外打 tag 的 Python 原型版本。涵盖后续 0.2.x / 0.3.x 演进的基线插件集（`git` / `gcc` / `maven` / `npm` / `cargo`）。功能列表详见 0.2.7 之后的增量条目。

---

## [0.1.x] — 2025-06..07 (Concept)

### Added
- Concept commit: 50-line Python script that grepped `Error:` lines and reported count.

[Unreleased]: https://github.com/nuoyazhizhou/tokenslim/compare/v0.3.7...HEAD
[0.3.7]: https://github.com/nuoyazhizhou/tokenslim/compare/v0.3.6...v0.3.7
[0.3.6]: https://github.com/nuoyazhizhou/tokenslim/compare/v0.3.3...v0.3.6
[0.3.3]: https://github.com/nuoyazhizhou/tokenslim/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/nuoyazhizhou/tokenslim/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/nuoyazhizhou/tokenslim/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/nuoyazhizhou/tokenslim/compare/v0.2.9...v0.3.0
[0.2.9]: https://github.com/nuoyazhizhou/tokenslim/compare/v0.2.8...v0.2.9
[0.2.8]: https://github.com/nuoyazhizhou/tokenslim/compare/v0.2.7...v0.2.8
[0.2.7]: https://github.com/nuoyazhizhou/tokenslim/compare/v0.2.6...v0.2.7
[0.2.6]: https://github.com/nuoyazhizhou/tokenslim/compare/v0.1.x...v0.2.6
