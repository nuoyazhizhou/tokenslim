# 插件名称：GitDiffPlugin

## 1. 模块概述
`GitDiffPlugin` 专门针对代码 Review 场景设计。它能识别标准的 Git Diff 格式，并通过裁剪不必要的上下文（Unchanged Context）来压缩 Token 占用，同时保留所有的修改逻辑。

## 2. 数据结构
- `GitDiffPlugin`: 插件核心。
- `SliceType::GitDiffBlock`: 在切片器中对应的类型。

## 3. 功能点清单
### 3.1 代码块识别 (Diff Detection)
- **功能描述**: 通过 `diff --git` 标志位精准锚定代码差异块。
- **调用者**: `TextSlicer` 的 `slice_by_tags` 方法。

### 3.2 上下文压缩 (Context Suppression)
- **功能描述**: 保留 `+` 和 `-` 开头的行，将连续的未修改上下文行合并缩减为 1-3 行，并添加省略标记。
- **目的**: 让 LLM 仅关注变更点，不被海量未重构代码淹没。

### 3.3 路径提取 (File Path Tokenization)
- **功能描述**: 将 `--- a/` 和 `+++ b/` 中的文件路径提取并放入路径字典 (`$P`)。

## 4. 与其它模块的交互
- 与 `RehydrationPipeline` 深度配合，确保压缩后的 Patch 依然可以被还原用于 `git apply`（未来计划）。

## 5. 待办与注意事项
- 目前主要针对标准 Unified Diff 格式。
- 需要支持 Patch 的双向转换。
