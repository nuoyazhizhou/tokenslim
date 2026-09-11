# vcs_fossil_plugin 设计文档

## 概述
`vcs_fossil_plugin` 是从旧 `vcs_plugin` 完全剥离的 Fossil 专用微插件。零耦合设计，所有辅助函数内联。

## 架构
```
src/plugins/vcs_fossil_plugin/
├── mod.rs       # 模块入口
├── parser.rs    # 类型定义、9 个 Fossil 解析器、全部内联辅助函数
├── methods.rs   # 压缩分发方法、检测函数
└── tests.rs     # 单元测试 + 10 个 case 展示测试
```

## Fossil 命令支持
| Parser | 命令 | DocKind |
|--------|------|---------|
| FossilStatusParser | fossil status | Status |
| FossilDiffParser | fossil diff | Diff |
| FossilLogParser | fossil log | Log |
| FossilChangesParser | fossil changes | Status |
| FossilTimelineParser | fossil timeline | Log |
| FossilUndoParser | fossil undo | Log |
| FossilStashParser | fossil stash | Log |
| FossilMergeParser | fossil merge | Log |
| FossilSyncParser | fossil sync | Log |

## 内联辅助函数
`parse_generic_status_for_tool`、`parse_generic_log_for_tool`、`parse_generic_diff_for_tool`、`parse_simple_status_path`、`looks_like_vcs_path`、`collapse_inline_whitespace` 等全部内联。

## 测试用例（10 个）
case_29_fossil_status ~ case_320_fossil_diff_brief
