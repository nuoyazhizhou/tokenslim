# vcs_bzr_plugin 设计文档

## 概述
`vcs_bzr_plugin` 是从旧 `vcs_plugin` 完全剥离的 Bazaar (Bzr) 专用微插件。零耦合设计，所有辅助函数内联。

## 架构
```
src/plugins/vcs_bzr_plugin/
├── mod.rs       # 模块入口
├── parser.rs    # 类型定义、8 个 Bzr 解析器、全部内联辅助函数
├── methods.rs   # 压缩分发方法、检测函数
└── tests.rs     # 单元测试 + 13 个 case 展示测试
```

## Bzr 命令支持
| Parser | 命令 | DocKind |
|--------|------|---------|
| BzrStatusParser | bzr status | Status |
| BzrDiffParser | bzr diff | Diff |
| BzrLogParser | bzr log | Log |
| BzrPullParser | bzr pull | Log |
| BzrPushParser | bzr push | Log |
| BzrMergeParser | bzr merge | Log |
| BzrResolveParser | bzr resolve | Status |
| BzrBranchParser | bzr branch | Log |

## 内联辅助函数
`parse_generic_status_for_tool`、`parse_generic_log_for_tool`、`parse_generic_diff_for_tool`、`parse_simple_status_path`、`looks_like_vcs_path`、`parse_bzr_revision_count_line`、`parse_bzr_total_revisions_line` 等全部内联。

## 测试用例（13 个）
case_28_bzr_diff ~ case_319_bzr_status_short
