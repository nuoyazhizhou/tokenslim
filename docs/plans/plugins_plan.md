# Plugins 插件层分计划

> **父计划**: [CODE_COMMENT_PLAN.md](./CODE_COMMENT_PLAN.md)  
> **执行前必须读取**: CODE_COMMENT_PLAN.md + 本文件  
> **范围**: `src/plugins/` 目录  
> **依赖**: utils 层 + core 层  
> **状态**: 进行中（部分文件已注释，文件级状态以本表勾选为准）

---

## 一、 本层概述

Plugins 层是 TokenSlim 的插件体系，包含 50+ 个插件，覆盖日志、构建、VCS、结构化数据等多个领域。每个插件通常包含 `types.rs`（类型定义）、`methods.rs`（实现）、`mod.rs`（模块导出）、`test.rs`（测试）、`showcase.rs`（示例）。

**执行顺序（按依赖关系，自底向上）**:

```
第 0 组：公共基础
  infra_tools_common.rs → test_utils.rs → mod.rs

第 1 组：基础插件（简单文本/格式处理）
  ansi_cleaner_plugin → generic_text_plugin → static_rule_plugin
  → markdown_plugin → xml_html_plugin → yaml_plugin → json_plugin
  → ndjson_plugin → protobuf_plugin → sql_plugin

第 2 组：日志类插件
  syslog_plugin → web_log_plugin → cloud_log_plugin → db_log_plugin
  → ci_log_plugin → gcc_log_plugin → xcode_log_plugin
  → node_error_plugin → java_stack_plugin → python_traceback_plugin

第 3 组：构建/工具类插件
  maven_plugin → gradle/android_gradle_plugin → bazel_plugin
  → dotnet_plugin → rust_go_plugin → nodejs_plugin
  → pytest_plugin → webpack_vite_plugin

第 4 组：智能/优化类插件
  smart_code_plugin → smart_path_plugin → noise_filter_plugin
  → spring_boot_plugin → helm_plugin → terraform_plugin
  → pulumi_plugin → cloudformation_plugin → ansible_plugin
  → unity_unreal_plugin → php_ruby_plugin → kubernetes_docker_plugin
  → artifact_summary_plugin → template_driven_plugin
  → shell_session_plugin → git_diff_plugin

第 5 组：VCS 插件族（最复杂）
  vcs_plugin（核心基类）→ vcs_git_plugin → vcs_gh_plugin
  → vcs_glab_plugin → vcs_bzr_plugin → vcs_hg_plugin
  → vcs_svn_plugin → vcs_cvs_plugin → vcs_p4_plugin
  → vcs_darcs_plugin → vcs_fossil_plugin → vcs_gerrit_plugin
  → vcs_az_plugin → vcs_bitbucket_plugin → vcs_repo_plugin
```

---

## 二、 插件分组与任务状态

### 第 0 组：公共基础

| 序号 | 插件/文件 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|----------|-----------|------|---------|------|
| 0.1 | `infra_tools_common.rs` | ~10 | 待开始 | - | 基础设施工具 |
| 0.2 | `test_utils.rs` | ~10 | 待开始 | - | 测试工具 |
| 0.3 | `mod.rs` | ~5 | 待开始 | - | 插件层模块导出 |

### 第 1 组：基础插件

| 序号 | 插件名 | 文件数 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|--------|--------|-----------|------|---------|------|
| 1.1 | ansi_cleaner_plugin | 5 | ~15 | 待开始 | - | ANSI 清理 |
| 1.2 | generic_text_plugin | 5 | ~15 | 待开始 | - | 通用文本 |
| 1.3 | static_rule_plugin | 5 | ~20 | 待开始 | - | 静态规则 |
| 1.4 | markdown_plugin | 5 | ~15 | 待开始 | - | Markdown |
| 1.5 | xml_html_plugin | 5 | ~20 | 待开始 | - | XML/HTML |
| 1.6 | yaml_plugin | 5 | ~20 | 待开始 | - | YAML |
| 1.7 | json_plugin | 5 | ~25 | 待开始 | - | JSON |
| 1.8 | ndjson_plugin | 5 | ~15 | 待开始 | - | NDJSON |
| 1.9 | protobuf_plugin | 5 | ~15 | 待开始 | - | Protobuf |
| 1.10 | sql_plugin | 5 | ~25 | 待开始 | - | SQL |

**第 1 组合计**: 10 个插件，50 个文件，约 185 函数

### 第 2 组：日志类插件

| 序号 | 插件名 | 文件数 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|--------|--------|-----------|------|---------|------|
| 2.1 | syslog_plugin | 5 | ~15 | 待开始 | - | Syslog |
| 2.2 | web_log_plugin | 5 | ~20 | 待开始 | - | Web 日志 |
| 2.3 | cloud_log_plugin | 5 | ~20 | 待开始 | - | 云日志 |
| 2.4 | db_log_plugin | 5 | ~20 | 待开始 | - | 数据库日志 |
| 2.5 | ci_log_plugin | 5 | ~20 | 待开始 | - | CI 日志 |
| 2.6 | gcc_log_plugin | 5 | ~25 | 待开始 | - | GCC 日志 |
| 2.7 | xcode_log_plugin | 5 | ~20 | 待开始 | - | Xcode 日志 |
| 2.8 | node_error_plugin | 5 | ~20 | 待开始 | - | Node.js 错误 |
| 2.9 | java_stack_plugin | 5 | ~25 | 待开始 | - | Java 栈 |
| 2.10 | python_traceback_plugin | 5 | ~20 | 待开始 | - | Python Traceback |

**第 2 组合计**: 10 个插件，50 个文件，约 205 函数

### 第 3 组：构建/工具类插件

| 序号 | 插件名 | 文件数 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|--------|--------|-----------|------|---------|------|
| 3.1 | maven_plugin | 5 | ~20 | 待开始 | - | Maven |
| 3.2 | android_gradle_plugin | 5 | ~20 | 待开始 | - | Android Gradle |
| 3.3 | bazel_plugin | 5 | ~20 | 待开始 | - | Bazel |
| 3.4 | dotnet_plugin | 5 | ~20 | 待开始 | - | .NET |
| 3.5 | rust_go_plugin | 5 | ~15 | 待开始 | - | Rust/Go |
| 3.6 | nodejs_plugin | 5 | ~20 | 待开始 | - | Node.js |
| 3.7 | pytest_plugin | 5 | ~20 | 待开始 | - | Pytest |
| 3.8 | webpack_vite_plugin | 5 | ~20 | 待开始 | - | Webpack/Vite |

**第 3 组合计**: 8 个插件，40 个文件，约 155 函数

### 第 4 组：智能/优化类插件

| 序号 | 插件名 | 文件数 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|--------|--------|-----------|------|---------|------|
| 4.1 | smart_code_plugin | 5 | ~25 | 待开始 | - | 智能代码 |
| 4.2 | smart_path_plugin | 5 | ~25 | 待开始 | - | 智能路径 |
| 4.3 | noise_filter_plugin | 5 | ~20 | 待开始 | - | 噪声过滤 |
| 4.4 | spring_boot_plugin | 5 | ~20 | 待开始 | - | Spring Boot |
| 4.5 | helm_plugin | 5 | ~15 | 待开始 | - | Helm |
| 4.6 | terraform_plugin | 5 | ~15 | 待开始 | - | Terraform |
| 4.7 | pulumi_plugin | 5 | ~15 | 待开始 | - | Pulumi |
| 4.8 | cloudformation_plugin | 5 | ~15 | 待开始 | - | CloudFormation |
| 4.9 | ansible_plugin | 5 | ~15 | 待开始 | - | Ansible |
| 4.10 | unity_unreal_plugin | 5 | ~15 | 待开始 | - | Unity/Unreal |
| 4.11 | php_ruby_plugin | 5 | ~15 | 待开始 | - | PHP/Ruby |
| 4.12 | kubernetes_docker_plugin | 5 | ~20 | 待开始 | - | K8s/Docker |
| 4.13 | artifact_summary_plugin | 5 | ~15 | 待开始 | - | 产物摘要 |
| 4.14 | template_driven_plugin | 5 | ~20 | 待开始 | - | 模板驱动 |
| 4.15 | shell_session_plugin | 4 | ~15 | 待开始 | - | Shell 会话 |
| 4.16 | git_diff_plugin | 5 | ~20 | 待开始 | - | Git Diff |

**第 4 组合计**: 16 个插件，79 个文件，约 280 函数

### 第 5 组：VCS 插件族（最复杂）

| 序号 | 插件名 | 文件数 | 预估函数数 | 状态 | 完成时间 | 备注 |
|------|--------|--------|-----------|------|---------|------|
| 5.0 | vcs_plugin（核心基类） | ~10 | ~80 | 待开始 | - | **核心基类，最复杂** |
| 5.1 | vcs_git_plugin | 5 | ~25 | 待开始 | - | Git |
| 5.2 | vcs_gh_plugin | 5 | ~20 | 待开始 | - | GitHub |
| 5.3 | vcs_glab_plugin | 5 | ~20 | 待开始 | - | GitLab |
| 5.4 | vcs_bzr_plugin | 5 | ~20 | 待开始 | - | Bazaar |
| 5.5 | vcs_hg_plugin | 5 | ~20 | 待开始 | - | Mercurial |
| 5.6 | vcs_svn_plugin | 5 | ~25 | 待开始 | - | SVN |
| 5.7 | vcs_cvs_plugin | 5 | ~20 | 待开始 | - | CVS |
| 5.8 | vcs_p4_plugin | 5 | ~20 | 待开始 | - | Perforce |
| 5.9 | vcs_darcs_plugin | 5 | ~20 | 待开始 | - | Darcs |
| 5.10 | vcs_fossil_plugin | 5 | ~20 | 待开始 | - | Fossil |
| 5.11 | vcs_gerrit_plugin | 5 | ~20 | 待开始 | - | Gerrit |
| 5.12 | vcs_az_plugin | 5 | ~20 | 待开始 | - | Azure DevOps |
| 5.13 | vcs_bitbucket_plugin | 5 | ~20 | 待开始 | - | Bitbucket |
| 5.14 | vcs_repo_plugin | 5 | ~25 | 待开始 | - | VCS Repo |

**第 5 组合计**: 15 个插件，约 75 个文件，约 335 函数

---

## 三、 总计

| 组 | 插件数 | 文件数 | 预估函数数 |
|----|--------|--------|-----------|
| 第 0 组 | - | 3 | ~25 |
| 第 1 组 | 10 | 50 | ~185 |
| 第 2 组 | 10 | 50 | ~205 |
| 第 3 组 | 8 | 40 | ~155 |
| 第 4 组 | 16 | 79 | ~280 |
| 第 5 组 | 15 | ~75 | ~335 |
| **总计** | **59** | **~297** | **~1185** |

---

## 四、 重点注意事项

### 4.1 vcs_plugin 核心基类

`src/plugins/vcs_plugin/` 是所有 VCS 插件的基类，最复杂：
- 包含 `ir.rs`（中间表示）
- 包含 `parser.rs` + `parser/helpers.rs`（解析器）
- 包含 `rule_engine.rs`（规则引擎）
- 包含 `methods/` 目录（多个实现文件）
- 必须第一个处理，后续 VCS 插件都依赖它

### 4.2 插件模板化

大部分插件结构相似（types.rs / methods.rs / mod.rs / test.rs / showcase.rs），但**绝对不能批量生成注释**。每个插件的业务逻辑不同，必须逐个分析。

### 4.3 单个插件处理流程

每个插件按以下顺序处理：
```
types.rs → methods.rs → mod.rs → test.rs → showcase.rs
```

---

## 五、 执行步骤

### 步骤 1：读取总计划 + 本分计划
- [ ] 读取 `docs/plans/CODE_COMMENT_PLAN.md`
- [ ] 读取本文件 `docs/plans/plugins_plan.md`

### 步骤 2-6：按组执行
- [ ] 第 0 组：公共基础
- [ ] 第 1 组：基础插件（10 个）
- [ ] 第 2 组：日志类插件（10 个）
- [ ] 第 3 组：构建/工具类插件（8 个）
- [ ] 第 4 组：智能/优化类插件（16 个）
- [ ] 第 5 组：VCS 插件族（15 个）

每组完成后：
- [ ] 运行 `tokenslim run cargo check`
- [ ] 更新本文件中的状态
- [ ] 记录问题清单

### 步骤 7：最终验证
- [ ] 全量 `tokenslim run cargo check`
- [ ] 抽查 10 个插件，检查注释质量
- [ ] 问题清单汇总

### 步骤 8：收口
- [ ] 更新本文件顶部状态为「已完成」
- [ ] 在总计划中标记本分计划为完成

---

## 六、 问题清单

（执行过程中发现的问题记录在此）

---

## 七、 完成标准

- [ ] 约 59 个插件、297 个文件全部处理完毕
- [ ] 所有 `pub fn` / `pub(crate) fn` / `fn` 都有中文注释
- [ ] 所有 `struct` / `enum` / `trait` 都有中文注释
- [ ] `cargo check` 通过
- [ ] 问题清单已记录
