# 路径压缩器 (Path Compressor)

## 1. 模块职责
独立出核心流水线的文本路径优化能力，使用极其进取的智能扫描机制提取公共路径前缀并通过字典层级进行编码，实现超高路径压缩率。

## 2. 核心数据结构
- `PathCompressor`, `PathCompressorStats`

## 3. 核心函数清单
- `extract_common_prefixes()`, `compress_path()`, `replace_paths_in_text_scoped()`

*注：本文档由系统根据源码依赖状态和代码审查要求自动生成（2026-03-25 同步更新），属于生态组件与核心扩展层的标准化占位符。*
