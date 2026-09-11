<!--
本文件由 AI 工具自动迁移自 .qoder/repowiki/, 用于补充 docs/ 的视角。
- 生成器: Qoder (阿里云 AI IDE)
- 生成日期: 2026-06-23
- 原文件: .qoder/repowiki/knowledge/en/Multi-Ecosystem Dependency Management Strategy/dependency_management.md
- 适用版本: TokenSlim v0.3.5 (扫描基线: C:\git_work\TokenSlim-publish2 当时快照)
- 维护说明: 内容已与项目 docs/ 现有文件去重; 若代码变更需人工校对。
-->

The TokenSlim repository employs a polyglot dependency management strategy tailored to its multi-component architecture, primarily driven by a Rust core with auxiliary TypeScript, Kotlin/Java, and Python integrations.

### 1. Rust Core (Primary Engine)
- **System**: Uses **Cargo** as the package manager and build tool.
- **Workspace Structure**: The project is configured as a Cargo workspace (`[workspace]` in `Cargo.toml`) containing the main `tokenslim` binary/library and the `crates/tokenslim-py` Python binding crate.
- **Versioning & Locking**: Dependencies are declared in `Cargo.toml` with specific version constraints (e.g., `axum = "0.8.8"`, `tokio = "1.36"`). A comprehensive `Cargo.lock` file is maintained to ensure deterministic builds across all environments. 
- **Internal Dependencies**: Internal crates like `plugin-interface` are referenced via local paths (`path = "./crates/plugin-interface"`) and versioned synchronously with the main package (`0.3.3`).
- **Feature Flags**: Optional dependencies for machine learning capabilities (e.g., `candle-core`, `tokenizers`) are gated behind the `ml` feature flag, allowing for a lightweight default build.

### 2. Web & IDE Extensions (TypeScript/JavaScript)
- **System**: Uses **npm** for package management.
- **Components**: 
  - `chrome-extension`: Minimal dependencies, relying primarily on `typescript` and `@types/chrome` for compilation.
  - `vscode-extension`: Depends on `typescript`, `@types/node`, and `@types/vscode` for extension development.
- **Lockfiles**: Both extensions maintain `package-lock.json` files to pin dependency versions.
- **Strategy**: These components are lightweight and primarily act as clients or UI layers, avoiding heavy third-party runtime dependencies.

### 3. JetBrains Plugin (Kotlin/JVM)
- **System**: Uses **Gradle** with the Kotlin DSL (`build.gradle.kts`).
- **Plugin**: Leverages the `org.jetbrains.intellij` Gradle plugin (version `1.15.0`) to manage IntelliJ Platform SDK dependencies.
- **Repositories**: Dependencies are resolved from `mavenCentral()`.
- **Configuration**: The build script configures the target IDE version (`2023.2.5`) and JVM compatibility (Java 17), ensuring consistent plugin compilation against the IntelliJ API.

### 4. Python Bindings & SDK
- **Bindings**: The `crates/tokenslim-py` crate uses **Maturin** (`pyproject.toml`) to bridge Rust code with Python. It specifies `maturin>=1.5,<2.0` as the build backend.
- **SDK**: The Python SDK (`sdk/python/tokenslim_sdk.py`) is a zero-dependency client using only standard library modules (`urllib`, `json`), avoiding external package management requirements for end-users.

### 5. Java & Node.js SDKs
- **Strategy**: Both the Java (`sdk/java/TokenSlimClient.java`) and Node.js (`sdk/nodejs/tokenslim-sdk.js`) SDKs are implemented as single-file, zero-dependency clients. They rely solely on standard library HTTP clients (`java.net.http`, `http` module) to interact with the TokenSlim sidecar server, eliminating the need for external package managers or dependency resolution for consumers.

### Developer Conventions
- **Deterministic Builds**: All managed ecosystems (Rust, npm) utilize lockfiles (`Cargo.lock`, `package-lock.json`) which must be committed to version control.
- **Local Path Dependencies**: Internal Rust crates are linked via relative paths in the workspace configuration.
- **Zero-Dependency SDKs**: Public-facing SDKs are designed to be drop-in compatible without requiring `pip install`, `npm install`, or Maven/Gradle dependencies, simplifying integration for downstream projects.
---

<!--
来源: knowledge/en/Multi-Ecosystem Dependency Management Strategy/dependency_management.md  |  生成器: Qoder (阿里云 AI IDE)  |  扫描基线: publish2 @ 2026-06-23
-->
