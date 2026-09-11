<!--
本文件由 AI 工具自动迁移自 .qoder/repowiki/, 用于补充 docs/ 的视角。
- 生成器: Qoder (阿里云 AI IDE)
- 生成日期: 2026-06-23
- 原文件: .qoder/repowiki/knowledge/zh/TokenSlim 多平台构建与发布体系/build_system.md
- 适用版本: TokenSlim v0.3.5 (扫描基线: C:\git_work\TokenSlim-publish2 当时快照)
- 维护说明: 内容已与项目 docs/ 现有文件去重; 若代码变更需人工校对。
-->

## 1. 核心构建系统
TokenSlim 采用 **Rust (Cargo)** 作为核心引擎的构建工具，并结合 **Maturin** 实现 Python SDK 的原生绑定。前端扩展（Chrome/VS Code）和 Node.js SDK 则使用 **npm/TypeScript** 生态。

- **Rust Workspace**: 根目录 `Cargo.toml` 定义了主包及 `crates/tokenslim-py` 子包。支持通过 `--features ml` 开启机器学习相关依赖（如 `candle-core`, `tokenizers`）。
- **Python 绑定**: 位于 `crates/tokenslim-py`，使用 `pyproject.toml` 配置 `maturin` 作为构建后端，将 Rust 逻辑编译为 Python 扩展模块。
- **IDE 插件**: JetBrains 插件使用 **Gradle (Kotlin DSL)** 构建；VS Code 和 Chrome 扩展使用标准的 `tsc` 编译流程。

## 2. CI/CD 流水线 (GitHub Actions)
项目建立了高度自动化的发布流水线，主要包含以下阶段：

### 2.1 版本一致性门禁 (`validate-versions`)
在触发 `v*` 标签推送时，首先运行 `scripts/bump-version.mjs --check`，确保所有 `package.json`、`Cargo.toml` 和 `pyproject.toml` 中的版本号与 Git Tag 严格一致，防止发布漂移。

### 2.2 跨平台原生二进制构建 (`build`)
利用 GitHub Hosted Runners 并行构建多平台二进制包：
- **Linux**: x64-gnu, arm64-gnu (ubuntu-22.04 / ubuntu-22.04-arm)
- **macOS**: arm64 (macos-14)；*注：darwin-x64 因 runner 资源问题暂时禁用*
- **Windows**: x64, arm64 (windows-2022 / windows-11-arm)

构建产物通过 `scripts/build-npm-binary-package.mjs` 打包为独立的 npm 平台包（如 `@tokenslim/cli-binary-linux-x64-gnu`），并生成 SHA-256 校验清单。

### 2.3 自动化发布 (`publish-*`)
- **npm 发布**: 依次发布各平台二进制包和纯 JS 的 `tokenslim` SDK 包。使用 `--provenance` 开启供应链签名。
- **PyPI 发布**: 通过 `pypi-publish.yml` 调用 `maturin-action` 构建多平台 Wheel 包，并利用 OIDC 信任发布机制上传至 PyPI。
- **GitHub Release**: 自动创建 Release 并附带所有平台的 `.tgz` 二进制包及校验文件。

## 3. 关键脚本与约定
- **`scripts/bump-version.mjs`**: 唯一的版本管理入口。负责同步更新 7 个 npm 包、3 个 Cargo.toml 以及 Python 配置文件，并自动重新生成 `package-lock.json` 以匹配新的可选依赖版本。
- **`scripts/build-npm-binary-package.mjs`**: 负责将编译好的 Rust 二进制文件和 `config/` 目录（包含插件定义）组装成符合 npm 规范的目录结构。

## 4. 开发者指南
- **本地构建**: 使用 `cargo build --release` 构建核心 CLI。若需开发 Python 绑定，需在 `crates/tokenslim-py` 下运行 `maturin develop`。
- **版本发布**: 严禁手动修改版本号。必须运行 `node scripts/bump-version.mjs <new_version> --commit`，然后推送 Tag 触发 CI 全流程。
- **依赖同步**: 修改 `packages/sdk-nodejs/package.json` 中的 `optionalDependencies` 后，务必运行上述 bump 脚本以更新 lockfile，否则 CI 中的 `npm ci` 会因版本不匹配而失败。
---

<!--
来源: knowledge/zh/TokenSlim 多平台构建与发布体系/build_system.md  |  生成器: Qoder (阿里云 AI IDE)  |  扫描基线: publish2 @ 2026-06-23
-->
