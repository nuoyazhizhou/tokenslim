# TokenSlim 文档与脚本组织规范

> 最后更新: 2026-06-11
> 目标: 让陌生开发者能从根目录入口快速找到功能点、设计、使用手册、开发手册和审计状态。

---

## 一、根目录只保留入口文档

根目录只放长期稳定入口，不放阶段计划、完成报告、单插件进度或临时分析。

| 文件                    | 角色                                                          | 是否长期保留 |
| ----------------------- | ------------------------------------------------------------- | ------------ |
| `README.md`             | 项目入口、快速使用、文档导航                                  | 是           |
| `AGENTS.md`             | **AI 规则唯一权威**（身份/红线/工具调用/流水线概览/按需索引） | 是           |
| `COMPRESSION.md`        | 压缩协议 V1 全文（被 AGENTS.md 按需引用）                     | 是           |
| `AUDIT.md`              | 变更后必跑审计流水线手册（被 AGENTS.md 按需引用）             | 是           |
| `TOOLS.md`              | 脚本目录全集（被 AGENTS.md 按需引用）                         | 是           |
| `CLAUDE.md`             | `@AGENTS.md` 指针（Claude Code 兼容，不维护内容）             | 是           |
| `CODEX.md`              | `@AGENTS.md` 指针（Codex 兼容，不维护内容）                   | 是           |
| `.tokenslim-context.md` | 自动生成的工作区上下文                                        | 是           |
| `DOCS_ORGANIZATION.md`  | 文档与脚本治理规范                                            | 是           |

当前根目录 MD 数量应保持为 9 个（AGENTS.md 为权威，COMPRESSION/AUDIT/TOOLS 为其按需引用文件，CLAUDE/CODEX 为指针）。新增任务文档必须放入 `docs/` 下对应目录。

---

## 二、docs 目录分工

| 目录                                   | 放什么                                                                | 例子                                                                                                                                                                                                                                                                                                                                                                                                            |
| -------------------------------------- | --------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `docs/guides/`                         | 面向使用者的操作手册                                                  | `USER_GUIDE.md`                                                                                                                                                                                                                                                                                                                                                                                                 |
| `docs/development/`                    | 面向开发者的接手文档、架构总览、开发流程                              | `ARCHITECTURE.md`, `DEVELOPER_GUIDE.md`, `TARGETED_EDGE_SAMPLE_PLAYBOOK.md`                                                                                                                                                                                                                                                                                                                                     |
| `docs/design/`                         | 模块设计、插件设计、系统设计                                          | `PLUGIN_DEVELOPMENT_GUIDE.md`, `vcs_plugin.md`                                                                                                                                                                                                                                                                                                                                                                  |
| `docs/plans/`                          | 当前仍有参考价值的路线图、计划、推荐清单                              | `FEATURE_ROADMAP.md`, `NEW_PLUGIN_RECOMMENDATIONS.md`                                                                                                                                                                                                                                                                                                                                                           |
| `docs/tasks/`                          | 可执行任务看板、case 修复清单                                         | `VCS_GIT_TASKS.md`                                                                                                                                                                                                                                                                                                                                                                                              |
| `docs/reports/`                        | 当前状态报告、完成报告、基准报告、能力矩阵                            | `IMPLEMENTATION_STATUS.md`, `P0_P3_DELIVERY_BASELINE.md`, `plugin_capability_matrix.md`                                                                                                                                                                                                                                                                                                                         |
| `docs/reports/plugin_enhancement/`     | plugin enhancement completion reports and evaluations                 | `PLUGIN_ENHANCEMENT_ANALYSIS.md`, `ROUTE_CAPABILITY_INDEX_P0_REPORT.md`, `CLOUD_LOG_PLUGIN_V2_COMPLETION_REPORT.md`, `CLOUD_LOG_PLUGIN_P1_WRAPPER_ALIASES_REPORT.md`, `DB_LOG_PLUGIN_P2_DEDICATED_SEMANTICS_REPORT.md`, `WEB_LOG_PLUGIN_V3_ACCESS_IR_COMPLETION_REPORT.md`, `WEB_LOG_PLUGIN_P3_REAL_FORMATS_DIAG_REPORT.md`, `CI_CD_LOG_SCENARIO_EVALUATION.md`, `ARTIFACT_SUMMARY_PLUGIN_COMPLETION_REPORT.md` |
| `docs/tasks/`                          | active implementation task boards                                     | `CI_CD_LOG_SCENARIO_TASKS.md`                                                                                                                                                                                                                                                                                                                                                                                   |
| `docs/reports/feature_implementation/` | 功能阶段实现报告                                                      | `RTK_TOKF_FEATURE_COMPLETION_STATUS.md`                                                                                                                                                                                                                                                                                                                                                                         |
| `docs/audit/`                          | 自动生成的审计快照、case 镜像、冻结状态、全局审计索引、LLM 审计提示包 | `docs/audit/<plugin>/`, `audit_index.json`, `audit_review_prompt.md`, `plugin_capability_index.json`                                                                                                                                                                                                                                                                                                            |
| `docs/prompts/`                        | LLM 执行提示词和审计提示词                                            | `non_vcs_classical_prompts.md`                                                                                                                                                                                                                                                                                                                                                                                  |
| `docs/archive/`                        | 已完成、过期或仅作历史参考的计划/会话/分析                            | `GCC_LOG_ENHANCEMENT_PLAN.md`                                                                                                                                                                                                                                                                                                                                                                                   |

---

## 三、本轮归位结果

| 原位置                                     | 新位置                                                                      | 理由                                                               |
| ------------------------------------------ | --------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `FEATURE_ROADMAP.md`                       | `docs/plans/FEATURE_ROADMAP.md`                                             | 当前功能路线图，属于计划资料                                       |
| `NEW_PLUGIN_RECOMMENDATIONS.md`            | `docs/plans/NEW_PLUGIN_RECOMMENDATIONS.md`                                  | 插件新增/收敛推荐，属于计划资料                                    |
| `PLUGIN_ENHANCEMENT_PLAN.md`               | `docs/plans/PLUGIN_ENHANCEMENT_PLAN.md`                                     | 插件质量门控计划，仍有流程参考价值                                 |
| `IMPLEMENTATION_STATUS.md`                 | `docs/reports/IMPLEMENTATION_STATUS.md`                                     | 当前实现状态，属于报告                                             |
| `P0_P3_DELIVERY_BASELINE.md`               | `docs/reports/P0_P3_DELIVERY_BASELINE.md`                                   | P0-P3 交付基线和收口清单                                           |
| `PLUGIN_ENHANCEMENT_ANALYSIS.md`           | `docs/reports/plugin_enhancement/PLUGIN_ENHANCEMENT_ANALYSIS.md`            | 插件增强综合分析，属于插件报告                                     |
| `RTK_TOKF_FEATURE_COMPLETION_STATUS.md`    | `docs/reports/feature_implementation/RTK_TOKF_FEATURE_COMPLETION_STATUS.md` | 功能完成状态，属于功能实现报告                                     |
| `GCC_LOG_ENHANCEMENT_PLAN.md`              | `docs/archive/GCC_LOG_ENHANCEMENT_PLAN.md`                                  | 单插件历史计划，已完成归档                                         |
| `docs/plans/REFACTORING_PLAN_V6.2.md`      | `docs/archive/REFACTORING_PLAN_V6.2.md`                                     | 旧重构计划，已完成归档                                             |
| `docs/plans/code_vs_design_audit.md`       | `docs/archive/code_vs_design_audit.md`                                      | 历史代码/设计对照审计                                              |
| `docs/plans/design_issues_analysis.md`     | `docs/archive/design_issues_analysis.md`                                    | 历史设计问题分析                                                   |
| `docs/reports/WORKSPACE_CLEANUP_REPORT.md` | `docs/archive/WORKSPACE_CLEANUP_REPORT.md`                                  | 旧工作区清理报告，避免误作当前状态                                 |
| `docs/tasks/VCS_COMPACTION_AUDIT.md`       | `docs/archive/VCS_COMPACTION_AUDIT.md`                                      | 旧 VCS ROI backlog，当前审计已由 `docs/audit/audit_health.md` 覆盖 |
| `docs/design/java_stack_plugin.md`         | `docs/archive/java_stack_plugin.md`                                         | 早期 Java stack 插件草案，当前插件已审计冻结                       |
| `docs/design/总体工作流程.md`              | `docs/archive/总体工作流程.md`                                              | 早期脚手架设计草稿，含过期未勾选清单                               |

---

## 四、脚本组织规则

当前 `scripts/` 保持扁平目录，因为 `CLAUDE.md`、审计提示词、任务看板和历史报告大量引用 `scripts/<name>`。迁移脚本目录前必须先提供兼容 wrapper，或一次性更新全部引用并验证。

| 脚本类别       | 当前脚本                                                                                                                                                                                                                    | 当前位置   | 迁移规则                                                                      |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- | ----------------------------------------------------------------------------- |
| 审计/冻结/洞察 | `audit_case_metrics.ps1`, `audit_all_case_metrics.ps1`, `audit_artifact_governance.ps1`, `generate_plugin_capability_index.ps1`, `generate_plugin_alignment_report.ps1`, `project_insight.py`, `update_vcs_task_ratios.ps1` | `scripts/` | 保持稳定路径；`project_insight.py` 负责配置驱动项目图景提取，不替代审计主脚本 |
| 基准测试       | `record_benchmark.ps1`, `run_pipeline_benchmark.ps1`                                                                                                                                                                        | `scripts/` | 可未来迁到 `scripts/benchmark/`，但需保留 wrapper                             |
| 构建/安装      | `build_and_install.py`                                                                                                                                                                                                      | `scripts/` | 可未来迁到 `scripts/release/`，但需保留 wrapper                               |
| 脚手架/迁移    | `generate_project.py`, `generate_vcs_config.py`, `split_tasks.py`, `vcs_move.ps1`                                                                                                                                           | `scripts/` | 可未来迁到 `scripts/migration/`，但需保留 wrapper                             |
| 维护工具       | `add_comments.py`, `clean_mojibake.py`                                                                                                                                                                                      | `scripts/` | 可未来迁到 `scripts/maintenance/`，但需保留 wrapper                           |
| 冒烟测试       | `test_cli.sh`, `release_smoke_gate.ps1`                                                                                                                                                                                     | `scripts/` | 可未来迁到 `scripts/smoke/`，但需保留 wrapper                                 |

---

## 五、移动文件安全规则

移动任何已存在文件前必须执行 git 检查：

```powershell
tokenslim run git status --short -- <path>
tokenslim run git diff -- <path>
```

移动已跟踪文件必须优先使用：

```powershell
tokenslim run git mv <old-path> <new-path>
```

移动未跟踪文件前，也必须先执行 `tokenslim run git status --short -- <path>`，确认不是用户尚未提交的重要工作；只有确认归属后才允许 `Move-Item`。

---

## 六、任务完成同步规则

为防止“代码完成但 PLAN/报告未更新”反复发生，每次完成代码、插件、审计或文档治理任务时，必须同步检查并更新：

1. 当前任务计划: `docs/plans/*.md` 或 `docs/tasks/*.md`
2. 当前实现状态: `docs/reports/IMPLEMENTATION_STATUS.md`
3. 插件/功能完成报告: `docs/reports/plugin_enhancement/` 或 `docs/reports/feature_implementation/`
4. 审计总览: `docs/audit/non_vcs_case_semantic_audit.md` 或 `docs/audit/vcs_case_semantic_audit.md`
5. 审计自动化产物: `docs/audit/audit_health.md`, `docs/audit/audit_index.json`, `docs/audit/audit_review_prompt.md`, `docs/audit/plugin_capability_index.json`
6. 用户入口: `README.md` 的文档导航和关键能力摘要

如果某计划已经完成且不再指导当前工作，应移动到 `docs/archive/`，不要留在根目录或继续作为“当前计划”。

---

## 七、陌生开发者接手入口

| 接手问题                   | 入口                                                                                             |
| -------------------------- | ------------------------------------------------------------------------------------------------ |
| 项目是什么、怎么快速使用   | `README.md`, `docs/guides/USER_GUIDE.md`                                                         |
| 当前功能完成到哪里         | `docs/reports/IMPLEMENTATION_STATUS.md`, `docs/reports/P0_P3_DELIVERY_BASELINE.md`               |
| 系统怎么设计               | `docs/development/ARCHITECTURE.md`                                                               |
| 如何开发和验证             | `docs/development/DEVELOPER_GUIDE.md`                                                            |
| 如何新增/增强插件          | `docs/design/PLUGIN_DEVELOPMENT_GUIDE.md`, `docs/plans/PLUGIN_ENHANCEMENT_PLAN.md`               |
| 插件新增建议和收敛决策     | `docs/plans/NEW_PLUGIN_RECOMMENDATIONS.md`                                                       |
| 审计是否通过               | `docs/audit/audit_health.md`, `docs/audit/audit_index.json`, `docs/audit/audit_review_prompt.md` |
| 哪个插件处理哪类日志       | `docs/reports/plugin_capability_matrix.md`, `docs/audit/plugin_capability_index.json`            |
| 本轮交付范围和历史文档清洗 | `docs/reports/DELIVERY_GOVERNANCE_REPORT.md`, `docs/reports/P0_P3_DELIVERY_BASELINE.md`          |

---

**维护者**: TokenSlim maintainers / Codex
**最后更新**: 2026-05-14
