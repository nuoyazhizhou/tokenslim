# vcs_bitbucket_plugin 设计文档

## 概述
`vcs_bitbucket_plugin` 是从旧 `vcs_plugin` 剥离的 Bitbucket CLI 专用微插件。Bitbucket CLI 输出结构简单，采用 raw 直通模式。

## 架构
```
src/plugins/vcs_bitbucket_plugin/
├── mod.rs       # 模块入口
├── parser.rs    # 类型定义、4 个 parser struct
├── methods.rs   # raw 直通压缩方法
└── tests.rs     # 4 case 展示测试
```

## 命令支持（raw 直通）
| Parser | 命令 |
|--------|------|
| BitbucketPrListParser | bb pr list |
| BitbucketPrViewParser | bb pr view |
| BitbucketPrCreateParser | bb pr create |
| BitbucketIssueListParser | bb issue list |

## 测试用例（4 个）
samples/vcs_bitbucket_plugin/: case_113 ~ case_208
