# vcs_p4_plugin 设计文档

## 概述

`vcs_p4_plugin` 是从旧 `vcs_plugin` 中完全剥离的 Perforce (P4/Helix Core) 专用微插件。本插件为**完全自包含**设计，不依赖 `vcs_plugin` 的任何共享代码，所有辅助函数均已内联到 `parser.rs` 中。

## 架构

```
src/plugins/vcs_p4_plugin/
├── mod.rs          # 插件入口，模块声明
├── parser.rs       # 类型定义、解析器、辅助函数（完全自包含）
├── methods.rs      # 压缩分发方法、检测函数
└── tests.rs        # 单元测试 + 展示测试（44 个 case）
```

## P4 专属命令支持列表

| 命令 | 解析器 | DocKind | 说明 |
|------|--------|---------|------|
| `p4 opened` | `P4OpenedParser` | Status | 查看已打开文件（add/edit/delete） |
| `p4 describe` | `P4DescribeParser` | Diff | 查看 changelist 详情与 diff |
| `p4 changes` | `P4ChangesParser` | Log | 查看 changelist 历史 |
| `p4 fstat` | `P4FstatParser` | Show | 文件元数据查询 |
| `p4 where` | `P4WhereParser` | Show | 路径映射（depot→client→local） |
| `p4 info` | `P4InfoParser` | Show | 服务器/客户端连接信息 |
| `p4 labels` | `P4LabelsParser` | Show | 标签列表 |
| `p4 dirs` | `P4DirsParser` | Show | 目录列表（支持公共根目录提取） |
| `p4 sync` | `P4SyncParser` | Status | 文件同步 |
| `p4 submit` | `P4SubmitParser` | Log | 提交 changelist |
| `p4 shelve` | `P4ShelveParser` | Log | 搁置修改 |
| `p4 unshelve` | `P4UnshelveParser` | Log | 取消搁置 |
| `p4 resolve` | `P4ResolveParser` | Status | 冲突解决 |
| `p4 revert` | `P4RevertParser` | Status | 撤销修改 |
| `p4 edit` | `P4EditParser` | Status | 打开编辑 |
| `p4 add` | `P4AddParser` | Status | 添加文件 |
| `p4 delete` | `P4DeleteParser` | Status | 删除文件 |

### 未明确实现专用解析器的命令（走 raw 直通）

以下 P4 命令目前没有专用解析器，在 `compact_p4_other_for_ai` 或 `compact_p4_status_for_ai` 中走 raw 直通（返回原文本）：
- `p4 move`, `p4 copy`, `p4 integrate` — 检测函数会尝试 `is_p4_opened_block` 匹配
- `p4 branches`, `p4 branch` — 走 `compact_p4_other_for_ai` → raw
- `p4 label`, `p4 users`, `p4 workspaces`, `p4 client`, `p4 files`, `p4 print` — raw
- `p4 tag`, `p4 passwd`, `p4 protect`, `p4 triggers`, `p4 depot` — raw
- `p4 diff2`, `p4 diff` — raw
- `p4 filelog` — `compact_p4_log_family_for_ai` 中有轻量压缩（去除命令前缀，压缩 revision 行）

> 注：这些命令的输出量通常较小或结构不规整，走 raw 直通可以避免误压缩。如果有需求可以在后续版本中添加专用解析器。

## 正则表达式优化说明

与旧 `vcs_plugin` 不同，本微插件**不使用正则表达式进行解析**。所有 P4 命令输出均采用**行级字符串前缀匹配**进行解析，原因如下：

1. **P4 输出格式高度规整**：P4 命令输出都有固定的格式约定（如 `... depotFile //path`、`Change NNN on YYYY/MM/DD by user`），不需要正则的灵活性。
2. **性能优势**：字符串前缀匹配比正则快 3-5 倍，且不需要 regex crate 的编译开销。
3. **零外部依赖**：完全自包含，不需要 `lazy_static` 或 `once_cell` 来缓存正则。

旧代码中使用的 `P4_DEPOT_PATH_RE` 正则已被 `parse_p4_depot_path()` 函数的字符串查找逻辑替代。

## 检测函数机制

`methods.rs` 中提供了一系列 `is_p4_*_block()` 检测函数，用于根据文本内容特征自动识别属于哪个 P4 子命令：

| 检测函数 | 检测逻辑 |
|----------|----------|
| `is_p4_opened_block` | 包含 "p4 opened" 或 depot 路径带 `#` revision 标记 |
| `is_p4_describe_block` | 包含 "p4 describe" 或 `Change NNN by user` |
| `is_p4_changes_block` | 包含 "p4 changes" 或 `Change NNN on YYYY/MM/DD by` |
| `is_p4_fstat_block` | 包含 "p4 fstat" 或 `... depotFile` / `... clientFile` |
| `is_p4_where_block` | 包含 "p4 where" 或多 `//` 路径行 |
| `is_p4_info_block` | 包含 "p4 info" 或 `User name:` / `Client name:` 等键值行 |
| `is_p4_labels_block` | 包含 "p4 labels" 或 `Label XXX by user` |
| `is_p4_dirs_block` | 包含 "p4 dirs" 或所有行都以 `//` 开头 |
| `is_p4_sync_block` | 包含 "p4 sync" / "sync completed"，或行尾为 `- updated/added/deleted` |
| `is_p4_submit_block` | 包含 "p4 submit" 或 "submitted" |
| `is_p4_shelve_block` | 包含 "p4 shelve" 或 "shelve change" |
| `is_p4_resolve_block` | 包含 "p4 resolve" 或 `- resolved using` / `- skipped` |
| `is_p4_revert_block` | 包含 "p4 revert" 或 `- reverted` |
| `is_p4_edit_block` | 包含 "p4 edit" 或 `- opened for edit` |
| `is_p4_add_block` | 包含 "p4 add" 或 `- added for add` |
| `is_p4_delete_block` | 包含 "p4 delete" 或 `- deleted for delete` |

## 已知负压缩 Case 记录

以下场景已经过审查，确认压缩后不会出现膨胀（`cost_gate()` 确保 output ≤ input）：

1. **单行 p4 changes 输出**：已通过 `compact_p4_changes_for_ai` → `compact_p4_changes_message_indent` 的分支保护，当 IR 压缩产出的内容不小于原文时，回退到缩进去除模式。
2. **极短 fstat 输出**：`process_parser` → `cost_gate` 确保单文件 fstat 不会因格式转换而膨胀。
3. **单条 p4 dirs 条目**：`maybe_factor_p4_dirs_root` 仅在至少 2 条目录且存在公共前缀时才进行根目录提取。
4. **p4 info 丰富输出**：`compact_p4_info_records` 会丢弃 `Client address`、`Server license`、`+0800 CST` 等冗余字段，始终产生更小的输出。

## 测试用例清单（44 个）

测试日志文件位于 `samples/vcs_p4_plugin/`：

| Case ID | 文件名 | 命令 |
|---------|--------|------|
| case_15 | case_15_p4_opened.log | p4 opened |
| case_16 | case_16_p4_describe.log | p4 describe |
| case_17 | case_17_p4_changes.log | p4 changes |
| case_18 | case_18_p4_fstat.log | p4 fstat |
| case_19 | case_19_p4_where.log | p4 where |
| case_20 | case_20_p4_info.log | p4 info |
| case_21 | case_21_p4_labels.log | p4 labels |
| case_22 | case_22_p4_dirs.log | p4 dirs |
| case_83 | case_83_p4_sync.log | p4 sync |
| case_84 | case_84_p4_submit.log | p4 submit |
| case_85 | case_85_p4_shelve.log | p4 shelve |
| case_86 | case_86_p4_unshelve.log | p4 unshelve |
| case_87 | case_87_p4_resolve.log | p4 resolve |
| case_88 | case_88_p4_revert.log | p4 revert |
| case_89 | case_89_p4_edit.log | p4 edit |
| case_90 | case_90_p4_add.log | p4 add |
| case_91 | case_91_p4_delete.log | p4 delete |
| case_142 | case_142_p4_move.log | p4 move |
| case_143 | case_143_p4_copy.log | p4 copy |
| case_144 | case_144_p4_integrate.log | p4 integrate |
| case_145 | case_145_p4_branches.log | p4 branches |
| case_179 | case_179_p4_branch.log | p4 branch |
| case_180 | case_180_p4_label.log | p4 label |
| case_181 | case_181_p4_users.log | p4 users |
| case_182 | case_182_p4_workspaces.log | p4 workspaces |
| case_183 | case_183_p4_client.log | p4 client |
| case_184 | case_184_p4_files.log | p4 files |
| case_185 | case_185_p4_filelog.log | p4 filelog |
| case_186 | case_186_p4_print.log | p4 print |
| case_211 | case_211_p4_tag.log | p4 tag |
| case_212 | case_212_p4_passwd.log | p4 passwd |
| case_213 | case_213_p4_protect.log | p4 protect |
| case_214 | case_214_p4_triggers.log | p4 triggers |
| case_215 | case_215_p4_depot.log | p4 depot |
| case_216 | case_216_p4_diff2.log | p4 diff2 |
| case_234 | case_234_p4_opened_long.log | p4 opened (long) |
| case_235 | case_235_p4_describe_short.log | p4 describe (short) |
| case_236 | case_236_p4_changes_max.log | p4 changes (max) |
| case_307 | case_307_p4_diff.log | p4 diff |
| case_308 | case_308_p4_changes_l.log | p4 changes -l |
| case_309 | case_309_p4_describe_S.log | p4 describe -S |
| case_310 | case_310_p4_sync_n.log | p4 sync -n |
| case_311 | case_311_p4_diff_dc.log | p4 diff -dc |
| case_312 | case_312_p4_fstat_T.log | p4 fstat -T |

## 依赖关系

本插件**零外部依赖**：
- 不依赖 `vcs_plugin` 的任何模块
- 不依赖 `vcs_git_plugin`、`vcs_hg_plugin`、`vcs_svn_plugin`
- 不使用 `lazy_static`、`once_cell`、`regex`
- 仅依赖 Rust 标准库 `std`（以及可选的 `tracing`）

## 与旧 vcs_plugin 的关系

拆分完成后，旧 `vcs_plugin` 中的 P4 相关代码（`parser/git_svn_hg_p4.rs` 中的 P4 解析器、`methods/p4.rs`、`test/p4.rs`）可以安全删除。但需要注意：

1. 旧 `vcs_plugin` 的 `methods.rs` 通过 `include!("methods/p4.rs")` 和 `include!("methods/core_logic.rs")` 引用了 P4 代码，需要同步移除。
2. 旧 `vcs_plugin` 的 `parser.rs` 和 `parser/git_svn_hg_p4.rs` 中包含 P4 解析器，需要移除。
3. `parser/helpers.rs` 中的纯 P4 辅助函数（`parse_p4_depot_path`、`compact_p4_info_records` 等 20+ 个函数）需要移除。
4. 测试文件 `test/p4.rs` 和 showcase 中的 P4 cases 需要移除。
