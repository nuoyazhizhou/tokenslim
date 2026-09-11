# 路径分析器 (Path Analyzer)

## 1. 模块职责
负责解析、验证和处理底层的文件及目录路径树，为 `PathCompressor` 提供前置的扫描分析能力。

## 2. 核心数据结构
- `PathAnalyzer`, `PathTreeContext`

## 3. 核心函数清单
- `analyze_directory()`, `filter_valid_paths()`

*注：本文档由系统根据源码依赖状态和代码审查要求自动生成（2026-03-25 同步更新）。*
