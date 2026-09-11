# vcs_cvs_plugin 设计文档

## 概述
`vcs_cvs_plugin` 是从旧 `vcs_plugin` 完全剥离的 CVS 专用微插件。零耦合设计，所有辅助函数内联到 `parser.rs` 中。

## 架构
```
src/plugins/vcs_cvs_plugin/
├── mod.rs       # 模块入口
├── parser.rs    # 类型定义、8 个 CVS 解析器、全部内联辅助函数
├── methods.rs   # 压缩分发方法、检测函数
└── tests.rs     # 单元测试 + 14 个 case 展示测试
```

## CVS 命令支持
| Parser | 命令 | DocKind |
|--------|------|---------|
| CvsStatusParser | cvs status | Status |
| CvsDiffParser | cvs diff | Diff |
| CvsLogParser | cvs log | Log |
| CvsAnnotateParser | cvs annotate | Show |
| CvsUpdateParser | cvs update | Status |
| CvsCommitParser | cvs commit | Log |
| CvsTagParser | cvs tag | Log |
| CvsEditParser | cvs edit | Status |

## 内联辅助函数
所有依赖从 `vcs_plugin/parser/helpers.rs` 复制内联：`parse_generic_status_for_tool`、`parse_generic_log_for_tool`、`parse_generic_diff_for_tool`、`parse_simple_status_path`、`looks_like_vcs_path`、`parse_cvs_checking_in_path`、`parse_cvs_annotate_line` 等 20+ 函数。

## 测试用例清单（14 个）
samples/vcs_cvs_plugin/: case_27_cvs_log ~ case_315_cvs_status_v
