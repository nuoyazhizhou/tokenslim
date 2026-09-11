# SVN Plugin Design Document

## Objective
对单体式 `vcs_plugin` 内过载的 Subversion (SVN) 解析与合并逻辑进行微架构拆分，建立高度自包含的独立微插件 `vcs_svn_plugin`，彻底清除与核心模块或其他通用解析库的耦合。

## Architecture
- **Zero Coupling**: 本插件不再依赖外部的通用状态处理（如旧有的 `VcsRuleEngine` 等），通过本地定义专有的 `VcsRecord` 实体来实现完全的代码物理隔离。
- **`mod.rs`**: 负责向外暴露 `methods` 接口供主网关转发调用。
- **`parser.rs`**: 包含了所有依赖于正则表达式与字符串拆分的类，涵盖 17 个专门对应具体 SVN 子命令的实现（`SvnStatusParser`, `SvnDiffParser`等），同时**完全接管内联了所有的 `helpers.rs` 辅助方法**。
- **`methods.rs`**: 提供针对大模型压缩场景下的专用精简方案路由，如 `compact_svn_diff_for_ai()`。
- **`showcase.rs` & `tests.rs`**: 作为测试与防腐层，指向新的测试依赖位置 `samples/vcs_svn_plugin/`。

## Handled SVM Commands
当前解耦设计可处理如下命令子集产生的数据流压缩：
* `status`, `diff`, `log`, `blame`, `list`, `propget`/`proplist`, `info`, `update`, `switch`, `relocate`, `merge`, `lock`/`unlock`, `revert`, `cleanup`, `resolve`, `export`.
