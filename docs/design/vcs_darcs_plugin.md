# vcs_darcs_plugin 设计文档

## 概述
`vcs_darcs_plugin` 是从旧 `vcs_plugin` 完全剥离的 Darcs 专用微插件。零耦合设计，所有辅助函数内联。

## 架构
```
src/plugins/vcs_darcs_plugin/
├── mod.rs       # 模块入口
├── parser.rs    # 类型定义、7 个 Darcs 解析器、全部内联辅助函数
├── methods.rs   # 压缩分发方法、检测函数
└── tests.rs     # 单元测试 + 10 个 case 展示测试
```

## Darcs 命令支持
| Parser | 命令 | DocKind |
|--------|------|---------|
| DarcsStatusParser | darcs status | Status |
| DarcsDiffParser | darcs diff | Diff |
| DarcsLogParser | darcs log | Log |
| DarcsRecordParser | darcs record | Log |
| DarcsAmendParser | darcs amend | Log |
| DarcsObliterateParser | darcs obliterate | Log |
| DarcsWhatsnewParser | darcs whatsnew | Status |

## 内联辅助函数
`parse_generic_status_for_tool`、`parse_generic_log_for_tool`、`parse_generic_diff_for_tool`、`parse_simple_status_path`、`looks_like_vcs_path`、`split_first_token`、`parse_darcs_hunk_record` 等全部内联。

## 测试用例（10 个）
case_35_darcs_log ~ case_322_darcs_whatsnew_s
