# TokenSlim 待注释清单（静态扫描生成）

> ⚠️ **过期快照（2026-09-05 标注）**：本清单生成于 2026-07-01，此后自动注释任务已覆盖大量文件（见 `docs/reports/代码注释工作日志.md`），下方"待注释项/待开始"统计已不反映当前状态。如需刷新：重新运行 `python scripts/generate_comment_checklist.py`。
>
> **生成方式**: `scripts/generate_comment_checklist.py` 静态扫描
> **说明**: 本文件只包含清单，不含注释内容。注释由人工逐个分析添加。

## 一、 总览统计

| 指标 | 数值 |
|------|------|
| 总文件数 | 452 |
| 总行数 | 103,927 |
| 总待注释项 | 6,039 |
| 已有模块级注释的文件 | 285 / 452 |

## 二、 按结构类型统计

| 类型 | 数量 | 优先级 |
|------|------|--------|
| `fn` | 3,908 | P0-P1 |
| `test_fn` | 1,029 | P2 |
| `impl` | 388 | P3 |
| `struct` | 314 | P0-P1 |
| `const` | 140 | P3 |
| `static` | 124 | P3 |
| `enum` | 118 | P0-P1 |
| `trait` | 17 | P0 |
| `macro_rules` | 1 | P2 |

## 三、 按层级统计（执行顺序）

| 层级 | 文件数 | 待注释项 | 总行数 | 状态 |
|------|--------|---------|--------|------|
| `utils` | 4 | 36 | 292 | 待开始 |
| `cli` | 18 | 645 | 13,037 | 待开始 |
| `core` | 121 | 1,660 | 26,268 | 待开始 |
| `plugins` | 302 | 3,568 | 60,973 | 待开始 |
| `bin` | 5 | 129 | 3,238 | 待开始 |
| `top_level` | 2 | 1 | 119 | 待开始 |

## 四、 文件清单

### utils 层（4 个文件）

| 文件 | 行数 | 待注释项 | 模块注释 | 状态 |
|------|------|---------|----------|------|
| `src\utils\const.rs` | 5 | 0 | ✅ | 待开始 |
| `src\utils\fn.rs` | 5 | 0 | ✅ | 待开始 |
| `src\utils\i18n.rs` | 268 | 36 | ❌ | 待开始 |
| `src\utils\mod.rs` | 14 | 0 | ✅ | 待开始 |

### cli 层（18 个文件）

| 文件 | 行数 | 待注释项 | 模块注释 | 状态 |
|------|------|---------|----------|------|
| `src\cli\app.rs` | 2170 | 64 | ✅ | 待开始 |
| `src\cli\commands\benchmark.rs` | 3430 | 265 | ✅ | 待开始 |
| `src\cli\commands\compress.rs` | 359 | 10 | ✅ | 待开始 |
| `src\cli\commands\config.rs` | 1013 | 30 | ✅ | 待开始 |
| `src\cli\commands\decompress.rs` | 82 | 1 | ✅ | 待开始 |
| `src\cli\commands\doctor.rs` | 135 | 4 | ✅ | 待开始 |
| `src\cli\commands\export.rs` | 1391 | 50 | ✅ | 待开始 |
| `src\cli\commands\mod.rs` | 12 | 0 | ✅ | 待开始 |
| `src\cli\commands\repair.rs` | 719 | 27 | ✅ | 待开始 |
| `src\cli\commands\run.rs` | 1920 | 70 | ✅ | 待开始 |
| `src\cli\commands\serve_static.rs` | 143 | 2 | ✅ | 待开始 |
| `src\cli\common.rs` | 158 | 9 | ✅ | 待开始 |
| `src\cli\conpty_probe.rs` | 302 | 32 | ✅ | 待开始 |
| `src\cli\mod.rs` | 27 | 0 | ✅ | 待开始 |
| `src\cli\pty_runner.rs` | 286 | 14 | ✅ | 待开始 |
| `src\cli\test.rs` | 13 | 0 | ✅ | 待开始 |
| `src\cli\types.rs` | 457 | 22 | ✅ | 待开始 |
| `src\cli\whitelist.rs` | 420 | 45 | ✅ | 待开始 |

### core 层（121 个文件）

| 文件 | 行数 | 待注释项 | 模块注释 | 状态 |
|------|------|---------|----------|------|
| `src\core\compression\mod.rs` | 15 | 0 | ✅ | 待开始 |
| `src\core\compression\test.rs` | 13 | 0 | ✅ | 待开始 |
| `src\core\compression\types.rs` | 108 | 8 | ✅ | 待开始 |
| `src\core\compression_context.rs` | 58 | 10 | ✅ | 待开始 |
| `src\core\compression_pipeline\methods.rs` | 585 | 19 | ✅ | 待开始 |
| `src\core\compression_pipeline\mod.rs` | 20 | 0 | ✅ | 待开始 |
| `src\core\compression_pipeline\test.rs` | 445 | 22 | ✅ | 待开始 |
| `src\core\compression_pipeline\types.rs` | 77 | 5 | ❌ | 待开始 |
| `src\core\config_manager.rs` | 499 | 24 | ✅ | 待开始 |
| `src\core\const.rs` | 5 | 0 | ✅ | 待开始 |
| `src\core\content_analyzer\drain\methods.rs` | 159 | 7 | ✅ | 待开始 |
| `src\core\content_analyzer\drain\mod.rs` | 8 | 0 | ❌ | 待开始 |
| `src\core\content_analyzer\drain\test.rs` | 41 | 2 | ✅ | 待开始 |
| `src\core\content_analyzer\drain\types.rs` | 48 | 6 | ✅ | 待开始 |
| `src\core\content_analyzer\methods.rs` | 521 | 17 | ✅ | 待开始 |
| `src\core\content_analyzer\mod.rs` | 21 | 0 | ✅ | 待开始 |
| `src\core\content_analyzer\test.rs` | 198 | 10 | ✅ | 待开始 |
| `src\core\content_analyzer\types.rs` | 273 | 23 | ✅ | 待开始 |
| `src\core\dedup_engine\methods.rs` | 134 | 8 | ✅ | 待开始 |
| `src\core\dedup_engine\mod.rs` | 18 | 0 | ✅ | 待开始 |
| `src\core\dedup_engine\test.rs` | 64 | 5 | ✅ | 待开始 |
| `src\core\dedup_engine\types.rs` | 65 | 7 | ✅ | 待开始 |
| `src\core\dictionary_engine\methods.rs` | 303 | 16 | ✅ | 待开始 |
| `src\core\dictionary_engine\mod.rs` | 12 | 0 | ✅ | 待开始 |
| `src\core\dictionary_engine\test.rs` | 127 | 16 | ✅ | 待开始 |
| `src\core\dictionary_engine\types.rs` | 120 | 12 | ❌ | 待开始 |
| `src\core\dictionary_manager\methods.rs` | 344 | 22 | ✅ | 待开始 |
| `src\core\dictionary_manager\mod.rs` | 10 | 0 | ✅ | 待开始 |
| `src\core\doctor_encoding\methods.rs` | 722 | 38 | ❌ | 待开始 |
| `src\core\doctor_encoding\mod.rs` | 6 | 0 | ❌ | 待开始 |
| `src\core\doctor_encoding\types.rs` | 59 | 7 | ❌ | 待开始 |
| `src\core\doctor_workspace\methods.rs` | 3233 | 83 | ❌ | 待开始 |
| `src\core\doctor_workspace\mod.rs` | 6 | 0 | ❌ | 待开始 |
| `src\core\doctor_workspace\types.rs` | 153 | 9 | ❌ | 待开始 |
| `src\core\dynamic_plugin_loader\mod.rs` | 69 | 13 | ✅ | 待开始 |
| `src\core\dynamic_plugin_loader\test.rs` | 47 | 6 | ✅ | 待开始 |
| `src\core\encoding_fallback\mod.rs` | 1763 | 139 | ✅ | 待开始 |
| `src\core\error_isolation\methods.rs` | 83 | 5 | ✅ | 待开始 |
| `src\core\error_isolation\mod.rs` | 16 | 0 | ✅ | 待开始 |
| `src\core\error_isolation\test.rs` | 13 | 0 | ✅ | 待开始 |
| `src\core\error_isolation\types.rs` | 43 | 5 | ✅ | 待开始 |
| `src\core\filter_discover\aggregator.rs` | 276 | 10 | ❌ | 待开始 |
| `src\core\filter_discover\classifier.rs` | 320 | 18 | ❌ | 待开始 |
| `src\core\filter_discover\mod.rs` | 44 | 1 | ❌ | 待开始 |
| `src\core\filter_discover\parser.rs` | 327 | 16 | ❌ | 待开始 |
| `src\core\filter_discover\types.rs` | 76 | 5 | ❌ | 待开始 |
| `src\core\filter_variants\detector.rs` | 19 | 3 | ❌ | 待开始 |
| `src\core\filter_variants\mod.rs` | 7 | 0 | ❌ | 待开始 |
| `src\core\filter_variants\router.rs` | 83 | 4 | ❌ | 待开始 |
| `src\core\filter_variants\types.rs` | 31 | 5 | ❌ | 待开始 |
| `src\core\init_command\methods.rs` | 758 | 22 | ✅ | 待开始 |
| `src\core\init_command\mod.rs` | 11 | 0 | ✅ | 待开始 |
| `src\core\init_command\types.rs` | 45 | 4 | ✅ | 待开始 |
| `src\core\json_extractor\mod.rs` | 273 | 35 | ✅ | 待开始 |
| `src\core\log_reorderer\methods.rs` | 394 | 15 | ❌ | 待开始 |
| `src\core\log_reorderer\mod.rs` | 6 | 0 | ❌ | 待开始 |
| `src\core\log_reorderer\types.rs` | 33 | 4 | ❌ | 待开始 |
| `src\core\metrics\methods.rs` | 274 | 20 | ✅ | 待开始 |
| `src\core\metrics\mod.rs` | 18 | 0 | ✅ | 待开始 |
| `src\core\metrics\test.rs` | 13 | 0 | ✅ | 待开始 |
| `src\core\metrics\types.rs` | 96 | 8 | ✅ | 待开始 |
| `src\core\mod.rs` | 66 | 0 | ✅ | 待开始 |
| `src\core\observability.rs` | 191 | 21 | ❌ | 待开始 |
| `src\core\path_analyzer\methods.rs` | 213 | 11 | ❌ | 待开始 |
| `src\core\path_analyzer\mod.rs` | 4 | 0 | ❌ | 待开始 |
| `src\core\path_analyzer\optimized_methods.rs` | 184 | 9 | ❌ | 待开始 |
| `src\core\path_analyzer\original_methods.rs` | 184 | 9 | ❌ | 待开始 |
| `src\core\path_compressor\methods.rs` | 163 | 6 | ✅ | 待开始 |
| `src\core\path_compressor\mod.rs` | 25 | 0 | ✅ | 待开始 |
| `src\core\path_compressor\types.rs` | 153 | 13 | ✅ | 待开始 |
| `src\core\path_optimizer\methods.rs` | 1090 | 74 | ❌ | 待开始 |
| `src\core\path_optimizer\mod.rs` | 3 | 0 | ❌ | 待开始 |
| `src\core\path_optimizer\token_boundary.rs` | 57 | 5 | ❌ | 待开始 |
| `src\core\plugin_config_loader\mod.rs` | 1716 | 78 | ✅ | 待开始 |
| `src\core\plugin_dispatcher\methods.rs` | 327 | 14 | ✅ | 待开始 |
| `src\core\plugin_dispatcher\mod.rs` | 18 | 0 | ✅ | 待开始 |
| `src\core\plugin_dispatcher\test.rs` | 517 | 40 | ✅ | 待开始 |
| `src\core\plugin_dispatcher\types.rs` | 130 | 18 | ✅ | 待开始 |
| `src\core\rehydration_pipeline\methods.rs` | 299 | 10 | ✅ | 待开始 |
| `src\core\rehydration_pipeline\mod.rs` | 19 | 0 | ✅ | 待开始 |
| `src\core\rehydration_pipeline\test.rs` | 243 | 22 | ✅ | 待开始 |
| `src\core\rehydration_pipeline\types.rs` | 50 | 5 | ✅ | 待开始 |
| `src\core\rewrite\bash_ast.rs` | 191 | 18 | ✅ | 待开始 |
| `src\core\rewrite\mod.rs` | 122 | 11 | ✅ | 待开始 |
| `src\core\rewrite\rules.rs` | 144 | 21 | ✅ | 待开始 |
| `src\core\rewrite\transparent.rs` | 80 | 12 | ✅ | 待开始 |
| `src\core\rewrite\user_config.rs` | 123 | 17 | ✅ | 待开始 |
| `src\core\rule_diagnosis\mod.rs` | 488 | 23 | ✅ | 待开始 |
| `src\core\safety_check\hidden_unicode.rs` | 41 | 6 | ❌ | 待开始 |
| `src\core\safety_check\mod.rs` | 48 | 12 | ✅ | 待开始 |
| `src\core\safety_check\prompt_injection.rs` | 43 | 6 | ❌ | 待开始 |
| `src\core\safety_check\shell_injection.rs` | 35 | 6 | ❌ | 待开始 |
| `src\core\stream_reader\methods.rs` | 463 | 24 | ✅ | 待开始 |
| `src\core\stream_reader\mod.rs` | 21 | 0 | ✅ | 待开始 |
| `src\core\stream_reader\test.rs` | 128 | 22 | ❌ | 待开始 |
| `src\core\stream_reader\types.rs` | 378 | 24 | ✅ | 待开始 |
| `src\core\sys_env\mod.rs` | 58 | 2 | ✅ | 待开始 |
| `src\core\template_render\mod.rs` | 180 | 24 | ❌ | 待开始 |
| `src\core\template_render\parser.rs` | 227 | 24 | ❌ | 待开始 |
| `src\core\template_render\renderer.rs` | 316 | 32 | ❌ | 待开始 |
| `src\core\template_render\types.rs` | 148 | 25 | ❌ | 待开始 |
| `src\core\text_slicer\config_loader.rs` | 195 | 19 | ✅ | 待开始 |
| `src\core\text_slicer\methods.rs` | 681 | 37 | ❌ | 待开始 |
| `src\core\text_slicer\mod.rs` | 26 | 0 | ✅ | 待开始 |
| `src\core\text_slicer\test.rs` | 223 | 15 | ✅ | 待开始 |
| `src\core\text_slicer\types.rs` | 242 | 20 | ✅ | 待开始 |
| `src\core\timestamp_converter\methods.rs` | 29 | 3 | ✅ | 待开始 |
| `src\core\timestamp_converter\mod.rs` | 19 | 0 | ✅ | 待开始 |
| `src\core\timestamp_converter\types.rs` | 195 | 14 | ✅ | 待开始 |
| `src\core\tracing_init.rs` | 32 | 2 | ❌ | 待开始 |
| `src\core\tracking\gain.rs` | 318 | 41 | ✅ | 待开始 |
| `src\core\tracking\mod.rs` | 40 | 0 | ✅ | 待开始 |
| `src\core\tracking\tracker.rs` | 656 | 58 | ✅ | 待开始 |
| `src\core\tracking\types.rs` | 176 | 18 | ✅ | 待开始 |
| `src\core\tree_restructure\config.rs` | 109 | 15 | ❌ | 待开始 |
| `src\core\tree_restructure\mod.rs` | 202 | 10 | ❌ | 待开始 |
| `src\core\tree_restructure\render.rs` | 188 | 11 | ❌ | 待开始 |
| `src\core\tree_restructure\trie.rs` | 259 | 18 | ❌ | 待开始 |
| `src\core\utils\json.rs` | 87 | 8 | ❌ | 待开始 |
| `src\core\utils\mod.rs` | 18 | 2 | ❌ | 待开始 |
| `src\core\utils\roi.rs` | 79 | 11 | ✅ | 待开始 |

### plugins 层（302 个文件）

| 文件 | 行数 | 待注释项 | 模块注释 | 状态 |
|------|------|---------|----------|------|
| `src\plugins\android_gradle_plugin\methods.rs` | 382 | 12 | ✅ | 待开始 |
| `src\plugins\android_gradle_plugin\mod.rs` | 27 | 0 | ✅ | 待开始 |
| `src\plugins\android_gradle_plugin\showcase.rs` | 153 | 4 | ❌ | 待开始 |
| `src\plugins\android_gradle_plugin\test.rs` | 126 | 20 | ✅ | 待开始 |
| `src\plugins\android_gradle_plugin\types.rs` | 114 | 15 | ✅ | 待开始 |
| `src\plugins\ansi_cleaner_plugin\methods.rs` | 155 | 14 | ✅ | 待开始 |
| `src\plugins\ansi_cleaner_plugin\mod.rs` | 21 | 0 | ✅ | 待开始 |
| `src\plugins\ansi_cleaner_plugin\showcase.rs` | 122 | 4 | ❌ | 待开始 |
| `src\plugins\ansi_cleaner_plugin\test.rs` | 38 | 6 | ✅ | 待开始 |
| `src\plugins\ansi_cleaner_plugin\types.rs` | 19 | 1 | ✅ | 待开始 |
| `src\plugins\ansible_plugin\methods.rs` | 267 | 21 | ✅ | 待开始 |
| `src\plugins\ansible_plugin\mod.rs` | 25 | 0 | ✅ | 待开始 |
| `src\plugins\ansible_plugin\showcase.rs` | 66 | 2 | ✅ | 待开始 |
| `src\plugins\ansible_plugin\test.rs` | 24 | 4 | ✅ | 待开始 |
| `src\plugins\ansible_plugin\types.rs` | 7 | 1 | ✅ | 待开始 |
| `src\plugins\artifact_summary_plugin\methods.rs` | 508 | 23 | ✅ | 待开始 |
| `src\plugins\artifact_summary_plugin\mod.rs` | 27 | 0 | ✅ | 待开始 |
| `src\plugins\artifact_summary_plugin\showcase.rs` | 66 | 2 | ✅ | 待开始 |
| `src\plugins\artifact_summary_plugin\test.rs` | 57 | 10 | ✅ | 待开始 |
| `src\plugins\artifact_summary_plugin\types.rs` | 58 | 6 | ✅ | 待开始 |
| `src\plugins\bazel_plugin\methods.rs` | 144 | 9 | ✅ | 待开始 |
| `src\plugins\bazel_plugin\mod.rs` | 23 | 0 | ✅ | 待开始 |
| `src\plugins\bazel_plugin\showcase.rs` | 66 | 2 | ✅ | 待开始 |
| `src\plugins\bazel_plugin\test.rs` | 24 | 4 | ✅ | 待开始 |
| `src\plugins\bazel_plugin\types.rs` | 7 | 1 | ✅ | 待开始 |
| `src\plugins\ci_log_plugin\methods.rs` | 566 | 26 | ✅ | 待开始 |
| `src\plugins\ci_log_plugin\mod.rs` | 26 | 0 | ✅ | 待开始 |
| `src\plugins\ci_log_plugin\showcase.rs` | 195 | 2 | ✅ | 待开始 |
| `src\plugins\ci_log_plugin\test.rs` | 112 | 20 | ✅ | 待开始 |
| `src\plugins\ci_log_plugin\types.rs` | 7 | 1 | ✅ | 待开始 |
| `src\plugins\cloud_log_plugin\methods.rs` | 1765 | 80 | ❌ | 待开始 |
| `src\plugins\cloud_log_plugin\mod.rs` | 25 | 0 | ✅ | 待开始 |
| `src\plugins\cloud_log_plugin\showcase.rs` | 222 | 4 | ❌ | 待开始 |
| `src\plugins\cloud_log_plugin\test.rs` | 303 | 44 | ✅ | 待开始 |
| `src\plugins\cloud_log_plugin\types.rs` | 14 | 1 | ❌ | 待开始 |
| `src\plugins\cloudformation_plugin\methods.rs` | 137 | 10 | ✅ | 待开始 |
| `src\plugins\cloudformation_plugin\mod.rs` | 21 | 0 | ✅ | 待开始 |
| `src\plugins\cloudformation_plugin\showcase.rs` | 66 | 2 | ✅ | 待开始 |
| `src\plugins\cloudformation_plugin\test.rs` | 24 | 4 | ✅ | 待开始 |
| `src\plugins\cloudformation_plugin\types.rs` | 7 | 1 | ✅ | 待开始 |
| `src\plugins\db_log_plugin\methods.rs` | 339 | 19 | ❌ | 待开始 |
| `src\plugins\db_log_plugin\mod.rs` | 23 | 0 | ✅ | 待开始 |
| `src\plugins\db_log_plugin\showcase.rs` | 144 | 4 | ❌ | 待开始 |
| `src\plugins\db_log_plugin\test.rs` | 130 | 20 | ✅ | 待开始 |
| `src\plugins\db_log_plugin\types.rs` | 14 | 1 | ❌ | 待开始 |
| `src\plugins\dotnet_plugin\methods.rs` | 172 | 14 | ✅ | 待开始 |
| `src\plugins\dotnet_plugin\mod.rs` | 19 | 0 | ✅ | 待开始 |
| `src\plugins\dotnet_plugin\showcase.rs` | 122 | 4 | ❌ | 待开始 |
| `src\plugins\dotnet_plugin\test.rs` | 44 | 6 | ✅ | 待开始 |
| `src\plugins\dotnet_plugin\types.rs` | 29 | 4 | ❌ | 待开始 |
| `src\plugins\gcc_log_plugin\methods.rs` | 874 | 38 | ✅ | 待开始 |
| `src\plugins\gcc_log_plugin\mod.rs` | 25 | 0 | ✅ | 待开始 |
| `src\plugins\gcc_log_plugin\showcase.rs` | 165 | 4 | ✅ | 待开始 |
| `src\plugins\gcc_log_plugin\test.rs` | 157 | 22 | ✅ | 待开始 |
| `src\plugins\gcc_log_plugin\types.rs` | 48 | 4 | ✅ | 待开始 |
| `src\plugins\generic_text_plugin\methods.rs` | 61 | 1 | ❌ | 待开始 |
| `src\plugins\generic_text_plugin\mod.rs` | 19 | 0 | ✅ | 待开始 |
| `src\plugins\generic_text_plugin\showcase.rs` | 130 | 4 | ❌ | 待开始 |
| `src\plugins\generic_text_plugin\test.rs` | 46 | 6 | ✅ | 待开始 |
| `src\plugins\generic_text_plugin\types.rs` | 88 | 13 | ❌ | 待开始 |
| `src\plugins\git_diff_plugin\methods.rs` | 190 | 11 | ✅ | 待开始 |
| `src\plugins\git_diff_plugin\mod.rs` | 23 | 0 | ✅ | 待开始 |
| `src\plugins\git_diff_plugin\showcase.rs` | 130 | 4 | ❌ | 待开始 |
| `src\plugins\git_diff_plugin\test.rs` | 37 | 4 | ✅ | 待开始 |
| `src\plugins\git_diff_plugin\types.rs` | 44 | 6 | ❌ | 待开始 |
| `src\plugins\helm_plugin\methods.rs` | 138 | 9 | ✅ | 待开始 |
| `src\plugins\helm_plugin\mod.rs` | 27 | 0 | ✅ | 待开始 |
| `src\plugins\helm_plugin\showcase.rs` | 66 | 2 | ✅ | 待开始 |
| `src\plugins\helm_plugin\test.rs` | 24 | 4 | ✅ | 待开始 |
| `src\plugins\helm_plugin\types.rs` | 7 | 1 | ✅ | 待开始 |
| `src\plugins\infra_tools_common.rs` | 150 | 11 | ✅ | 待开始 |
| `src\plugins\java_stack_plugin\methods.rs` | 630 | 27 | ✅ | 待开始 |
| `src\plugins\java_stack_plugin\mod.rs` | 26 | 0 | ✅ | 待开始 |
| `src\plugins\java_stack_plugin\showcase.rs` | 134 | 4 | ❌ | 待开始 |
| `src\plugins\java_stack_plugin\test.rs` | 152 | 18 | ✅ | 待开始 |
| `src\plugins\java_stack_plugin\types.rs` | 13 | 2 | ❌ | 待开始 |
| `src\plugins\json_plugin\methods.rs` | 162 | 12 | ✅ | 待开始 |
| `src\plugins\json_plugin\mod.rs` | 26 | 0 | ✅ | 待开始 |
| `src\plugins\json_plugin\showcase.rs` | 134 | 5 | ❌ | 待开始 |
| `src\plugins\json_plugin\test.rs` | 47 | 6 | ✅ | 待开始 |
| `src\plugins\json_plugin\types.rs` | 68 | 8 | ❌ | 待开始 |
| `src\plugins\kubernetes_docker_plugin\methods.rs` | 207 | 16 | ✅ | 待开始 |
| `src\plugins\kubernetes_docker_plugin\mod.rs` | 23 | 0 | ✅ | 待开始 |
| `src\plugins\kubernetes_docker_plugin\showcase.rs` | 169 | 4 | ❌ | 待开始 |
| `src\plugins\kubernetes_docker_plugin\test.rs` | 212 | 12 | ✅ | 待开始 |
| `src\plugins\kubernetes_docker_plugin\types.rs` | 32 | 4 | ❌ | 待开始 |
| `src\plugins\markdown_plugin\methods.rs` | 121 | 13 | ✅ | 待开始 |
| `src\plugins\markdown_plugin\mod.rs` | 19 | 0 | ✅ | 待开始 |
| `src\plugins\markdown_plugin\showcase.rs` | 134 | 5 | ❌ | 待开始 |
| `src\plugins\markdown_plugin\test.rs` | 50 | 6 | ✅ | 待开始 |
| `src\plugins\markdown_plugin\types.rs` | 29 | 4 | ❌ | 待开始 |
| `src\plugins\maven_plugin\methods.rs` | 597 | 20 | ✅ | 待开始 |
| `src\plugins\maven_plugin\mod.rs` | 21 | 0 | ✅ | 待开始 |
| `src\plugins\maven_plugin\showcase.rs` | 156 | 7 | ❌ | 待开始 |
| `src\plugins\maven_plugin\test.rs` | 138 | 16 | ✅ | 待开始 |
| `src\plugins\maven_plugin\types.rs` | 27 | 4 | ❌ | 待开始 |
| `src\plugins\mod.rs` | 66 | 0 | ✅ | 待开始 |
| `src\plugins\ndjson_plugin\methods.rs` | 340 | 17 | ✅ | 待开始 |
| `src\plugins\ndjson_plugin\mod.rs` | 19 | 0 | ✅ | 待开始 |
| `src\plugins\ndjson_plugin\showcase.rs` | 67 | 2 | ✅ | 待开始 |
| `src\plugins\ndjson_plugin\test.rs` | 72 | 14 | ✅ | 待开始 |
| `src\plugins\ndjson_plugin\types.rs` | 195 | 15 | ✅ | 待开始 |
| `src\plugins\node_error_plugin\methods.rs` | 290 | 15 | ✅ | 待开始 |
| `src\plugins\node_error_plugin\mod.rs` | 27 | 0 | ✅ | 待开始 |
| `src\plugins\node_error_plugin\showcase.rs` | 130 | 4 | ❌ | 待开始 |
| `src\plugins\node_error_plugin\test.rs` | 57 | 6 | ✅ | 待开始 |
| `src\plugins\node_error_plugin\types.rs` | 18 | 1 | ❌ | 待开始 |
| `src\plugins\nodejs_plugin\methods.rs` | 798 | 16 | ✅ | 待开始 |
| `src\plugins\nodejs_plugin\mod.rs` | 27 | 0 | ✅ | 待开始 |
| `src\plugins\nodejs_plugin\showcase.rs` | 144 | 4 | ❌ | 待开始 |
| `src\plugins\nodejs_plugin\test.rs` | 181 | 22 | ✅ | 待开始 |
| `src\plugins\nodejs_plugin\types.rs` | 129 | 15 | ✅ | 待开始 |
| `src\plugins\noise_filter_plugin\methods.rs` | 211 | 14 | ✅ | 待开始 |
| `src\plugins\noise_filter_plugin\mod.rs` | 17 | 0 | ✅ | 待开始 |
| `src\plugins\noise_filter_plugin\showcase.rs` | 130 | 4 | ❌ | 待开始 |
| `src\plugins\noise_filter_plugin\test.rs` | 40 | 6 | ✅ | 待开始 |
| `src\plugins\noise_filter_plugin\types.rs` | 35 | 4 | ❌ | 待开始 |
| `src\plugins\php_ruby_plugin\methods.rs` | 120 | 13 | ❌ | 待开始 |
| `src\plugins\php_ruby_plugin\mod.rs` | 20 | 0 | ✅ | 待开始 |
| `src\plugins\php_ruby_plugin\showcase.rs` | 130 | 4 | ❌ | 待开始 |
| `src\plugins\php_ruby_plugin\test.rs` | 42 | 6 | ✅ | 待开始 |
| `src\plugins\php_ruby_plugin\types.rs` | 25 | 4 | ❌ | 待开始 |
| `src\plugins\protobuf_plugin\methods.rs` | 119 | 10 | ✅ | 待开始 |
| `src\plugins\protobuf_plugin\mod.rs` | 23 | 0 | ✅ | 待开始 |
| `src\plugins\protobuf_plugin\showcase.rs` | 66 | 2 | ✅ | 待开始 |
| `src\plugins\protobuf_plugin\test.rs` | 24 | 4 | ✅ | 待开始 |
| `src\plugins\protobuf_plugin\types.rs` | 7 | 1 | ✅ | 待开始 |
| `src\plugins\pulumi_plugin\methods.rs` | 137 | 10 | ✅ | 待开始 |
| `src\plugins\pulumi_plugin\mod.rs` | 21 | 0 | ✅ | 待开始 |
| `src\plugins\pulumi_plugin\showcase.rs` | 66 | 2 | ✅ | 待开始 |
| `src\plugins\pulumi_plugin\test.rs` | 24 | 4 | ✅ | 待开始 |
| `src\plugins\pulumi_plugin\types.rs` | 7 | 1 | ✅ | 待开始 |
| `src\plugins\pytest_plugin\methods.rs` | 232 | 13 | ✅ | 待开始 |
| `src\plugins\pytest_plugin\mod.rs` | 24 | 0 | ✅ | 待开始 |
| `src\plugins\pytest_plugin\showcase.rs` | 91 | 2 | ✅ | 待开始 |
| `src\plugins\pytest_plugin\test.rs` | 63 | 10 | ✅ | 待开始 |
| `src\plugins\pytest_plugin\types.rs` | 7 | 1 | ✅ | 待开始 |
| `src\plugins\python_traceback_plugin\methods.rs` | 487 | 19 | ✅ | 待开始 |
| `src\plugins\python_traceback_plugin\mod.rs` | 27 | 0 | ✅ | 待开始 |
| `src\plugins\python_traceback_plugin\showcase.rs` | 134 | 4 | ❌ | 待开始 |
| `src\plugins\python_traceback_plugin\test.rs` | 139 | 16 | ✅ | 待开始 |
| `src\plugins\python_traceback_plugin\types.rs` | 21 | 1 | ❌ | 待开始 |
| `src\plugins\rust_go_plugin\methods.rs` | 507 | 17 | ❌ | 待开始 |
| `src\plugins\rust_go_plugin\mod.rs` | 24 | 0 | ✅ | 待开始 |
| `src\plugins\rust_go_plugin\showcase.rs` | 128 | 4 | ❌ | 待开始 |
| `src\plugins\rust_go_plugin\test.rs` | 140 | 16 | ✅ | 待开始 |
| `src\plugins\rust_go_plugin\types.rs` | 12 | 1 | ❌ | 待开始 |
| `src\plugins\shell_session_plugin\methods.rs` | 83 | 10 | ❌ | 待开始 |
| `src\plugins\shell_session_plugin\mod.rs` | 17 | 0 | ✅ | 待开始 |
| `src\plugins\shell_session_plugin\parser.rs` | 295 | 12 | ❌ | 待开始 |
| `src\plugins\shell_session_plugin\showcase.rs` | 153 | 3 | ❌ | 待开始 |
| `src\plugins\smart_code_plugin\methods.rs` | 295 | 14 | ✅ | 待开始 |
| `src\plugins\smart_code_plugin\mod.rs` | 22 | 0 | ✅ | 待开始 |
| `src\plugins\smart_code_plugin\showcase.rs` | 130 | 4 | ❌ | 待开始 |
| `src\plugins\smart_code_plugin\test.rs` | 59 | 8 | ✅ | 待开始 |
| `src\plugins\smart_code_plugin\types.rs` | 42 | 4 | ❌ | 待开始 |
| `src\plugins\smart_path_plugin\methods.rs` | 75 | 13 | ✅ | 待开始 |
| `src\plugins\smart_path_plugin\mod.rs` | 16 | 0 | ✅ | 待开始 |
| `src\plugins\smart_path_plugin\showcase.rs` | 130 | 4 | ❌ | 待开始 |
| `src\plugins\smart_path_plugin\test.rs` | 38 | 6 | ✅ | 待开始 |
| `src\plugins\smart_path_plugin\types.rs` | 10 | 0 | ✅ | 待开始 |
| `src\plugins\spring_boot_plugin\methods.rs` | 157 | 13 | ✅ | 待开始 |
| `src\plugins\spring_boot_plugin\mod.rs` | 23 | 0 | ✅ | 待开始 |
| `src\plugins\spring_boot_plugin\showcase.rs` | 136 | 4 | ❌ | 待开始 |
| `src\plugins\spring_boot_plugin\test.rs` | 93 | 14 | ✅ | 待开始 |
| `src\plugins\spring_boot_plugin\types.rs` | 32 | 4 | ❌ | 待开始 |
| `src\plugins\sql_plugin\methods.rs` | 151 | 17 | ✅ | 待开始 |
| `src\plugins\sql_plugin\mod.rs` | 19 | 0 | ✅ | 待开始 |
| `src\plugins\sql_plugin\showcase.rs` | 130 | 4 | ❌ | 待开始 |
| `src\plugins\sql_plugin\test.rs` | 54 | 8 | ✅ | 待开始 |
| `src\plugins\sql_plugin\types.rs` | 35 | 4 | ❌ | 待开始 |
| `src\plugins\static_rule_plugin\methods.rs` | 452 | 23 | ❌ | 待开始 |
| `src\plugins\static_rule_plugin\mod.rs` | 18 | 0 | ✅ | 待开始 |
| `src\plugins\static_rule_plugin\showcase.rs` | 130 | 4 | ❌ | 待开始 |
| `src\plugins\static_rule_plugin\test.rs` | 53 | 6 | ✅ | 待开始 |
| `src\plugins\static_rule_plugin\types.rs` | 75 | 8 | ❌ | 待开始 |
| `src\plugins\syslog_plugin\methods.rs` | 144 | 11 | ❌ | 待开始 |
| `src\plugins\syslog_plugin\mod.rs` | 20 | 0 | ✅ | 待开始 |
| `src\plugins\syslog_plugin\showcase.rs` | 122 | 4 | ❌ | 待开始 |
| `src\plugins\syslog_plugin\test.rs` | 51 | 6 | ✅ | 待开始 |
| `src\plugins\syslog_plugin\types.rs` | 10 | 1 | ❌ | 待开始 |
| `src\plugins\template_driven_plugin\methods.rs` | 129 | 11 | ✅ | 待开始 |
| `src\plugins\template_driven_plugin\mod.rs` | 17 | 0 | ✅ | 待开始 |
| `src\plugins\template_driven_plugin\showcase.rs` | 130 | 4 | ❌ | 待开始 |
| `src\plugins\template_driven_plugin\test.rs` | 38 | 6 | ✅ | 待开始 |
| `src\plugins\template_driven_plugin\types.rs` | 26 | 3 | ❌ | 待开始 |
| `src\plugins\terraform_plugin\methods.rs` | 143 | 12 | ✅ | 待开始 |
| `src\plugins\terraform_plugin\mod.rs` | 23 | 0 | ✅ | 待开始 |
| `src\plugins\terraform_plugin\showcase.rs` | 66 | 2 | ✅ | 待开始 |
| `src\plugins\terraform_plugin\test.rs` | 25 | 4 | ✅ | 待开始 |
| `src\plugins\terraform_plugin\types.rs` | 7 | 1 | ✅ | 待开始 |
| `src\plugins\test_utils.rs` | 136 | 8 | ✅ | 待开始 |
| `src\plugins\unity_unreal_plugin\methods.rs` | 162 | 12 | ❌ | 待开始 |
| `src\plugins\unity_unreal_plugin\mod.rs` | 16 | 0 | ✅ | 待开始 |
| `src\plugins\unity_unreal_plugin\showcase.rs` | 133 | 4 | ❌ | 待开始 |
| `src\plugins\unity_unreal_plugin\test.rs` | 80 | 12 | ✅ | 待开始 |
| `src\plugins\unity_unreal_plugin\types.rs` | 25 | 4 | ❌ | 待开始 |
| `src\plugins\vcs_az_plugin\methods.rs` | 408 | 17 | ✅ | 待开始 |
| `src\plugins\vcs_az_plugin\mod.rs` | 24 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_az_plugin\parser.rs` | 66 | 8 | ❌ | 待开始 |
| `src\plugins\vcs_az_plugin\showcase.rs` | 74 | 2 | ❌ | 待开始 |
| `src\plugins\vcs_az_plugin\tests.rs` | 241 | 26 | ❌ | 待开始 |
| `src\plugins\vcs_bitbucket_plugin\methods.rs` | 496 | 18 | ✅ | 待开始 |
| `src\plugins\vcs_bitbucket_plugin\mod.rs` | 24 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_bitbucket_plugin\parser.rs` | 66 | 8 | ❌ | 待开始 |
| `src\plugins\vcs_bitbucket_plugin\showcase.rs` | 74 | 2 | ❌ | 待开始 |
| `src\plugins\vcs_bitbucket_plugin\tests.rs` | 319 | 26 | ❌ | 待开始 |
| `src\plugins\vcs_bzr_plugin\methods.rs` | 613 | 25 | ✅ | 待开始 |
| `src\plugins\vcs_bzr_plugin\mod.rs` | 22 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_bzr_plugin\parser.rs` | 570 | 37 | ✅ | 待开始 |
| `src\plugins\vcs_bzr_plugin\showcase.rs` | 89 | 2 | ❌ | 待开始 |
| `src\plugins\vcs_bzr_plugin\tests.rs` | 116 | 18 | ❌ | 待开始 |
| `src\plugins\vcs_cvs_plugin\methods.rs` | 574 | 25 | ✅ | 待开始 |
| `src\plugins\vcs_cvs_plugin\mod.rs` | 25 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_cvs_plugin\parser.rs` | 873 | 47 | ✅ | 待开始 |
| `src\plugins\vcs_cvs_plugin\showcase.rs` | 90 | 2 | ❌ | 待开始 |
| `src\plugins\vcs_cvs_plugin\tests.rs` | 123 | 20 | ❌ | 待开始 |
| `src\plugins\vcs_darcs_plugin\methods.rs` | 609 | 21 | ✅ | 待开始 |
| `src\plugins\vcs_darcs_plugin\mod.rs` | 21 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_darcs_plugin\parser.rs` | 428 | 33 | ✅ | 待开始 |
| `src\plugins\vcs_darcs_plugin\showcase.rs` | 86 | 2 | ❌ | 待开始 |
| `src\plugins\vcs_darcs_plugin\tests.rs` | 144 | 20 | ❌ | 待开始 |
| `src\plugins\vcs_fossil_plugin\methods.rs` | 462 | 23 | ✅ | 待开始 |
| `src\plugins\vcs_fossil_plugin\mod.rs` | 22 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_fossil_plugin\parser.rs` | 406 | 35 | ✅ | 待开始 |
| `src\plugins\vcs_fossil_plugin\showcase.rs` | 86 | 2 | ❌ | 待开始 |
| `src\plugins\vcs_fossil_plugin\tests.rs` | 103 | 16 | ❌ | 待开始 |
| `src\plugins\vcs_gerrit_plugin\methods.rs` | 456 | 16 | ✅ | 待开始 |
| `src\plugins\vcs_gerrit_plugin\mod.rs` | 27 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_gerrit_plugin\parser.rs` | 66 | 8 | ❌ | 待开始 |
| `src\plugins\vcs_gerrit_plugin\showcase.rs` | 74 | 2 | ❌ | 待开始 |
| `src\plugins\vcs_gerrit_plugin\tests.rs` | 236 | 26 | ❌ | 待开始 |
| `src\plugins\vcs_gh_plugin\methods.rs` | 744 | 30 | ✅ | 待开始 |
| `src\plugins\vcs_gh_plugin\mod.rs` | 25 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_gh_plugin\parser.rs` | 146 | 17 | ❌ | 待开始 |
| `src\plugins\vcs_gh_plugin\showcase.rs` | 85 | 2 | ❌ | 待开始 |
| `src\plugins\vcs_gh_plugin\tests.rs` | 116 | 20 | ❌ | 待开始 |
| `src\plugins\vcs_git_plugin\methods.rs` | 761 | 42 | ❌ | 待开始 |
| `src\plugins\vcs_git_plugin\mod.rs` | 28 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_git_plugin\parser.rs` | 2093 | 84 | ❌ | 待开始 |
| `src\plugins\vcs_git_plugin\showcase.rs` | 209 | 2 | ❌ | 待开始 |
| `src\plugins\vcs_git_plugin\tests.rs` | 779 | 81 | ❌ | 待开始 |
| `src\plugins\vcs_glab_plugin\methods.rs` | 662 | 25 | ✅ | 待开始 |
| `src\plugins\vcs_glab_plugin\mod.rs` | 23 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_glab_plugin\parser.rs` | 68 | 8 | ❌ | 待开始 |
| `src\plugins\vcs_glab_plugin\showcase.rs` | 72 | 2 | ❌ | 待开始 |
| `src\plugins\vcs_glab_plugin\tests.rs` | 207 | 22 | ❌ | 待开始 |
| `src\plugins\vcs_hg_plugin\methods.rs` | 522 | 34 | ❌ | 待开始 |
| `src\plugins\vcs_hg_plugin\mod.rs` | 28 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_hg_plugin\parser.rs` | 1548 | 85 | ❌ | 待开始 |
| `src\plugins\vcs_hg_plugin\showcase.rs` | 204 | 3 | ❌ | 待开始 |
| `src\plugins\vcs_hg_plugin\tests.rs` | 538 | 118 | ❌ | 待开始 |
| `src\plugins\vcs_p4_plugin\methods.rs` | 1572 | 89 | ✅ | 待开始 |
| `src\plugins\vcs_p4_plugin\mod.rs` | 25 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_p4_plugin\parser.rs` | 1310 | 80 | ✅ | 待开始 |
| `src\plugins\vcs_p4_plugin\showcase.rs` | 123 | 2 | ❌ | 待开始 |
| `src\plugins\vcs_p4_plugin\tests.rs` | 655 | 64 | ❌ | 待开始 |
| `src\plugins\vcs_plugin\ir.rs` | 34 | 3 | ❌ | 待开始 |
| `src\plugins\vcs_plugin\methods.rs` | 1556 | 96 | ✅ | 待开始 |
| `src\plugins\vcs_plugin\methods\core_logic.rs` | 1834 | 20 | ❌ | 待开始 |
| `src\plugins\vcs_plugin\methods\text_compact.rs` | 1055 | 66 | ❌ | 待开始 |
| `src\plugins\vcs_plugin\mod.rs` | 28 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_plugin\parser.rs` | 19 | 2 | ✅ | 待开始 |
| `src\plugins\vcs_plugin\parser\helpers.rs` | 2447 | 112 | ❌ | 待开始 |
| `src\plugins\vcs_plugin\rule_engine.rs` | 124 | 4 | ❌ | 待开始 |
| `src\plugins\vcs_plugin\test.rs` | 169 | 15 | ✅ | 待开始 |
| `src\plugins\vcs_plugin\types.rs` | 306 | 32 | ❌ | 待开始 |
| `src\plugins\vcs_repo_plugin\methods.rs` | 401 | 13 | ✅ | 待开始 |
| `src\plugins\vcs_repo_plugin\mod.rs` | 23 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_repo_plugin\parser.rs` | 73 | 8 | ❌ | 待开始 |
| `src\plugins\vcs_repo_plugin\showcase.rs` | 76 | 2 | ❌ | 待开始 |
| `src\plugins\vcs_repo_plugin\tests.rs` | 200 | 16 | ❌ | 待开始 |
| `src\plugins\vcs_svn_plugin\methods.rs` | 659 | 33 | ✅ | 待开始 |
| `src\plugins\vcs_svn_plugin\mod.rs` | 19 | 0 | ✅ | 待开始 |
| `src\plugins\vcs_svn_plugin\parser.rs` | 3458 | 160 | ❌ | 待开始 |
| `src\plugins\vcs_svn_plugin\showcase.rs` | 142 | 2 | ❌ | 待开始 |
| `src\plugins\vcs_svn_plugin\tests.rs` | 318 | 39 | ❌ | 待开始 |
| `src\plugins\web_log_plugin\methods.rs` | 1863 | 99 | ❌ | 待开始 |
| `src\plugins\web_log_plugin\mod.rs` | 26 | 0 | ✅ | 待开始 |
| `src\plugins\web_log_plugin\showcase.rs` | 193 | 4 | ❌ | 待开始 |
| `src\plugins\web_log_plugin\test.rs` | 256 | 42 | ✅ | 待开始 |
| `src\plugins\web_log_plugin\types.rs` | 17 | 1 | ❌ | 待开始 |
| `src\plugins\webpack_vite_plugin\methods.rs` | 463 | 22 | ❌ | 待开始 |
| `src\plugins\webpack_vite_plugin\mod.rs` | 18 | 0 | ✅ | 待开始 |
| `src\plugins\webpack_vite_plugin\showcase.rs` | 130 | 4 | ❌ | 待开始 |
| `src\plugins\webpack_vite_plugin\test.rs` | 60 | 8 | ✅ | 待开始 |
| `src\plugins\webpack_vite_plugin\types.rs` | 25 | 4 | ❌ | 待开始 |
| `src\plugins\xcode_log_plugin\methods.rs` | 216 | 12 | ❌ | 待开始 |
| `src\plugins\xcode_log_plugin\mod.rs` | 21 | 0 | ✅ | 待开始 |
| `src\plugins\xcode_log_plugin\showcase.rs` | 122 | 4 | ❌ | 待开始 |
| `src\plugins\xcode_log_plugin\test.rs` | 29 | 4 | ✅ | 待开始 |
| `src\plugins\xcode_log_plugin\types.rs` | 11 | 1 | ❌ | 待开始 |
| `src\plugins\xml_html_plugin\methods.rs` | 130 | 13 | ✅ | 待开始 |
| `src\plugins\xml_html_plugin\mod.rs` | 20 | 0 | ✅ | 待开始 |
| `src\plugins\xml_html_plugin\showcase.rs` | 130 | 4 | ❌ | 待开始 |
| `src\plugins\xml_html_plugin\test.rs` | 41 | 6 | ✅ | 待开始 |
| `src\plugins\xml_html_plugin\types.rs` | 16 | 1 | ❌ | 待开始 |
| `src\plugins\yaml_plugin\methods.rs` | 185 | 16 | ✅ | 待开始 |
| `src\plugins\yaml_plugin\mod.rs` | 23 | 0 | ✅ | 待开始 |
| `src\plugins\yaml_plugin\showcase.rs` | 135 | 4 | ❌ | 待开始 |
| `src\plugins\yaml_plugin\test.rs` | 71 | 10 | ✅ | 待开始 |
| `src\plugins\yaml_plugin\types.rs` | 61 | 8 | ❌ | 待开始 |

### bin 层（5 个文件）

| 文件 | 行数 | 待注释项 | 模块注释 | 状态 |
|------|------|---------|----------|------|
| `src\bin\log_miner.rs` | 126 | 3 | ✅ | 待开始 |
| `src\bin\log_reorder.rs` | 120 | 4 | ❌ | 待开始 |
| `src\bin\pipeline_bench.rs` | 400 | 12 | ❌ | 待开始 |
| `src\bin\tokenslim-server.rs` | 2392 | 100 | ❌ | 待开始 |
| `src\bin\tree_dict_experiment.rs` | 200 | 10 | ❌ | 待开始 |

### top_level 层（2 个文件）

| 文件 | 行数 | 待注释项 | 模块注释 | 状态 |
|------|------|---------|----------|------|
| `src\lib.rs` | 64 | 0 | ✅ | 待开始 |
| `src\main.rs` | 55 | 1 | ✅ | 待开始 |

---

*本清单由脚本静态扫描生成，确保 100% 文件覆盖率。注释内容由人工逐个分析添加。*