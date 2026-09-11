# Doctor Workspace 模块设计文档

## 概述

`workspace` 是 TokenSlim 的工作区诊断工具，自动探测当前目录的项目类型、框架、包管理器、IDE 信号、版本控制系统和 Git 状态，为 LLM 提供精准的代码生成上下文。

## 模块结构

```
src/core/doctor_workspace/
├── mod.rs      # 模块导出
├── types.rs    # 类型定义
└── methods.rs  # 核心逻辑
```

## 类型定义

### WorkspaceRiskLevel

风险等级枚举：`Ok` | `Warn` | `Fail`

### ProjectInfo

项目信息结构：
- `primary`: 主语言（rust/java/node/python/cpp/csharp/go/ruby/php/dart/swift/kotlin/scala/elixir/haskell/lua/zig/r/matlab/perl/shell/c/sql/embedded/arduino）
- `secondary`: 次要语言列表
- `framework`: 框架（nextjs/vue/nuxt/svelte/angular/react-native/electron/tauri/flutter/django/fastapi/flask/spring-boot/laravel/rails/express/astro/remix/solid/vite/turbo/nx/expo/android/ios-macos）
- `package_manager`: 包管理器（npm/pnpm/yarn/bun）
- `build` / `test`: 构建/测试命令
- `dialect`: 版本方言（python-3.8+/c++17/rust-2021/go-1.21/spring-boot-3/esm/cjs 等）
- `database`: 数据库类型（通过 ORM/迁移文件推断：postgresql/mysql/sqlite/oracle/sqlserver/db2/mongodb）
- `module_system`: 模块系统（esm/cjs）

### ToolVersions

工具版本结构：rust, node, python, java, gcc, clang, deno, msvc, ninja, bazel

### IdeInfo

IDE 检测结构：vscode, idea, visual_studio, xcode, cursor, neovim, eclipse, sublime, android_studio

### RepoInfo

版本控制结构：git, git_branch, git_dirty, svn, hg, p4

## 检测逻辑

### 项目类型检测

通过文件特征判定：
- `Cargo.toml` → rust
- `pom.xml` / `build.gradle` → java
- `package.json` → node
- `pyproject.toml` / `requirements.txt` → python
- 等 25+ 种语言

### 版本方言检测 (`detect_dialect`)

- **Python**: `pyproject.toml` requires-python, `.python-version`, 运行时版本
- **C/C++**: `CMakeLists.txt` CMAKE_CXX_STANDARD, `Makefile` -std 标志
- **Rust**: `Cargo.toml` edition
- **Go**: `go.mod` go 指令
- **Node.js**: `package.json` engines.node, `.nvmrc`, type 字段

### 数据库推断 (`detect_database`)

通过 ORM/迁移文件推断：
- Prisma schema → provider 字段
- Flyway conf → jdbc URL
- Alembic ini → sqlalchemy URL
- Knexfile → client 字段
- Django settings → ENGINE 字段
- Liquibase properties → jdbc URL

### 模块系统检测 (`detect_module_system`)

- `package.json` `"type": "module"` → esm
- `.mjs` / `.cjs` 文件存在性

## 输出格式

- **Text**: 完整报告
- **JSON**: 结构化数据
- **LLM**: 紧凑 JSON（9 字段 + repo 扩展）
- **JSON-Min**: 去掉 null/false 默认值

## CLI 接口

```bash
tokenslim workspace                            # 默认 text
tokenslim workspace --format json              # JSON
tokenslim workspace --format llm               # LLM 紧凑格式
tokenslim workspace --strict                   # 严格模式
tokenslim workspace --inject                   # 生成 .tokenslim-context.md
```
