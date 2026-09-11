# Init Command 模块设计文档

## 概述

`tokenslim init` 命令用于快速初始化 TokenSlim 项目配置，自动生成适配当前项目的 `.tokenslim.toml` 配置文件，并可选安装 shell hooks。

## 模块结构

```
src/core/init_command/
├── mod.rs      # 模块导出
├── types.rs    # 类型定义
└── methods.rs  # 核心逻辑
```

## 类型定义

### InitOptions

初始化配置选项：
- `install_hooks`: 是否安装 shell hooks
- `hook_shell`: shell 类型（None = 自动探测）
- `dry_run`: 仅打印计划变更
- `force`: 强制覆盖已存在的配置文件

### InitResult

初始化结果：
- `config_created`: 配置文件是否已创建
- `config_path`: 配置文件路径
- `project_type` / `framework` / `package_manager`: 检测到的项目信息
- `hooks_installed`: Shell hooks 是否已安装
- `message`: 状态消息

## 核心逻辑

### 项目检测

复用 `doctor_workspace` 的检测逻辑，识别 25+ 种语言和框架。

### 配置生成

生成 `.tokenslim.toml` 文件，包含：
- `[general]`: 项目类型、框架、包管理器
- `[compression]`: reorder, ai_export, preset
- `[encoding]`: force_utf8
- `[plugins]`: semantic_fallback, plugin_chain
- `[token_optimizer]`: 全局 token 字典优化配置

其中配置职责分层如下：

- `[paths]`（若在 `config/plugins.toml` 中配置）：前置路径压缩参数（如 `min_prefix_length`、`min_occurrences`）
- `[token_optimizer]`：后置字典收益优化参数（如 `min_footer_token_uses`、`enable_nested_aliases`、成本模型）

`init` 模板默认写入：

- `[token_optimizer]`
- `[token_optimizer.scopes.paths]`
- `[token_optimizer.presets.fast|balanced|ai.paths]`

这样新项目初始化后即可按 preset 独立调优路径字典策略，而不影响基础路径提取层。

### Shell Hook 安装

支持 bash/zsh/fish/powershell 四种 shell，生成便捷别名：
- `ts` → `tokenslim`
- `ts-compress` → `tokenslim --mode compress`
- `ts-decompress` → `tokenslim --mode decompress`
- `ts-doctor` → `tokenslim workspace`
- `ts-init` → `tokenslim init`

## CLI 接口

```bash
tokenslim init                          # 生成配置 + 安装 hooks
tokenslim init --no-hooks               # 仅生成配置
tokenslim init --force                  # 强制覆盖
tokenslim init --hook-shell zsh         # 指定 shell
tokenslim init --dry-run                # 预览
```
