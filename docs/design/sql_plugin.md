# 插件名称：SqlPlugin

## 1. 模块概述
数据库与 SQL 脚本压缩插件。旨在保留 SQL 语法骨架的同时，精简重复的 Data Values。

## 2. 功能点清单
### 2.1 批量插入压缩 (Batch Insert Compression)
- **功能描述**: 对于数千行的 `INSERT INTO table VALUES (...), (...);`，自动截断为仅保留前 3 行及总行数统计。

### 2.2 敏感数据脱敏与哈希
- **功能描述**: 自动识别并屏蔽 SQL 中的明文密码、PII 信息。
