# TokenSlim 工具手册

> 本文件按需加载。当你需要知道"某个脚本怎么用"或"有哪些可用脚本"时才读它。
> 审计流水线的 4 个核心脚本用法和参数已在 CLAUDE.md 中，不需要重复查本文件。
> Agent 需要查找脚本用法时读此文件。不需要每次会话都加载。

## 审计流水线（核心，变更后必跑）

| 脚本 | 用途 | 用法 |
|------|------|------|
| `audit_sample_case_quality.py` | 物理 case 质量门禁（压缩前） | `tokenslim run python scripts/audit_sample_case_quality.py --plugin <plugin>` |
| `audit_case_metrics.py` | 压缩语义 + 冻结门禁（压缩后） | `tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --version vX_r1 --require-semantic-gate` |
| `audit_all_case_metrics.py` | 全插件健康检查 | `tokenslim run python scripts/audit_all_case_metrics.py --version vX_r1 --require-semantic-gate --fail-on-regression` |
| `generate_plugin_capability_index.py` | 刷新插件能力索引/矩阵 | `tokenslim run python scripts/generate_plugin_capability_index.py` |
| `audit_llm_common.py` | 审计公共基座（LLMConfig、提示词加载、漂移检测） | 被上述脚本 import，不单独运行 |

## 审计辅助（按需运行）

| 脚本 | 用途 | 用法 |
|------|------|------|
| `generate_case_sidecars.py` | 为 case 生成 scenario.yaml 模板 | `python scripts/generate_case_sidecars.py --plugin <plugin>` |
| `fill_case_sidecars.py` | 批量填充 sidecar 全部 5 字段（含 expected_dispatch_chain） | `python scripts/fill_case_sidecars.py --plugin <plugin>` |
| `extract_plugin_design.py` | 从源码提取设计意图 → 写入 config/plugins/*.json；`--update-modrs` 同步更新 mod.rs //! 文档注释 | `python scripts/extract_plugin_design.py [--plugin <name>] [--update-modrs]` |
| `generate_plugin_alignment_report.py` | 生成插件功能对齐报告 | `python scripts/generate_plugin_alignment_report.py` |
| `project_insight.py` | 提取功能点/调用链/覆盖矩阵 | `python scripts/project_insight.py --config config/project_insight.toml --out-dir docs/audit` |

## PS1 专项审计（独立功能，无 Python 替代）

| 脚本 | 用途 | 用法 |
|------|------|------|
| `audit_artifact_governance.ps1` | 构建产物治理审计 | `powershell -File scripts/audit_artifact_governance.ps1` |
| `audit_error_literal_guard.ps1` | 错误字面量守卫审计 | `powershell -File scripts/audit_error_literal_guard.ps1` |
| `audit_i18n_coverage.ps1` | 国际化覆盖率审计 | `powershell -File scripts/audit_i18n_coverage.ps1` |
| `release_smoke_gate.ps1` | 发布冒烟门禁 | `powershell -File scripts/release_smoke_gate.ps1` |

## 构建 & 开发工具

| 脚本 | 用途 | 用法 |
|------|------|------|
| `build_and_install.py` | 构建并安装 TokenSlim 到本地 | `python scripts/build_and_install.py` |
| `generate_vcs_config.py` | 从 VCS 命令日志生成插件配置 | `python scripts/generate_vcs_config.py --input samples/` |
| `generate_project.py` | 根据 YAML 脚手架生成 Rust 项目 | `python scripts/generate_project.py <yaml_dir> <output_dir>` |
| `add_comments.py` | 为 Rust 代码自动添加中文注释 | `python scripts/add_comments.py` |
| `clean_mojibake.py` | 修复代码中的中文乱码问题 | `python scripts/clean_mojibake.py` |
| `record_benchmark.ps1` | 运行并记录基准测试结果 | `powershell -File scripts/record_benchmark.ps1 -Message "优化后"` |
| `run_pipeline_benchmark.ps1` | 运行流水线性能基准测试 | `powershell -File scripts/run_pipeline_benchmark.ps1` |
| `update_vcs_task_ratios.ps1` | 更新 VCS 任务文件中的压缩比数据 | `powershell -File scripts/update_vcs_task_ratios.ps1` |
| `vcs_move.ps1` | 将 VCS 案例文件移动到对应插件目录 | `powershell -File scripts/vcs_move.ps1` |
| `test_cli.sh` | 简单的 CLI 压缩/解压缩测试 | `bash scripts/test_cli.sh` |

## 已归档（tmp/archive/）

以下脚本已从 `scripts/` 移至 `tmp/archive/`，不再维护：

| 脚本 | 归档原因 |
|------|----------|
| `audit_case_metrics.ps1` | Python 版 `audit_case_metrics.py` 是主力，PS1 版为独立实现易漂移 |
| `audit_all_case_metrics.ps1` | 同上，Python 版 `audit_all_case_metrics.py` 已覆盖 |
| `generate_plugin_capability_index.ps1` | 同上，Python 版已覆盖 |
| `generate_plugin_alignment_report.ps1` | 同上，Python 版已覆盖 |
| `split_tasks.py` | 一次性工具，硬编码路径，VCS_TASKS.md 已不存在 |
| `fill_dispatch_chain.py` | 一次性补丁；`fill_case_sidecars.py` 的 prompt 已包含 `expected_dispatch_chain` 字段 |

## 提示词模板

路径：`scripts/prompts/audit/`

- `case_metrics/` — audit_case_metrics.py 的 LLM 提示词
- `case_quality/` — audit_sample_case_quality.py 的 LLM 提示词（按类型分：default、vcs、build、access_log、error_trace、data_struct、shell）
- 改动提示词不需要改 Python 代码
- 新增审计目标只新建 `<type>.md` 一个文件
