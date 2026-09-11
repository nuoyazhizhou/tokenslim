# 模块名称：TokenSlim Python Bindings

## 1. 模块概述
该模块位于 `crates/tokenslim-py`，利用 `PyO3` 框架为 Python 环境提供原生的高性能脱水能力。它通过 FFI 调用 Rust 核心引擎，确保 Python 用户也能享受到零拷贝带来的极速压缩。

## 2. 导出函数
### 2.1 `compress(text: str) -> (tokens_json, dict_json)`
- **功能**: 执行完整的压缩流水线。
- **返回**: 一个包含 Token 流和字典的元组。

### 2.2 `decompress(tokens_json, dict_json) -> str`
- **功能**: 执行还原流水线。

## 3. 安装与使用
```python
import tokenslim
compressed, dictionary = tokenslim.compress("high redundant log content...")
original = tokenslim.decompress(compressed, dictionary)
```

## 4. 依赖
- `pyo3`: 负责 Rust 与 Python 类型的映射。
- `serde_json`: 负责数据在不同语言边界的串行化。
