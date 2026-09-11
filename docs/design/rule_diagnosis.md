# Rule Diagnosis 模块设计文档

## 概述

`rule_diagnosis` 模块用于分析 TOML 静态规则文件，检测命中率、冲突规则、空规则和无效正则，帮助维护规则文件的质量。

## 模块结构

```
src/core/rule_diagnosis/
└── mod.rs      # 完整实现（单文件模块）
```

## 类型定义

### RuleDiagnosis

诊断结果：
- `file`: 规则文件路径
- `total_sections` / `valid_sections`: 规则段统计
- `empty_sections`: 空规则段列表
- `invalid_regex`: 无效正则列表
- `conflicts`: 冲突列表
- `hit_rate`: 命中率统计

### RegexError

无效正则错误：section, field, pattern, error

### Conflict

冲突信息：
- `section_a` / `section_b`: 冲突的规则段
- `kind`: 冲突类型（DuplicateEnter / KeepDropOverlap / KeepOverlap）
- `pattern`: 冲突的模式

### HitRate

命中率统计：
- `total_patterns` / `valid_patterns` / `empty_patterns`
- `sections_with_aggregates` / `sections_without_enter`

## 检测逻辑

### 无效正则检测

对每个规则的 enter/keep/drop/aggregate pattern 尝试编译正则，失败则记录错误。

### 空规则检测

检测没有任何可用模式（enter 为空且无 keep/drop/aggregates）的规则段。

### 冲突检测

- **DuplicateEnter**: 两个规则段有相同的 enter 模式
- **KeepDropOverlap**: 一个规则段的 keep 模式与另一个的 drop 模式相同
- **KeepOverlap**: 两个规则段有相同的 keep 模式

### 命中率计算

`valid_patterns / total_patterns * 100%`

## 输出格式

- **Text**: 人类可读报告，包含 Sections/Invalid Regex/Conflicts/Hit Rate
- **JSON**: 结构化数据，便于程序消费

## CLI 接口

```bash
tokenslim rule                           # 默认 text
tokenslim rule --format json             # JSON
```

自动搜索 `config/plugins.toml`, `rules.toml`, `.tokenslim.toml` 中的规则配置。
