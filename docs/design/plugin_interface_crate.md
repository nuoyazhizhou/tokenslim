# 插件稳定接口协议库 (plugin-interface Crate)

## 1. 模块职责
剥离出来的一个超强稳定 ABI 接口和 API 抽象库，使得未来的各类动态插件可以用统一的方法与 TokenSlim 底层数据结构无缝打通。

## 2. 核心数据结构
- `PluginApi`, `StableToken`, `CAbiString`

## 3. 核心函数清单
- `register_plugin()`, `call_compress()`

*注：本文档由系统根据源码依赖状态和代码审查要求自动生成（2026-03-25 同步更新），属于生态组件与核心扩展层的标准化占位符。*
