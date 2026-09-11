# 全局字典管理器 (Dictionary Manager)

## 1. 模块职责
统一管理并共享在并行和串行路径流中动态分配的字典项 ID (如 $P1, $PK1)，实现与分片 DictionaryEngine 之间的数据同步与持久存储。

## 2. 核心数据结构
- `DictionaryManager`

## 3. 核心函数清单
- `new()`, `allocate_id()`, `merge_dict()`

*注：本文档由系统根据源码依赖状态和代码审查要求自动生成（2026-03-25 同步更新），属于生态组件与核心扩展层的标准化占位符。*
