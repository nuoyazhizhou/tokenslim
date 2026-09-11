# 嵌入引擎 (Embedding Engine)

## 1. 模块职责
基于 candle 框架实现，集成轻量化 BERT 模型，支持通过语义相似度对切片进行智能指纹匹配，提高特殊日志形式的压缩率。

## 2. 核心数据结构
- `SemanticClassifier`, `EmbeddingModel`, `ModelConfig`

## 3. 核心函数清单
- `init_model()`, `compute_embedding()`, `predict_plugin_type()`

*注：本文档由系统根据源码依赖状态和代码审查要求自动生成（2026-03-25 同步更新），属于生态组件与核心扩展层的标准化占位符。*
