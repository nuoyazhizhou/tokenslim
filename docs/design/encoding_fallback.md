# Encoding Fallback 模块设计文档

## 概述

`encoding_fallback` 模块提供子进程输出的解码回退链，UTF-8 优先 + 本地 codepage 候选，避免 `from_utf8_lossy` 单一路径导致信息损失。

## 模块结构

```
src/core/encoding_fallback/
└── mod.rs      # 完整实现（单文件模块）
```

## 核心函数

### decode_with_fallback(bytes: &[u8]) -> (String, &'static str)

解码回退链：
1. **UTF-8 优先**: 尝试 `from_utf8`，成功则直接返回
2. **Codepage 回退**: 根据 locale 检测候选编码（GBK/Big5/Shift-JIS/EUC-KR）
3. **Lossy 兜底**: 所有回退失败时使用 `from_utf8_lossy`

返回解码后的字符串和使用的编码名称。

### write_utf8(path: &Path, content: &str) -> io::Result<()>

UTF-8 无 BOM 写入函数：
- 确保内容不以 BOM 开头
- 直接写入 UTF-8 字节

### write_utf8_bom(path: &Path, content: &str) -> io::Result<()>

UTF-8 带 BOM 写入函数（仅用于需要 BOM 的遗留工具兼容）。

## 编码策略

所有 TokenSlim 文件写入默认使用 UTF-8 无 BOM。这是为了确保：
- 跨平台一致性（Windows/Linux/macOS）
- 不破坏解析器（BOM 可能导致 JSON/YAML 解析失败）
- 最大化工具兼容性

## 测试

`encoding_fallback` 模块测试覆盖：
- UTF-8 直接解码
- GBK 回退解码
- 无效 UTF-8 回退
- write_utf8 无 BOM 验证
- write_utf8 BOM 剥离
- write_utf8_bom BOM 添加
