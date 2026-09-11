# vcs_hg_plugin 设计文档

## 概述

`vcs_hg_plugin` 是从旧 `vcs_plugin` 中剥离的 Mercurial (Hg) 专用微插件。本插件为**零耦合**设计，所有辅助函数均已内联，不依赖 `vcs_plugin` 的共享代码。

## 架构

```
src/plugins/vcs_hg_plugin/
├── mod.rs          # 模块入口，声明子模块并 re-export 公共 API
├── parser.rs       # 类型定义、20 个 Hg 解析器、全部内联辅助函数
├── methods.rs      # AI 压缩分发方法、检测函数、路由逻辑
├── tests.rs        # 35+ 单元测试
└── showcase.rs     # 展示报告生成器（压缩比测量）
```

## 数据模型

```rust
pub enum VcsTool { Hg }
pub enum VcsDocKind { Status, Log, Diff, Show }
pub enum VcsRecord {
    Section(String), Branch(String),
    File { status: Option<char>, path: String },
    LabeledFile { label: String, path: String },
    DiffFile { left: String, right: String },
    Subject(String), Author(String), Date(String),
    Commit(String), Stat(String), Hunk(String),
    Patch(String), Raw(String),
}
pub struct VcsDocument { pub tool: VcsTool, pub kind: VcsDocKind, pub records: Vec<VcsRecord> }
pub trait VcsParser { fn parse(&self, raw: &str) -> Option<VcsDocument>; }
```

## Hg 专属命令支持列表

### 1. Core Hg Commands（20 个专用解析器）

| # | Parser | 命令 | DocKind | 说明 |
|---|--------|------|---------|------|
| 1 | `HgStatusParser` | hg status | Status | 工作区状态 |
| 2 | `HgDiffParser` | hg diff | Diff | 差异比较 |
| 3 | `HgLogParser` | hg log | Log | 提交日志 |
| 4 | `HgHeadsParser` | hg heads | Log | 分支头指针 |
| 5 | `HgOutgoingParser` | hg outgoing | Log | 待推送变更集 |
| 6 | `HgIncomingParser` | hg incoming | Log | 待拉取变更集 |
| 7 | `HgParentsParser` | hg parents | Log | 父变更集 |
| 8 | `HgCloneParser` | hg clone | Log | 克隆仓库 |
| 9 | `HgPullParser` | hg pull | Log | 拉取变更 |
| 10 | `HgPushParser` | hg push | Log | 推送变更 |
| 11 | `HgUpdateParser` | hg update | Status | 更新工作区 |
| 12 | `HgCommitParser` | hg commit | Log | 提交变更 |
| 13 | `HgBranchesParser` | hg branches | Log | 分支列表 |
| 14 | `HgMergeParser` | hg merge | Log | 合并分支 |
| 15 | `HgRollbackParser` | hg rollback | Log | 回滚操作 |
| 16 | `HgBackoutParser` | hg backout | Log | 撤销提交 |
| 17 | `HgShelveParser` | hg shelve | Log | 搁置修改 |
| 18 | `HgPhaseParser` | hg phase | Log | 变更集阶段 |
| 19 | `HgBookmarksParser` | hg bookmarks | Log | 书签列表 |
| 20 | `HgTagParser` | hg tag | Log | 标签操作 |

### 2. 无专用解析器命令（走 raw 直通）

以下 Hg 命令没有专用解析器，在 `compact_hg_other_for_ai` 中走 raw 直通：

| 命令 | 说明 |
|------|------|
| hg copy | 文件复制 |
| hg move | 文件移动 |
| hg purge | 清理未跟踪文件 |
| hg archive | 打包导出 |
| hg verify | 仓库校验 |
| hg identify | 仓库标识 |
| hg paths | 远程路径 |
| hg config | 配置信息 |
| hg summarize | 摘要统计 |
| hg transplant | 变更集迁移 |

## 内联辅助函数

与 P4 微插件一样，本插件将所有依赖的辅助函数内联到 `parser.rs` 中：

| 函数 | 来源 | 说明 |
|------|------|------|
| `looks_like_vcs_path` | 旧 helpers.rs | VCS 路径识别 |
| `parse_simple_status_path` | 旧 helpers.rs | 状态行解析（如 "M path"） |
| `parse_generic_patch_or_stat_line` | 旧 helpers.rs | 通用 diff 行解析 |
| `to_doc_if_any` | 旧 helpers.rs | 条件文档构造 |
| `collapse_inline_whitespace` | 旧 helpers.rs | 行内空白压缩 |
| `compact_hg_date_value` | 旧 helpers.rs | Hg 日期格式压缩 |
| `enforce_hg_changeset_boundaries` | 旧 helpers.rs | 变更集边界规范化 |
| `parse_hg_changeset_like_records` | 旧 helpers.rs | 变更集类记录解析 |
| `compact_hg_update_summary_line` | 旧 helpers.rs | 更新摘要压缩 |
| `strip_hg_field_prefix` | 旧 helpers.rs | Hg 字段前缀提取 |
| `compact_hg_branch_line` | 旧 helpers.rs | 分支行压缩 |
| `split_on_repeated_whitespace` | 旧 helpers.rs | 多空白分隔 |

## 检测函数机制

`methods.rs` 提供了一系列 `is_hg_*_block()` 检测函数，用于根据文本内容特征自动识别 Hg 子命令类型：

| 检测函数 | 检测逻辑 |
|----------|----------|
| `is_hg_status_block` | 包含 "hg status" / "hg st" |
| `is_hg_diff_block` | 包含 "hg diff" |
| `is_hg_log_block` | 包含 "hg log" / changeset 格式 |
| `is_hg_heads_block` | 包含 "hg heads" / 分支头指针格式 |
| `is_hg_outgoing_block` | 包含 "hg outgoing" |
| `is_hg_incoming_block` | 包含 "hg incoming" |
| `is_hg_parents_block` | 包含 "hg parents" |
| `is_hg_merge_like_block` | 包含合并特征行 |
| `is_hg_rollback_like_block` | 包含回滚特征行 |
| `is_hg_backout_like_block` | 包含撤销特征行 |
| `is_hg_shelve_like_block` | 包含搁置特征行 |
| `is_hg_phase_like_block` | 包含阶段标记行 |
| `is_hg_bookmarks_like_block` | 包含书签标记行 |
| `is_hg_tag_like_block` | 包含标签标记行 |

## 测试覆盖

- **总用例数**: 46（含扩展变体）
- **专用解析器**: 20
- **单元测试**: 35+（全部通过）
- **展示报告**: `target/vcs_hg_compact_showcase_report.txt`

## 已知负压缩 Case 记录

以下场景已经过审查：

1. **单行 hg status 输出**：极短输出在 `process_parser` → `cost_gate` 保护下不会膨胀
2. **无变更的 pull/push**：`HgPullParser` / `HgPushParser` 在无变更时直接返回 "no changes"
3. **纯分支列表**：`HgBranchesParser` 对 inactive 分支做 `~` 标记压缩

## 依赖关系

本插件为**零外部依赖**设计：
- 不依赖 `vcs_plugin` 的任何模块
- 不依赖 `vcs_git_plugin`、`vcs_svn_plugin`、`vcs_p4_plugin`
- 不使用 `lazy_static`、`once_cell`、`regex`
- 仅依赖 Rust 标准库 `std`

## 运行测试

```bash
# 运行所有 Hg 插件测试
cargo test vcs_hg_plugin

# 运行展示报告（输出压缩比）
cargo test vcs_hg_plugin::showcase -- --nocapture
```
