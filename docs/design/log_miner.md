# 工具名称：Log Miner

## 1. 概述
`log_miner` 是一个独立的二进制工具（`src/bin/log_miner.rs`），它为 TokenSlim 提供智能规则挖掘能力。它基于 Drain 算法，在无需预定义规则的情况下，自动将海量原始日志分类为不同的“模式模板”。

## 2. 核心算法：Drain
- **固定深度搜索树**: 使用指定的树深度来过滤和匹配日志。
- **相似度计算**: 统计对令牌的命中率，动态决定是创建新簇（Cluster）还是归并到现有模板。
- **模板提取**: 自动将不同行中变化的部分替换为 `<*>`。

## 3. 功能点
- **配置文件导出**: 自动生成符合 `TemplateDrivenPlugin` 格式的 JSON 规则文件。
- **AI Prompt 生成**: 生成专门设计的 Prompt，引导大模型对挖掘出的原始模板进行深层语义优化。

## 4. 调用方式
```bash
cargo run --bin log_miner -- -i <INPUT_FILE> -o <OUTPUT_JSON>
```
