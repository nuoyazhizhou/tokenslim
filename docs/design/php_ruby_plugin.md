# 插件名称：PhpRubyPlugin

## 1. 模块概述
处理 PHP 致命错误、警告以及 Ruby/Rails 的堆栈跟踪。

## 2. 功能点清单
### 2.1 PHP Error 识别
- **功能描述**: 正则匹配 `PHP Fatal error:`, `PHP Warning:` 等行，提取错误信息、文件名及行号。

### 2.2 Ruby Stack Trace 压缩
- **功能描述**: 识别 `from ...:in ...` 格式的 Ruby 堆栈，并将重复的路径前缀置入字典。
