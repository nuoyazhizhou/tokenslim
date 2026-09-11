# Core 核心层分计划

> **父计划**: [CODE_COMMENT_PLAN.md](./CODE_COMMENT_PLAN.md)  
> **执行前必须读取**: CODE_COMMENT_PLAN.md + 本文件  
> **范围**: `src/core/` 目录  
> **依赖**: utils 层  
> **状态**: 进行中（部分文件已注释，文件级状态以本表勾选为准）

---

## 一、 本层概述

Core 层是 TokenSlim 的核心业务逻辑层，包含压缩流水线、内容分析、去重引擎、字典引擎、插件调度器等核心组件。

**执行顺序（按依赖关系，自底向上）**:

```
第 1 组：基础类型与工具
  const.rs → utils/ → tracking/ → error_isolation/ → safety_check/

第 2 组：数据处理引擎
  encoding_fallback/ → stream_reader/ → text_slicer/ → content_analyzer/
  → json_extractor/ → timestamp_converter/ → tree_restructure/

第 3 组：核心引擎
  dictionary_engine/ → dictionary_manager/ → dedup_engine/
  → metrics/ → log_reorderer/ → path_analyzer/ → path_compressor/
  → path_optimizer/

第 4 组：插件与流水线
  filter_discover/ → filter_variants/ → plugin_dispatcher/
  → compression/ → compression_pipeline/ → rehydration_pipeline/
  → rewrite/ → template_render/

第 5 组：功能模块
  init_command/ → doctor_encoding/ → doctor_workspace/
  → rule_diagnosis/ → dynamic_plugin_loader/ → plugin_config_loader/
  → sys_env/

第 6 组：顶层整合
  compression_context.rs → config_manager.rs → observability.rs
  → tracing_init.rs → mod.rs
```

---

## 二、 文件清单与任务状态

### 2.1 第 1 组：基础类型与工具

| 序号 | 文件路径 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|---------|-----------|------|---------|------|
| 1 | `src/core/const.rs` | ~10 | 待开始 | - | 核心层常量 |
| 2 | `src/core/utils/mod.rs` | ~5 | 待开始 | - | 核心工具模块导出 |
| 3 | `src/core/utils/json.rs` | ~10 | 待开始 | - | JSON 工具 |
| 4 | `src/core/utils/roi.rs` | ~5 | 待开始 | - | ROI 计算 |
| 5 | `src/core/tracking/types.rs` | ~10 | 待开始 | - | 追踪类型 |
| 6 | `src/core/tracking/gain.rs` | ~5 | 待开始 | - | 增益计算 |
| 7 | `src/core/tracking/tracker.rs` | ~10 | 待开始 | - | 追踪器 |
| 8 | `src/core/tracking/mod.rs` | ~5 | 待开始 | - | 追踪模块导出 |
| 9 | `src/core/error_isolation/types.rs` | ~10 | 待开始 | - | 错误隔离类型 |
| 10 | `src/core/error_isolation/methods.rs` | ~10 | 待开始 | - | 错误隔离实现 |
| 11 | `src/core/error_isolation/mod.rs` | ~5 | 待开始 | - | 错误隔离模块导出 |
| 12 | `src/core/error_isolation/test.rs` | ~5 | 待开始 | - | 错误隔离测试 |
| 13 | `src/core/safety_check/mod.rs` | ~5 | 待开始 | - | 安全检查模块导出 |
| 14 | `src/core/safety_check/hidden_unicode.rs` | ~5 | 待开始 | - | 隐藏 Unicode 检测 |
| 15 | `src/core/safety_check/prompt_injection.rs` | ~5 | 待开始 | - | Prompt 注入检测 |
| 16 | `src/core/safety_check/shell_injection.rs` | ~5 | 待开始 | - | Shell 注入检测 |

### 2.2 第 2 组：数据处理引擎

| 序号 | 文件路径 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|---------|-----------|------|---------|------|
| 17 | `src/core/encoding_fallback/mod.rs` | ~10 | 待开始 | - | 编码回退 |
| 18 | `src/core/stream_reader/types.rs` | ~10 | 待开始 | - | 流读取类型 |
| 19 | `src/core/stream_reader/methods.rs` | ~20 | 待开始 | - | 流读取实现 |
| 20 | `src/core/stream_reader/mod.rs` | ~5 | 待开始 | - | 流读取模块导出 |
| 21 | `src/core/stream_reader/test.rs` | ~10 | 待开始 | - | 流读取测试 |
| 22 | `src/core/text_slicer/types.rs` | ~15 | 待开始 | - | 文本切片类型 |
| 23 | `src/core/text_slicer/methods.rs` | ~30 | 待开始 | - | 文本切片实现（大文件） |
| 24 | `src/core/text_slicer/config_loader.rs` | ~10 | 待开始 | - | 配置加载 |
| 25 | `src/core/text_slicer/mod.rs` | ~5 | 待开始 | - | 文本切片模块导出 |
| 26 | `src/core/text_slicer/test.rs` | ~15 | 待开始 | - | 文本切片测试 |
| 27 | `src/core/content_analyzer/types.rs` | ~15 | 待开始 | - | 内容分析类型 |
| 28 | `src/core/content_analyzer/methods.rs` | ~20 | 待开始 | - | 内容分析实现 |
| 29 | `src/core/content_analyzer/mod.rs` | ~5 | 待开始 | - | 内容分析模块导出 |
| 30 | `src/core/content_analyzer/test.rs` | ~10 | 待开始 | - | 内容分析测试 |
| 31 | `src/core/content_analyzer/drain/types.rs` | ~10 | 待开始 | - | Drain 类型 |
| 32 | `src/core/content_analyzer/drain/methods.rs` | ~15 | 待开始 | - | Drain 实现 |
| 33 | `src/core/content_analyzer/drain/mod.rs` | ~5 | 待开始 | - | Drain 模块导出 |
| 34 | `src/core/content_analyzer/drain/test.rs` | ~10 | 待开始 | - | Drain 测试 |
| 35 | `src/core/json_extractor/mod.rs` | ~10 | 待开始 | - | JSON 提取器 |
| 36 | `src/core/timestamp_converter/types.rs` | ~5 | 待开始 | - | 时间戳转换类型 |
| 37 | `src/core/timestamp_converter/methods.rs` | ~10 | 待开始 | - | 时间戳转换实现 |
| 38 | `src/core/timestamp_converter/mod.rs` | ~5 | 待开始 | - | 时间戳转换模块导出 |
| 39 | `src/core/tree_restructure/config.rs` | ~5 | 待开始 | - | 树重构配置 |
| 40 | `src/core/tree_restructure/trie.rs` | ~10 | 待开始 | - | Trie 树 |
| 41 | `src/core/tree_restructure/render.rs` | ~10 | 待开始 | - | 渲染器 |
| 42 | `src/core/tree_restructure/mod.rs` | ~5 | 待开始 | - | 树重构模块导出 |

### 2.3 第 3 组：核心引擎

| 序号 | 文件路径 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|---------|-----------|------|---------|------|
| 43 | `src/core/dictionary_engine/types.rs` | ~15 | 待开始 | - | 字典引擎类型 |
| 44 | `src/core/dictionary_engine/methods.rs` | ~20 | 待开始 | - | 字典引擎实现 |
| 45 | `src/core/dictionary_engine/mod.rs` | ~5 | 待开始 | - | 字典引擎模块导出 |
| 46 | `src/core/dictionary_engine/test.rs` | ~10 | 待开始 | - | 字典引擎测试 |
| 47 | `src/core/dictionary_manager/mod.rs` | ~10 | 待开始 | - | 字典管理器 |
| 48 | `src/core/dictionary_manager/methods.rs` | ~15 | 待开始 | - | 字典管理器实现 |
| 49 | `src/core/dedup_engine/types.rs` | ~10 | 待开始 | - | 去重引擎类型 |
| 50 | `src/core/dedup_engine/methods.rs` | ~15 | 待开始 | - | 去重引擎实现 |
| 51 | `src/core/dedup_engine/mod.rs` | ~5 | 待开始 | - | 去重引擎模块导出 |
| 52 | `src/core/dedup_engine/test.rs` | ~10 | 待开始 | - | 去重引擎测试 |
| 53 | `src/core/metrics/types.rs` | ~15 | 待开始 | - | 指标类型 |
| 54 | `src/core/metrics/methods.rs` | ~20 | 待开始 | - | 指标实现 |
| 55 | `src/core/metrics/mod.rs` | ~5 | 待开始 | - | 指标模块导出 |
| 56 | `src/core/metrics/test.rs` | ~10 | 待开始 | - | 指标测试 |
| 57 | `src/core/log_reorderer/types.rs` | ~10 | 待开始 | - | 日志重排序类型 |
| 58 | `src/core/log_reorderer/methods.rs` | ~15 | 待开始 | - | 日志重排序实现 |
| 59 | `src/core/log_reorderer/mod.rs` | ~5 | 待开始 | - | 日志重排序模块导出 |
| 60 | `src/core/path_analyzer/mod.rs` | ~5 | 待开始 | - | 路径分析器模块导出 |
| 61 | `src/core/path_analyzer/methods.rs` | ~20 | 待开始 | - | 路径分析器实现 |
| 62 | `src/core/path_analyzer/optimized_methods.rs` | ~10 | 待开始 | - | 优化方法 |
| 63 | `src/core/path_analyzer/original_methods.rs` | ~10 | 待开始 | - | 原始方法 |
| 64 | `src/core/path_compressor/types.rs` | ~10 | 待开始 | - | 路径压缩类型 |
| 65 | `src/core/path_compressor/methods.rs` | ~15 | 待开始 | - | 路径压缩实现 |
| 66 | `src/core/path_compressor/mod.rs` | ~5 | 待开始 | - | 路径压缩模块导出 |
| 67 | `src/core/path_optimizer/methods.rs` | ~15 | 待开始 | - | 路径优化实现 |
| 68 | `src/core/path_optimizer/mod.rs` | ~5 | 待开始 | - | 路径优化模块导出 |
| 69 | `src/core/path_optimizer/token_boundary.rs` | ~5 | 待开始 | - | Token 边界 |

### 2.4 第 4 组：插件与流水线

| 序号 | 文件路径 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|---------|-----------|------|---------|------|
| 70 | `src/core/filter_discover/types.rs` | ~10 | 待开始 | - | 过滤器发现类型 |
| 71 | `src/core/filter_discover/parser.rs` | ~10 | 待开始 | - | 配置解析器 |
| 72 | `src/core/filter_discover/classifier.rs` | ~10 | 待开始 | - | 分类器 |
| 73 | `src/core/filter_discover/aggregator.rs` | ~10 | 待开始 | - | 聚合器 |
| 74 | `src/core/filter_discover/mod.rs` | ~5 | 待开始 | - | 过滤器发现模块导出 |
| 75 | `src/core/filter_variants/types.rs` | ~10 | 待开始 | - | 过滤器变体类型 |
| 76 | `src/core/filter_variants/detector.rs` | ~10 | 待开始 | - | 检测器 |
| 77 | `src/core/filter_variants/router.rs` | ~10 | 待开始 | - | 路由器 |
| 78 | `src/core/filter_variants/mod.rs` | ~5 | 待开始 | - | 过滤器变体模块导出 |
| 79 | `src/core/plugin_dispatcher/types.rs` | ~15 | 待开始 | - | 插件调度器类型 |
| 80 | `src/core/plugin_dispatcher/methods.rs` | ~20 | 待开始 | - | 插件调度器实现 |
| 81 | `src/core/plugin_dispatcher/mod.rs` | ~5 | 待开始 | - | 插件调度器模块导出 |
| 82 | `src/core/plugin_dispatcher/test.rs` | ~10 | 待开始 | - | 插件调度器测试 |
| 83 | `src/core/compression/types.rs` | ~10 | 待开始 | - | 压缩类型 |
| 84 | `src/core/compression/mod.rs` | ~5 | 待开始 | - | 压缩模块导出 |
| 85 | `src/core/compression/test.rs` | ~10 | 待开始 | - | 压缩测试 |
| 86 | `src/core/compression_pipeline/types.rs` | ~15 | 待开始 | - | 压缩流水线类型 |
| 87 | `src/core/compression_pipeline/methods.rs` | ~25 | 待开始 | - | 压缩流水线实现 |
| 88 | `src/core/compression_pipeline/mod.rs` | ~5 | 待开始 | - | 压缩流水线模块导出 |
| 89 | `src/core/compression_pipeline/test.rs` | ~10 | 待开始 | - | 压缩流水线测试 |
| 90 | `src/core/rehydration_pipeline/types.rs` | ~10 | 待开始 | - | 复水流线型 |
| 91 | `src/core/rehydration_pipeline/methods.rs` | ~15 | 待开始 | - | 复水流水线实现 |
| 92 | `src/core/rehydration_pipeline/mod.rs` | ~5 | 待开始 | - | 复水流水线模块导出 |
| 93 | `src/core/rehydration_pipeline/test.rs` | ~10 | 待开始 | - | 复水流水线测试 |
| 94 | `src/core/rewrite/rules.rs` | ~15 | 待开始 | - | 重写规则 |
| 95 | `src/core/rewrite/transparent.rs` | ~10 | 待开始 | - | 透明重写 |
| 96 | `src/core/rewrite/bash_ast.rs` | ~15 | 待开始 | - | Bash AST |
| 97 | `src/core/rewrite/user_config.rs` | ~10 | 待开始 | - | 用户配置 |
| 98 | `src/core/rewrite/mod.rs` | ~5 | 待开始 | - | 重写模块导出 |
| 99 | `src/core/template_render/types.rs` | ~10 | 待开始 | - | 模板渲染类型 |
| 100 | `src/core/template_render/parser.rs` | ~10 | 待开始 | - | 模板解析器 |
| 101 | `src/core/template_render/renderer.rs` | ~10 | 待开始 | - | 渲染器 |
| 102 | `src/core/template_render/mod.rs` | ~5 | 待开始 | - | 模板渲染模块导出 |

### 2.5 第 5 组：功能模块

| 序号 | 文件路径 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|---------|-----------|------|---------|------|
| 103 | `src/core/init_command/types.rs` | ~10 | 待开始 | - | init 命令类型 |
| 104 | `src/core/init_command/methods.rs` | ~15 | 待开始 | - | init 命令实现 |
| 105 | `src/core/init_command/mod.rs` | ~5 | 待开始 | - | init 命令模块导出 |
| 106 | `src/core/doctor_encoding/types.rs` | ~10 | 待开始 | - | 编码诊断类型 |
| 107 | `src/core/doctor_encoding/methods.rs` | ~15 | 待开始 | - | 编码诊断实现 |
| 108 | `src/core/doctor_encoding/mod.rs` | ~5 | 待开始 | - | 编码诊断模块导出 |
| 109 | `src/core/doctor_workspace/types.rs` | ~10 | 待开始 | - | 工作区诊断类型 |
| 110 | `src/core/doctor_workspace/methods.rs` | ~15 | 待开始 | - | 工作区诊断实现 |
| 111 | `src/core/doctor_workspace/mod.rs` | ~5 | 待开始 | - | 工作区诊断模块导出 |
| 112 | `src/core/rule_diagnosis/mod.rs` | ~10 | 待开始 | - | 规则诊断 |
| 113 | `src/core/dynamic_plugin_loader/mod.rs` | ~10 | 待开始 | - | 动态插件加载 |
| 114 | `src/core/dynamic_plugin_loader/test.rs` | ~5 | 待开始 | - | 动态插件加载测试 |
| 115 | `src/core/plugin_config_loader/mod.rs` | ~10 | 待开始 | - | 插件配置加载 |
| 116 | `src/core/sys_env/mod.rs` | ~10 | 待开始 | - | 系统环境 |

### 2.6 第 6 组：顶层整合

| 序号 | 文件路径 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|---------|-----------|------|---------|------|
| 117 | `src/core/compression_context.rs` | ~10 | 待开始 | - | 压缩上下文 |
| 118 | `src/core/config_manager.rs` | ~15 | 待开始 | - | 配置管理器 |
| 119 | `src/core/observability.rs` | ~10 | 待开始 | - | 可观测性 |
| 120 | `src/core/tracing_init.rs` | ~5 | 待开始 | - | 追踪初始化 |
| 121 | `src/core/mod.rs` | ~10 | 待开始 | - | 核心层模块导出 |

**小计**: 约 121 个文件，约 1200+ 个函数

---

## 三、 重点注意事项

### 3.1 大文件列表（需分段处理）

以下文件预估超过 500 行，需采用手术刀式编辑：

| 文件 | 预估行数 | 处理策略 |
|------|---------|---------|
| `src/core/text_slicer/methods.rs` | ~1000+ | 按功能拆分：切片算法 / 边界处理 / 配置应用 |
| `src/core/compression_pipeline/methods.rs` | ~1000+ | 按阶段拆分：预处理 / 压缩 / 后处理 |
| `src/core/plugin_dispatcher/methods.rs` | ~800+ | 按流程拆分：发现 / 匹配 / 调度 / 执行 |
| `src/core/content_analyzer/methods.rs` | ~800+ | 按分析类型拆分 |
| `src/core/path_analyzer/methods.rs` | ~800+ | 按分析阶段拆分 |
| `src/core/dictionary_engine/methods.rs` | ~600+ | 按操作类型拆分 |

### 3.2 模块间依赖关系复杂

Core 层模块间调用关系复杂，建议：
1. 严格按自底向上的顺序执行
2. 遇到不熟悉的调用时，先确认下层模块是否已注释
3. 下层模块已有注释的，优先读注释理解，不读源码

---

## 四、 执行步骤

### 步骤 1：读取总计划 + 本分计划
- [ ] 读取 `docs/plans/CODE_COMMENT_PLAN.md`
- [ ] 读取本文件 `docs/plans/core_plan.md`

### 步骤 2-7：按组执行
- [ ] 第 1 组：基础类型与工具（16 个文件）
- [ ] 第 2 组：数据处理引擎（26 个文件）
- [ ] 第 3 组：核心引擎（27 个文件）
- [ ] 第 4 组：插件与流水线（33 个文件）
- [ ] 第 5 组：功能模块（14 个文件）
- [ ] 第 6 组：顶层整合（5 个文件）

每组完成后：
- [ ] 运行 `tokenslim run cargo check`
- [ ] 更新本文件中的状态
- [ ] 记录问题清单

### 步骤 8：最终验证
- [ ] 全量 `tokenslim run cargo check`
- [ ] 抽查 10 个文件，检查注释质量
- [ ] 问题清单汇总

### 步骤 9：收口
- [ ] 更新本文件顶部状态为「已完成」
- [ ] 在总计划中标记本分计划为完成

---

## 五、 问题清单

（执行过程中发现的问题记录在此）

---

## 六、 完成标准

- [ ] 约 121 个文件全部处理完毕
- [ ] 所有 `pub fn` / `pub(crate) fn` / `fn` 都有中文注释
- [ ] 所有 `struct` / `enum` / `trait` 都有中文注释
- [ ] `cargo check` 通过
- [ ] 问题清单已记录
