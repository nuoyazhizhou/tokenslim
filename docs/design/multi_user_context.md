# 多用户环境下的上下文冲突解决方案 (Multi-User Context Conflict Resolution)

## 1. 问题背景 (The Problem)

TokenSlim 的 `workspace --inject` 功能会自动将当前工作区的上下文（包括操作系统、Shell、本地工具版本、项目框架等）注入到 AI 配置文件（如 `CLAUDE.md`, `.cursorrules`）中。

**痛点（张冠李戴）**：
这些 AI 配置文件通常会被提交到版本控制系统（Git）。如果开发者 A 在 Windows 上运行了 `--inject` 并提交，开发者 B 在 Linux 上拉取了代码。此时，AI 读取到的上下文是 A 的环境（Windows, PowerShell），从而给 B 生成错误的终端命令，导致严重的协作冲突。

## 2. 核心理念：上下文分离 (Context Separation)

为了解决这个问题，我们必须将上下文严格划分为两类：

### 2.1 项目级上下文 (Project Context) - 共享且持久化
这些信息属于代码库本身，对所有开发者都是一致的，**应该**被注入并提交到 Git。
- **Primary Language**: 如 `rust`, `node`, `java`
- **Framework**: 如 `nextjs`, `spring-boot`
- **Package Manager**: 如 `cargo`, `pnpm`
- **Version Dialect**: 如 `rust-2021`, `python-3.8+`, `esm`
- **Database Type**: 如 `postgresql`, `mysql`
- **Build/Test Commands**: 如 `cargo build`, `npm test`

### 2.2 环境级上下文 (Environment Context) - 个人且动态
这些信息属于开发者当前的物理机，因人而异，**绝对不能**被静态写入共享文件。
- **OS**: Windows, macOS, Linux
- **Shell**: bash, zsh, powershell, cmd
- **Encoding**: UTF-8, cp936
- **Local Tool Versions**: `rustc 1.93.1`, `node v24.14.0`
- **Local IDE**: VSCode, IntelliJ

## 3. 解决方案设计 (The Solution)

### 3.1 静态注入层 (Static Injection)
`tokenslim workspace --inject` 命令在修改 `CLAUDE.md` 或 `.cursorrules` 时，**只写入项目级上下文**。

### 3.2 动态指令层 (Dynamic Directive)
为了让 AI 依然能感知到当前开发者的真实环境（OS、Shell、编码），我们在注入的静态文件中添加一条**强指令 (System Directive)**，要求 AI 在会话开始时主动探测环境。

**注入的模板示例**：

```markdown
<!-- tokenslim-context-start -->
# TokenSlim Workspace Context (AUTO-GENERATED)
# DO NOT EDIT MANUALLY - run `tokenslim workspace --inject` to update

## Project Context (Shared)
- Primary: rust
- Package Manager: cargo
- Version Dialect: rust-2021
- Build: cargo build
- Test: cargo test

## ⚠️ AI SYSTEM DIRECTIVE (CRITICAL)
The environment (OS, Shell, Encoding, Tool Versions) varies per developer.
DO NOT assume the environment based on previous conversations.
Before generating any terminal commands or debugging local issues, YOU MUST run:
`tokenslim workspace --format llm`
to get the current developer's real-time local environment.

## Constraints
- Always prefix terminal commands with `tokenslim run`.
- Use the detected project type and version dialect for all code generation.
<!-- tokenslim-context-end -->
```

## 4. 预期收益 (Expected Benefits)

1. **消除 Git 冲突**：团队成员运行 `--inject` 产生的文件内容是完全一致的（只要项目结构没变），不会因为 OS 不同产生 diff。
2. **精准的本地感知**：AI 被强制要求动态运行 `workspace` 诊断，确保它拿到的永远是当前坐在电脑前的开发者的真实环境（Linux 开发者得到 bash 命令，Windows 开发者得到 PowerShell 命令）。
3. **自愈循环**：如果项目依赖发生变化，AI 会根据指令提示用户或自行运行 `--inject` 更新共享上下文。
