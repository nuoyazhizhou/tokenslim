<p align="center">
  <h1 align="center">TokenSlim</h1>
  <p align="center">
    High-performance Rust token compression engine for LLM inputs.<br>
    Plugin-based · 50–95% token savings · AI-export diagnostics · CLI / Server / IDE / SDK
  </p>
</p>

<p align="center">
  <a href="#what-is-tokenslim">What is TokenSlim</a> ·
  <a href="#why-tokenslim">Why</a> ·
  <a href="#features">Features</a> ·
  <a href="#installation">Installation</a> ·
  <a href="#usage">Usage</a> ·
  <a href="#plugins">Plugins</a> ·
  <a href="#integrations">Integrations</a> ·
  <a href="#license">License</a>
</p>

<p align="center">
  <strong>English</strong> · <a href="./README.zh-CN.md">简体中文</a> · <a href="./README.ja.md">日本語</a> · <a href="./README.ko.md">한국어</a> · <a href="./README.es.md">Español</a> · <a href="./README.fr.md">Français</a> · <a href="./README.de.md">Deutsch</a> · <a href="./README.ar.md">العربية</a>
</p>

---

## What is TokenSlim?

TokenSlim is a high-performance, plugin-based text compression engine written in Rust. Its core mission is to **dramatically reduce the token cost of LLM inputs** and to make it possible to fit long, noisy real-world logs (build pipelines, CI runs, web access logs, database traces, cloud logs, VCS output, stack traces, etc.) into LLM context windows — without losing the diagnostic signals the model needs.

On highly structured, repetitive inputs (compiler logs, build output, CI logs, access logs, etc.), TokenSlim typically delivers **50%–90%** reduction while preserving 100% of the original information. In its **AI Export** mode, designed specifically for LLM consumption, the reduction reaches **90%–95%** with context-aware denoising that keeps the error/warning context the model needs to reason about.

Beyond compression, TokenSlim ships with environment-diagnostic tooling (`workspace`, `encoding`, `rule`, `env` commands) that auto-detects OS, shell, code page, Python/Node/JDK encoding configuration, flags mojibake risk, and emits actionable fixes. Combined with a subprocess decoding fallback chain (UTF-8 first, codepage candidates next), it stays reliable across mixed-language environments.

## Why TokenSlim?

### 1. Real money saved
LLM API cost is dominated by input token count. TokenSlim cuts that by 50–95%:

- **Lower API bills** — 50–95% fewer input tokens.
- **Context-aware AI Export (`--ai-export`)** — strips routine lines, keeps the error/warning window the model actually needs; reduces hallucination on noisy inputs.
- **Longer effective context** — same context window, more real signal.
- **Faster prefill** — shorter inputs usually mean faster model prefill and lower TTFT.

### 2. Industrial-grade performance
- **Zero-copy pipeline** — built on Rust `Cow<'a, str>`, parallel block processing with `rayon`, and `Bump` arena allocation. Processes 100 MB of industrial-grade log in **~250 ms**, ~400 MB/s throughput.
- **Deterministic global reordering** — a streaming build-target tracker fixes the out-of-order interleaving produced by `make -jN` / `Ninja`. Two identical parallel builds always produce the same error stack order.
- **Sidecar mode** — high-throughput REST API server, embeddable into IDE / CI / Agent workflows with zero startup overhead.

### 3. Data-driven extraction
- **Radix-trie path extraction** — TokenSlim does not slice line-by-line. After scanning 100 MB of input, it builds a project-wide radix trie in memory and only emits directory dictionaries (`$D`) on hot branches (weight > 10), eliminating fragmentary tokens.
- **Semantic markers** — environment-aware substitutions for Android, iOS, GCC, MSVC, and linkers.
- **Full build ecosystem detection** — C/C++, Rust, Go, Java, Android, iOS/Xcode, MSVC, Swift, and major linkers, with context-aware folding and error deduplication.

## Features

- **Three runtimes**
  - **CLI** — scriptable batch processing
  - **Server** — long-lived REST API for full ecosystem integration
  - **SDKs** — Java, Python (PyO3), Node.js
- **Plugin ecosystem** (60+ plugins covering the most common LLM-input sources)
  - **Mobile** — `android_gradle`, `xcode_log`
  - **General dev** — `gcc_log`, `java_stack`, `python_traceback`, `dotnet`, `rust_go`, `maven`, `gradle`, `node_error`, `nodejs`, `php_ruby`, `unity_unreal`
  - **Structured data** — `json`, `yaml`, `xml_html`, `ndjson`, `protobuf`
  - **Build artifacts** — `artifact_summary` (SARIF / JUnit XML), with semantic preservation of test status, SARIF level/rule/location/tool
  - **Cloud & ops** — `cloud_log` (AWS / GCP / Azure / Alibaba / OCI / Tencent / Huawei / Cloudflare), `web_log` (Nginx / Apache / ingress / Envoy / CloudFront / IIS / ALB / Cloudflare), `db_log` (PostgreSQL / MySQL / MongoDB / Redis), `syslog`
  - **CI/CD** — `ci_log` (GitHub Actions / GitLab CI / Jenkins / Azure Pipelines / CircleCI / Buildkite / local `act` / TeamCity / Travis CI)
  - **VCS** — unified `vcs_plugin` for git / svn / hg / p4 / cvs / bzr / fossil / darcs, plus `git_diff`, `smart_code` (AST-level), `smart_path`
- **Environment diagnostics** — `workspace`, `encoding`, `rule`, `env` subcommands detect mojibake risk and emit fix recipes.
- **AI-native output modes**
  - `--ai-export` — context-aware denoising, keeps error/warning windows
  - `--ai-signal` — lossy but high-signal, preserves the most decision-relevant fields
- **Plugin introspection** — `tokenslim explain-plugin` and `tokenslim run --explain-route` explain route selection, fallbacks, confidence, alternatives, and replay misclassifications for audit.

## Installation

### One-liner install (any platform — recommended)

```bash
# Project-local
npm install tokenslim-sdk

# Or globally so `tokenslim` / `tokenslim-server` are on PATH
npm install -g tokenslim-sdk
```

`tokenslim-sdk` ships 6 platform-specific `optionalDependencies` (`@tokenslim/cli-binary-linux-x64-gnu`, `…-linux-arm64-gnu`, `…-darwin-x64`, `…-darwin-arm64`, `…-windows-x64`, `…-windows-arm64`). npm/pnpm/yarn automatically installs the one matching your OS + CPU, pulling the `tokenslim` + `tokenslim-server` binaries and 60+ plugin configs into `node_modules/`. A small Node wrapper at `bin/tokenslim.js` then forwards each call to the real binary.

If network access is unavailable and the optional package fails to install, the `postinstall` script transparently falls back to downloading from GitHub Releases. If that also fails, the install still succeeds — only the CLI commands become unavailable; the JS SDK keeps working as a REST client.

### From source (Rust toolchain ≥ 1.75)

```bash
git clone https://github.com/nuoyazhizhou/tokenslim.git
cd tokenslim
cargo build --release
```

The binaries land at `./target/release/tokenslim` and `./target/release/tokenslim-server` (or `*.exe` on Windows).

### Prebuilt binaries (no Node)

Download both binaries from the [Releases](https://github.com/nuoyazhizhou/tokenslim/releases) page.

### Configuration (optional)

All runtime configuration goes through environment variables. Copy [`.env.example`](./.env.example) to `.env` and fill in your local values. `.env` is git-ignored by default; only the example template is tracked.

Most users only need `RUST_LOG=info` (or `debug` for verbose tracing). The LLM-audit related variables (`OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_MODEL`) are only required if you run `scripts/audit_*.py --llm-audit` — without them, audits degrade to lint-only mode.

### Editor / IDE integrations

- **VS Code** — see `vscode-extension/`
- **Chrome** — see `chrome-extension/`
- **JetBrains** — see `jetbrains-plugin/`

### SDKs

- **Node.js / TypeScript** — `npm i tokenslim-sdk` (source: [`packages/sdk-nodejs/`](./packages/sdk-nodejs/)). Ships the `tokenslim` + `tokenslim-server` binaries as `optionalDependencies` — see [Installation](#installation) above.
- **Python** — see [`sdk/python/tokenslim_sdk.py`](./sdk/python/tokenslim_sdk.py) (single-file client)
- **Java 11+** — see [`sdk/java/TokenSlimClient.java`](./sdk/java/TokenSlimClient.java)

> 📖 [5-minute Quickstart](./docs/guides/QUICKSTART.md) · [Full SDK usage guide](./docs/guides/SDK_USAGE.md) · [User guide](./docs/guides/USER_GUIDE.md)

## Usage

### CLI

```bash
# Compress a build log
./target/release/tokenslim -i build.log -o output.json --reorder

# AI-friendly denoised diagnostic report
./target/release/tokenslim decompress -i output.json -o ai_report.txt --ai-export

# High-signal lossy mode (keeps error window + key metadata)
./target/release/tokenslim decompress -i output.json -o ai_signal.txt --ai-signal

# Static rule validation (single file)
./target/release/tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule/sample_fixture.log \
  --verify-expected tests/fixtures/static_rule/sample_expected.txt

# Static rule validation (batch, directory mode)
./target/release/tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule \
  --verify-expected tests/fixtures/static_rule

# Project bootstrap & shell hooks
./target/release/tokenslim init
./target/release/tokenslim workspace
./target/release/tokenslim --dry-run workspace --inject
./target/release/tokenslim workspace --inject
./target/release/tokenslim hooks install
./target/release/tokenslim hooks status
./target/release/tokenslim hooks uninstall
```

### Server (Sidecar)

```bash
./target/release/tokenslim-server
# Listens on 127.0.0.1:<port>, see /health, /compress, /decompress
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
const { compress, decompress } = require("tokenslim-sdk");
const compressed = compress(fs.readFileSync("build.log", "utf8"));
console.log(decompress(compressed, { mode: "ai-export" }));
```

```java
// Java
TokenSlimClient client = new TokenSlimClient("http://127.0.0.1:8080");
String compressed = client.compress(logText);
String report = client.decompress(compressed, "ai-export");
```

## Plugins

TokenSlim ships with **60+ plugins** covering the inputs that dominate real LLM traffic. Each plugin is data-driven (JSON / TOML config under `config/plugins/`) and dispatch is route-based, so adding a new source format is a config-only change in most cases.

Browse the full registry at [`config/plugins/`](./config/plugins/), or run:

```bash
./target/release/tokenslim plugins list
./target/release/tokenslim explain-plugin --explain-command "cargo build"
```

## Integrations

| Surface | Path | Status |
|---|---|---|
| CLI | `src/bin/tokenslim-server.rs`, `src/cli/` | Stable |
| REST Server | `src/bin/tokenslim-server.rs` | Stable |
| VS Code | `vscode-extension/` | Stable |
| Chrome | `chrome-extension/` | Stable |
| JetBrains | `jetbrains-plugin/` | Stable |
| Python SDK | `crates/tokenslim-py/` | Stable |
| Node.js SDK | `packages/sdk-nodejs/` (npm: `tokenslim-sdk@0.1.0` — includes the CLI binaries) | Stable |
| Java SDK | `sdk/java/` | Stable |

## Architecture

TokenSlim follows a layered pipeline:

1. **Route dispatcher** — selects plugin(s) by command / content signature.
2. **Plugin chain** — each plugin owns extraction, folding, semantic substitution.
3. **Compression core** — radix-trie path extraction, dictionary layering, global dedup.
4. **Rehydration** — round-trip-safe so the original input is fully recoverable from the compressed form.
5. **AI Export / Signal** — context-aware post-processing for LLM consumption.

See `docs/development/ARCHITECTURE.md` for the full design.

## Contributing

Contributions are welcome. Please open an issue first to discuss larger changes; small fixes and new plugin configs can go straight to a PR.

```bash
# Run tests
cargo test

# Run with a sample
./target/release/tokenslim -i samples/web_log_plugin/case_001_access.log -o out.json --reorder
```

## License

[MIT](./LICENSE)
