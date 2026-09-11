# Gcc Log Plugin 模块设计

## 职责
专门用于对 `gcc`, `g++`, `make`, `cmake`, `ninja` 等 C/C++ 编译工具链产生的日志进行高比例的脱水压缩和精确还原。
核心策略是将重复的文件路径、宏定义及常见编译词汇提取为字典 token，并将结构化的错误信息（如 `file:line:col: level: msg`）转化为紧凑的标记流。

## MVP 功能点
- [x] 基于正则的 GCC 编译日志格式检测 (`detect`)。
- [x] 解析标准的 `file:line:column: error/warning: message` 结构。
- [x] 解析 `make`/`ninja` 的常见日志格式（如 `[ 50%] Building CXX object...` 或 `make[1]: Entering directory...`）。
- [x] 字典化：提取所有的路径到 DictionaryEngine。
- [x] 还原：基于解析的结构和字典反向重建一模一样的原始日志。

## 数据结构
```rust
struct GccLogPlugin {
    name: &'static str,
    priority: u8,
    // 缓存编译好的正则以提高性能
    log_pattern: regex::Regex,
    make_pattern: regex::Regex,
}
```

## 函数清单
| 函数 | 说明 |
|------|------|
| `new()` | 初始化，编译正则表达式。|
| `detect()` | 探测切片是否匹配 gcc 或 make 日志的典型特征。|
| `compress()` | 使用字典和去重折叠提取关键信息并返回 Token 流。|
| `decompress()` | 根据字典将 Token 恢复为原始文本。|

## 正则策略
* **GCC 报错**: `^(?P<file>[a-zA-Z0-9_/\.\-\+]+):(?P<line>\d+):(?:(?P<col>\d+):)?\s*(?P<level>error|warning|note|fatal error):\s*(?P<msg>.*)$`
* **Make 进出目录**: `^make\[\d+\]: (Entering|Leaving) directory '(?P<dir>[^']+)'$`
* **Ninja/CMake 编译**: `^\[\s*\d+%\] Building [A-Z]+ object (?P<file>[^\s]+)`
