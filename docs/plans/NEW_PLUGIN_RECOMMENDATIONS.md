# TokenSlim 新插件需求分析

> Historical snapshot note (updated 2026-05-15): this document keeps phase-time next-step and pending language for that checkpoint.
> It is not an active task source for current delivery.
> Current authoritative status must be read from:
> docs/audit/audit_health.md, docs/audit/audit_index.json, docs/reports/P0_P3_DELIVERY_BASELINE.md, and docs/reports/DELIVERY_GOVERNANCE_REPORT.md.


> 生成时间: 2026-05-11  
> 最后更新: 2026-05-13  
> 目的: 分析是否需要新增插件，基于生态系统覆盖度和实际使用场景

---

## 2026-05-13 完成状态补充

本节为增量补充，用于保留 2026-05-12 原始分析正文的同时记录本轮完成状态。下方“2026-05-12 原始分析正文”仍保留原来的优先级评估、覆盖度分析和工作量估算。

### 已完成插件

| 优先级 | 插件名称 | 场景 | 状态 | Case | 审计状态 |
| ------ | -------- | ---- | ---- | ---- | -------- |
| 高 | `terraform_plugin` | IaC | ✅ 已实现 | 12 | ✅ frozen=12, regressed=0 |
| 高 | `ansible_plugin` | 配置管理 | ✅ 已实现 | 12 | ✅ frozen=12, regressed=0 |
| 中 | `pulumi_plugin` | IaC | ✅ 已实现 | 12 | ✅ frozen=12, regressed=0 |
| 中 | `cloudformation_plugin` | AWS IaC | ✅ 已实现 | 12 | ✅ frozen=12, regressed=0 |
| 中 | `helm_plugin` | Kubernetes 包管理 | ✅ 已实现 | 12 | ✅ frozen=12, regressed=0 |
| 中 | `bazel_plugin` | 大型项目构建 | ✅ 已实现 | 12 | ✅ frozen=12, regressed=0 |
| 低 | `protobuf_plugin` | 协议定义 | ✅ 已实现 | 12 | ✅ frozen=12, regressed=0 |

本轮共新增 7 个插件，每个插件 12 个 showcase/test case，总计 84 个 case。所有新增插件均已生成 `target/*_compact_showcase_report.txt`，并通过 `scripts/audit_case_metrics.ps1` 完成压缩前后 case 镜像导出、semantic gate 审计与冻结。

最终审计版本：`v1_r4_opt_final`

| 插件 | cases | regressed | frozen_changed | state_frozen |
| ---- | ----- | --------- | -------------- | ------------ |
| terraform | 12 | 0 | 0 | 12 |
| ansible | 12 | 0 | 0 | 12 |
| pulumi | 12 | 0 | 0 | 12 |
| cloudformation | 12 | 0 | 0 | 12 |
| helm | 12 | 0 | 0 | 12 |
| bazel | 12 | 0 | 0 | 12 |
| protobuf | 12 | 0 | 0 | 12 |

### 2026-05-13 缺口收敛开发计划与结果

本节记录第二批缺口核对后的执行计划和完成结果。原则：能自然归入已有插件的场景优先增强/泛化已有插件；只有语义边界明显独立时才新增插件。

| 顺序 | 工作项 | 处理策略 | 状态 | 审计结果 |
| ---- | ------ | -------- | ---- | -------- |
| 1 | 校验 `config/plugins/*.json` | 逐个 JSON 解析校验 | ✅ 已完成 | 配置文件可解析 |
| 2 | `ndjson_plugin` 样本化 | 补 `samples/ndjson_plugin`，移除测试内联样本 | ✅ 已完成 | 12 case, frozen=12, regressed=0 |
| 3 | MongoDB/Redis/PostgreSQL 专用日志 | 增强 `db_log_plugin`，不新建数据库子插件 | ✅ 已完成 | 14 case, frozen=14, regressed=0 |
| 4 | CMake/Ninja | 增强 `gcc_log_plugin`，作为构建日志子场景 | ✅ 已完成 | 17 case, frozen=17, regressed=0 |
| 5 | 通用 Gradle | 泛化 `android_gradle_plugin`，覆盖 task/download/daemon failure | ✅ 已完成 | 14 case, frozen=14, regressed=0 |
| 6 | pytest | 新增轻量 `pytest_plugin`，因其属于测试框架语义而非 traceback 子场景 | ✅ 已完成 | 12 case, frozen=12, regressed=0 |

补充说明：

- `pytest_plugin` 已新增并注册到 CLI 与 `build_plugin.route.json`。
- `gradle_plugin` 未单独新增；通用 Gradle 输出已合并增强到 `android_gradle_plugin`。
- `cmake_plugin` 和 `ninja_plugin` 未单独新增；CMake/Ninja 输出已合并增强到 `gcc_log_plugin`。
- `MongoDB`、`Redis`、`PostgreSQL（专用）` 未单独新增；数据库日志语义已合并增强到 `db_log_plugin`。

### 当前剩余任务

| 任务 | 状态 | 说明 |
| ---- | ---- | ---- |
| `pytest_plugin` | ✅ 已完成 | 已新增独立插件，覆盖 pytest session/result/summary/collection error。 |
| `gradle_plugin` | ✅ 已收敛 | 不新建独立插件；已泛化 `android_gradle_plugin` 覆盖通用 Gradle。 |
| `cmake_plugin` | ✅ 已收敛 | 不新建独立插件；已增强 `gcc_log_plugin` 覆盖 CMake configure/generate。 |
| `ninja_plugin` | ✅ 已收敛 | 不新建独立插件；已增强 `gcc_log_plugin` 覆盖 Ninja 进度输出。 |
| `MongoDB`/`Redis`/`PostgreSQL（专用）` | ✅ 已收敛 | 已增强 `db_log_plugin` 覆盖 MongoDB 慢查询、Redis 事件、PostgreSQL duration/statement。 |
| `rust_go`/`maven` 新增 case 语义冻结 | ✅ 已完成 | 修复并冻结 `rust_go` case_015-018、`maven` case_016，均通过 `-RequireSemanticGate`。 |
| 路由测试 | ✅ 已完成 | `pytest/go test -json/gradle/cmake/ninja/psql/mongosh/redis-cli` 已覆盖 run 路由与插件链断言。 |
| 云厂商日志剥壳层 | ✅ 已完成 | 已增强 `cloud_log_plugin` 到 43 frozen case，覆盖 AWS/GCP/Azure/阿里云/OCI/腾讯云/华为云/Cloudflare tail/table/csv/jsonl/plain wrapper，脱壳后衔接传统日志插件链。 |
| 交付治理 | 已分流 | 当前交付范围、历史文档归档和未跟踪产物策略记录在 `docs/reports/DELIVERY_GOVERNANCE_REPORT.md`。 |
| 额外边界样本 | 可选 backlog | 可继续为新增插件补充 ANSI 彩色输出、超长 JSON、部分成功部分失败等真实边界样本；不属于当前 coverage gap。 |

### 设计合规性修正

本轮已将插件专属压缩逻辑放回各自 `methods.rs`，`src/plugins/infra_tools_common.rs` 只保留跨插件共享的小工具与 showcase report 辅助，不再承载插件专属语义压缩函数。

---

## 2026-05-12 原始分析正文

> 以下为原始分析内容，仅追加本次完成状态，不再整体替换原文。

## 执行摘要

TokenSlim 已有 **46 个插件**，覆盖了主流开发场景。**2026-05-12 更新**: 所有 10 个计划增强的插件已完成，部分新增插件需求已被现有插件覆盖。经过重新评估，建议**核心新增 4 个插件**（可选 3 个）以提升生态系统完整性。

### 推荐新增插件（2026-05-12 更新）

| 优先级 | 插件名称           | 场景         | 理由                                              | 状态                             |
| ------ | ------------------ | ------------ | ------------------------------------------------- | -------------------------------- |
| 高     | `terraform_plugin` | IaC          | Terraform 是主流 IaC 工具                         | ✅ 核心推荐                       |
| 高     | `ansible_plugin`   | 配置管理     | Ansible 输出冗长                                  | ✅ 核心推荐                       |
| 中     | `bazel_plugin`     | 大型项目     | Google 构建工具                                   | ✅ 核心推荐                       |
| 低     | `protobuf_plugin`  | 协议定义     | protoc 输出需要压缩                               | ✅ 核心推荐                       |
| 低     | `pytest_plugin`    | Python 测试  | 测试结果聚合（异常已由 python_traceback v2 覆盖） | ⚠️ 可选（部分覆盖）               |
| 低     | `gradle_plugin`    | Java/Android | Gradle 特有格式（Java 构建已由 maven v3 覆盖）    | ⚠️ 可选（部分覆盖）               |
| 低     | `cmake_plugin`     | C/C++        | CMake 配置压缩（编译器输出已由 gcc_log v2 覆盖）  | ⚠️ 可选（部分覆盖）               |
| ~~中~~ | ~~`jest_plugin`~~  | ~~JS 测试~~  | ~~Jest 输出需要专门处理~~                         | ❌ 已由 nodejs_plugin v2 完全覆盖 |

**调整说明**:
- ✅ **核心推荐**: 4 个插件（terraform, ansible, bazel, protobuf）
- ⚠️ **可选推荐**: 3 个插件（pytest, gradle, cmake）- 现有插件已部分覆盖
- ❌ **无需新增**: 1 个插件（jest）- nodejs_plugin v2 已完全覆盖

---

## 一、现有插件覆盖度分析

### 1.1 编程语言覆盖（15 种）✅ 完善

| 语言                  | 覆盖插件                         | 状态 | 增强状态                  |
| --------------------- | -------------------------------- | ---- | ------------------------- |
| Rust                  | rust_go_plugin                   | ✅    | ✅ v3 已增强（+58.3%）     |
| Go                    | rust_go_plugin                   | ✅    | ✅ v3 已增强（+58.3%）     |
| Java                  | java_stack_plugin, maven_plugin  | ✅    | ✅ v2/v3 已增强（+36-40%） |
| Python                | python_traceback_plugin          | ✅    | ✅ v2 已增强（+39.5%）     |
| JavaScript/TypeScript | nodejs_plugin, node_error_plugin | ✅    | ✅ v2 已增强（+25%）       |
| C/C++                 | gcc_log_plugin                   | ✅    | ✅ v2 已增强（+15-30%）    |
| C#                    | dotnet_plugin                    | ✅    | -                         |
| PHP                   | php_ruby_plugin                  | ✅    | -                         |
| Ruby                  | php_ruby_plugin                  | ✅    | -                         |
| Kotlin                | java_stack_plugin                | ✅    | ✅ v2 已增强（+36.7%）     |
| Swift                 | xcode_log_plugin                 | ✅    | -                         |
| Objective-C           | xcode_log_plugin                 | ✅    | -                         |
| Scala                 | maven_plugin                     | ✅    | ✅ v3 已增强（+35-40%）    |
| Clojure               | maven_plugin                     | ✅    | ✅ v3 已增强（+35-40%）    |
| Groovy                | android_gradle_plugin            | ✅    | -                         |

**结论**: 主流编程语言已全覆盖 ✅，10 个插件已完成 v2/v3 增强（2026-05-12）

---

### 1.2 构建工具覆盖（10 种）✅ 已完善（2026-05-12 更新）

| 工具          | 覆盖插件              | 状态         | 增强状态                |
| ------------- | --------------------- | ------------ | ----------------------- |
| Make          | gcc_log_plugin        | ✅            | ✅ v2 已增强（+15-30%）  |
| Cargo         | rust_go_plugin        | ✅            | ✅ v3 已增强（+58.3%）   |
| Maven         | maven_plugin          | ✅            | ✅ v3 已增强（+35-40%）  |
| npm/yarn/pnpm | nodejs_plugin         | ✅            | ✅ v2 已增强（+25%）     |
| MSBuild       | dotnet_plugin         | ✅            | -                       |
| Gradle        | android_gradle_plugin | ⚠️ 仅 Android | ⚠️ 可选新增通用支持      |
| Xcode         | xcode_log_plugin      | ✅            | -                       |
| Webpack/Vite  | webpack_vite_plugin   | ✅            | ✅ v3 已增强（+28%）     |
| Spring Boot   | spring_boot_plugin    | ✅            | -                       |
| CMake         | gcc_log_plugin        | ⚠️ 部分覆盖   | ✅ v2 已增强，⚠️ 可选新增 |
| **可选新增**  | **Gradle（通用）**    | ⚠️            | maven v3 已部分覆盖     |
| **推荐新增**  | **Bazel**             | ❌            | 大型项目必备            |
| **缺失**      | **Ninja**             | ❌            | 使用率较低              |

**结论**: 主流构建工具已覆盖 ✅，Gradle 通用支持可选（maven v3 已覆盖 Java 构建），Bazel 推荐新增 ⚠️

---

### 1.3 测试框架覆盖（8 种）✅ 已完善（2026-05-12 更新）

| 框架       | 覆盖插件                | 状态        | 增强状态                                 |
| ---------- | ----------------------- | ----------- | ---------------------------------------- |
| Go test    | ndjson_plugin           | ✅           | -                                        |
| Cargo test | rust_go_plugin          | ✅ v3 已增强 | ✅ 测试输出压缩（92.6%）                  |
| JUnit      | maven_plugin            | ✅ v3 已增强 | ✅ 测试输出压缩（23.4%）                  |
| pytest     | python_traceback_plugin | ✅ v2 已增强 | ✅ 异常处理完善（+39.5%），⚠️ 可选测试聚合 |
| Jest       | nodejs_plugin           | ✅ v2 已增强 | ✅ 测试摘要功能                           |
| Mocha      | nodejs_plugin           | ✅ v2 已增强 | ✅ 测试摘要功能                           |
| XCTest     | xcode_log_plugin        | ✅           | -                                        |
| NUnit      | dotnet_plugin           | ✅           | -                                        |

**结论**: 主流测试框架已全覆盖 ✅，pytest 可选新增测试结果聚合功能（异常处理已完善）⚠️

---

### 1.4 VCS 工具覆盖（14 种）✅ 完善

| 工具         | 覆盖插件             | 状态 | 增强状态              |
| ------------ | -------------------- | ---- | --------------------- |
| Git          | vcs_git_plugin       | ✅    | ✅ v3 已增强（49.2%）  |
| SVN          | vcs_svn_plugin       | ✅    | ✅ v2 已增强（66-88%） |
| Mercurial    | vcs_hg_plugin        | ✅    | ✅ v2 已增强（38-48%） |
| Perforce     | vcs_p4_plugin        | ✅    | -                     |
| CVS          | vcs_cvs_plugin       | ✅    | -                     |
| Bazaar       | vcs_bzr_plugin       | ✅    | -                     |
| Darcs        | vcs_darcs_plugin     | ✅    | -                     |
| Fossil       | vcs_fossil_plugin    | ✅    | -                     |
| GitHub CLI   | vcs_gh_plugin        | ✅    | -                     |
| GitLab CLI   | vcs_glab_plugin      | ✅    | -                     |
| Azure DevOps | vcs_az_plugin        | ✅    | -                     |
| Bitbucket    | vcs_bitbucket_plugin | ✅    | -                     |
| Gerrit       | vcs_gerrit_plugin    | ✅    | -                     |
| Repo         | vcs_repo_plugin      | ✅    | -                     |

**结论**: VCS 工具已全覆盖 ✅，主流 VCS（Git/SVN/Hg）已完成 v2/v3 增强（2026-05-12）

---

### 1.5 DevOps/IaC 工具覆盖（3 种）❌ 严重缺口

| 工具       | 覆盖插件                 | 状态 |
| ---------- | ------------------------ | ---- |
| Docker     | kubernetes_docker_plugin | ✅    |
| Kubernetes | kubernetes_docker_plugin | ✅    |
| **缺失**   | **Terraform**            | ❌    |
| **缺失**   | **Ansible**              | ❌    |
| **缺失**   | **Pulumi**               | ❌    |
| **缺失**   | **CloudFormation**       | ❌    |
| **缺失**   | **Helm**                 | ❌    |

**结论**: 需要新增 Terraform、Ansible 插件 ❌

---

### 1.6 数据库工具覆盖（3 种）⚠️ 有缺口

| 工具          | 覆盖插件               | 状态 |
| ------------- | ---------------------- | ---- |
| SQL           | sql_plugin             | ✅    |
| Database logs | db_log_plugin          | ✅    |
| **缺失**      | **MongoDB**            | ❌    |
| **缺失**      | **Redis**              | ❌    |
| **缺失**      | **PostgreSQL（专用）** | ❌    |

**结论**: 数据库工具覆盖基本够用，可选新增 ⚠️

---

### 1.7 协议/格式工具覆盖（7 种）⚠️ 有缺口

| 工具     | 覆盖插件        | 状态 |
| -------- | --------------- | ---- |
| JSON     | json_plugin     | ✅    |
| YAML     | yaml_plugin     | ✅    |
| XML/HTML | xml_html_plugin | ✅    |
| SQL      | sql_plugin      | ✅    |
| Markdown | markdown_plugin | ✅    |
| NDJSON   | ndjson_plugin   | ✅    |
| **缺失** | **Protobuf**    | ❌    |
| **缺失** | **Thrift**      | ❌    |
| **缺失** | **Avro**        | ❌    |

**结论**: 需要新增 Protobuf 插件 ⚠️

---

## 二、推荐新增插件详细分析

### 2.1 高优先级插件（3 个）⭐⭐⭐

#### 1. terraform_plugin ⭐⭐⭐ 强烈推荐

**理由**:
- Terraform 是主流 IaC 工具（市场占有率 > 60%）
- Terraform 输出极其冗长（plan/apply 输出可达数万行）
- 大量重复的资源定义和状态信息
- 现有插件无法有效处理

**典型输出**:
```hcl
Terraform will perform the following actions:

  # aws_instance.example will be created
  + resource "aws_instance" "example" {
      + ami                          = "ami-0c55b159cbfafe1f0"
      + arn                          = (known after apply)
      + associate_public_ip_address  = (known after apply)
      + availability_zone            = (known after apply)
      ...（50+ 行属性）
    }

  # aws_security_group.example will be created
  + resource "aws_security_group" "example" {
      ...（50+ 行属性）
    }

Plan: 10 to add, 5 to change, 2 to destroy.
```

**压缩策略**:
```rust
// 1. 折叠资源属性（保留关键属性）
"+ resource \"aws_instance.example\" { ami = \"ami-xxx\", ... (45 attributes) }"

// 2. 聚合相同类型资源
"[TERRAFORM] 5 aws_instance resources (details suppressed)"

// 3. 提取 Plan 摘要
"[PLAN] 10 to add, 5 to change, 2 to destroy"

// 4. 折叠 known after apply
"(known after apply)" → "(computed)"
```

**预期压缩率**: 70-80%

**工作量**: 3-5 天

---

#### 2. ansible_plugin ⭐⭐⭐ 强烈推荐

**理由**:
- Ansible 是主流配置管理工具
- Ansible 输出包含大量重复的 task 信息
- 每个 task 都有冗长的 JSON 输出
- 现有插件无法有效处理

**典型输出**:
```yaml
PLAY [webservers] **************************************************************

TASK [Gathering Facts] *********************************************************
ok: [web1]
ok: [web2]
ok: [web3]

TASK [Install nginx] ***********************************************************
changed: [web1]
changed: [web2]
changed: [web3]

TASK [Start nginx] *************************************************************
ok: [web1]
ok: [web2]
ok: [web3]

PLAY RECAP *********************************************************************
web1                       : ok=3    changed=1    unreachable=0    failed=0
web2                       : ok=3    changed=1    unreachable=0    failed=0
web3                       : ok=3    changed=1    unreachable=0    failed=0
```

**压缩策略**:
```rust
// 1. 折叠相同状态的 task
"TASK [Gathering Facts] ok: [web1, web2, web3]"

// 2. 聚合 PLAY RECAP
"[RECAP] 3 hosts: 9 ok, 3 changed, 0 failed"

// 3. 折叠 JSON 输出
"changed: [web1] { ... (20 keys) }"

// 4. 提取失败任务
"[FAILED] Task 'Install nginx' on [web2]: error message"
```

**预期压缩率**: 60-70%

**工作量**: 3-5 天

---

#### 3. pytest_plugin ⭐⭐⭐ 强烈推荐（优先级降低为 ⭐）

**理由**:
- pytest 是 Python 主流测试框架
- pytest 输出包含大量测试详情
- ⚠️ **优先级降低**: python_traceback_plugin v2 已实现异常去重、堆栈截断、链式异常压缩、异常摘要
- pytest 输出 = 测试结果 + Python 异常，现有插件已覆盖异常部分（压缩率 +39.5%）
- 可选：仅需新增测试结果聚合功能（工作量 1-2 天）

**典型输出**:
```python
============================= test session starts ==============================
platform linux -- Python 3.9.0, pytest-7.0.0, pluggy-1.0.0
rootdir: /home/user/project
collected 120 items

tests/test_auth.py::test_login PASSED                                    [  1%]
tests/test_auth.py::test_logout PASSED                                   [  2%]
tests/test_auth.py::test_invalid_password FAILED                         [  3%]
...（117 行测试结果）

=================================== FAILURES ===================================
_______________________________ test_invalid_password __________________________

    def test_invalid_password():
>       assert login("user", "wrong") == False
E       AssertionError: assert True == False

tests/test_auth.py:42: AssertionError
=========================== short test summary info ============================
FAILED tests/test_auth.py::test_invalid_password - AssertionError: assert True == False
========================= 1 failed, 119 passed in 12.34s =======================
```

**压缩策略**:
```rust
// 1. 折叠通过的测试
"[PASSED] 119 tests (details suppressed)"

// 2. 保留失败的测试
"[FAILED] test_invalid_password: AssertionError (line 42)"

// 3. 提取测试摘要
"[PYTEST] 120 tests: 119 passed, 1 failed (12.34s)"

// 4. 折叠 fixture 输出
"[FIXTURES] 10 fixtures loaded"
```

**预期压缩率**: 70-80%

**工作量**: 2-3 天

---

### 2.2 中优先级插件（3 个）⭐⭐

#### 4. jest_plugin ⭐⭐

**理由**:
- Jest 是 JavaScript/TypeScript 主流测试框架
- Jest 输出包含大量快照和覆盖率信息
- ✅ **已由 nodejs_plugin v2 覆盖**（Jest 测试摘要功能已实现）

**典型输出**:
```
PASS  src/components/Button.test.tsx
  Button component
    ✓ renders correctly (12 ms)
    ✓ handles click events (5 ms)
    ✓ applies custom className (3 ms)

PASS  src/utils/helpers.test.ts
  Helper functions
    ✓ formatDate works correctly (2 ms)
    ✓ parseJSON handles invalid input (4 ms)

Test Suites: 2 passed, 2 total
Tests:       5 passed, 5 total
Snapshots:   0 total
Time:        2.345 s
```

**压缩策略**:
```rust
// 1. 折叠通过的测试套件
"[JEST] 2 test suites passed (5 tests)"

// 2. 保留失败的测试
"[FAILED] Button component > handles click events: Expected 1 but received 2"

// 3. 折叠快照信息
"[SNAPSHOTS] 10 snapshots: 8 passed, 2 updated"

// 4. 提取覆盖率摘要
"[COVERAGE] 85% statements, 80% branches"
```

**预期压缩率**: 65-75%

**工作量**: 2-3 天

---

#### 5. gradle_plugin ⭐⭐

**理由**:
- Gradle 是 Java/Kotlin/Android 主流构建工具
- 现有 android_gradle_plugin 仅处理 Android 场景
- 通用 Gradle 输出需要专门处理
- ⚠️ **注意**: maven_plugin v3 已增强 Java 构建场景，部分覆盖 Gradle 通用场景

**典型输出**:
```
> Task :compileJava
> Task :processResources
> Task :classes
> Task :jar
> Task :assemble
> Task :compileTestJava
> Task :processTestResources
> Task :testClasses
> Task :test

BUILD SUCCESSFUL in 12s
8 actionable tasks: 8 executed
```

**压缩策略**:
```rust
// 1. 折叠成功的 task
"[GRADLE] 8 tasks executed successfully (12s)"

// 2. 保留失败的 task
"[FAILED] Task :compileJava: error message"

// 3. 折叠依赖解析
"[DEPENDENCIES] 45 dependencies resolved"

// 4. 提取构建摘要
"[BUILD] SUCCESS: 8 tasks (12s)"
```

**预期压缩率**: 60-70%

**工作量**: 3-5 天

---

#### 6. bazel_plugin ⭐⭐

**理由**:
- Bazel 是 Google 开源的大型项目构建工具
- Bazel 输出包含大量缓存和远程执行信息
- 适合大型 monorepo 项目

**典型输出**:
```
INFO: Analyzed 120 targets (45 packages loaded, 1234 targets configured).
INFO: Found 120 targets...
[0 / 1,234] Checking cached actions
[1,234 / 1,234] 120 actions, 100 running
INFO: Elapsed time: 45.678s, Critical Path: 12.345s
INFO: 1234 processes: 1000 remote cache hit, 234 linux-sandbox.
INFO: Build completed successfully, 1234 total actions
```

**压缩策略**:
```rust
// 1. 折叠缓存信息
"[BAZEL] 1000 remote cache hits, 234 executed"

// 2. 提取构建摘要
"[BUILD] 120 targets: 1234 actions (45.678s)"

// 3. 折叠进度信息
"[PROGRESS] 1234 actions (details suppressed)"

// 4. 保留错误信息
"[ERROR] Target //foo:bar failed: error message"
```

**预期压缩率**: 65-75%

**工作量**: 3-5 天

---

### 2.3 低优先级插件（2 个）⭐

#### 7. cmake_plugin ⭐

**理由**:
- CMake 是 C/C++ 主流构建系统
- 现有 gcc_log_plugin 部分覆盖，但不充分
- CMake 配置输出冗长

**典型输出**:
```
-- The C compiler identification is GNU 11.2.0
-- The CXX compiler identification is GNU 11.2.0
-- Detecting C compiler ABI info
-- Detecting C compiler ABI info - done
-- Check for working C compiler: /usr/bin/cc - skipped
-- Detecting C compile features
-- Detecting C compile features - done
...（50+ 行检测信息）
-- Configuring done
-- Generating done
-- Build files have been written to: /build
```

**压缩策略**:
```rust
// 1. 折叠编译器检测
"[CMAKE] Compiler: GCC 11.2.0 (C/C++)"

// 2. 折叠特性检测
"[CMAKE] Features detected (details suppressed)"

// 3. 提取配置摘要
"[CMAKE] Configuration done: 50 targets"
```

**预期压缩率**: 60-70%

**工作量**: 2-3 天

---

#### 8. protobuf_plugin ⭐

**理由**:
- Protobuf 是主流 RPC 协议定义工具
- protoc 编译输出包含大量警告和错误
- 适合微服务架构项目

**典型输出**:
```
user.proto:10:5: warning: Field "email" is deprecated.
user.proto:15:5: warning: Field "phone" is deprecated.
order.proto:20:10: error: Type "User" not found.
order.proto:25:10: error: Type "Product" not found.
```

**压缩策略**:
```rust
// 1. 折叠重复警告
"[PROTOC] 10 deprecation warnings (details suppressed)"

// 2. 保留错误
"[ERROR] order.proto:20: Type 'User' not found"

// 3. 提取编译摘要
"[PROTOC] 5 files: 2 errors, 10 warnings"
```

**预期压缩率**: 50-60%

**工作量**: 1-2 天

---

## 三、不推荐新增的插件

### 3.1 覆盖度已足够

| 工具          | 理由                       |
| ------------- | -------------------------- |
| MongoDB       | 现有 db_log_plugin 已覆盖  |
| Redis         | 输出简单，不需要专用插件   |
| Nginx         | 现有 web_log_plugin 已覆盖 |
| Apache        | 现有 web_log_plugin 已覆盖 |
| Elasticsearch | 现有 json_plugin 已覆盖    |

### 3.2 使用场景较少

| 工具           | 理由                            |
| -------------- | ------------------------------- |
| Thrift         | 使用率低于 Protobuf             |
| Avro           | 使用率低于 Protobuf             |
| Pulumi         | 使用率低于 Terraform            |
| CloudFormation | AWS 专用，覆盖面窄              |
| Helm           | Kubernetes 专用，现有插件已覆盖 |

---

## 四、实施建议

### 4.1 分阶段实施

#### 阶段 1: 高优先级（1 个月内）⭐⭐⭐

1. **terraform_plugin** (3-5 天)
   - IaC 场景必备
   - 压缩率提升显著

2. **ansible_plugin** (3-5 天)
   - 配置管理场景必备
   - 输出冗长，压缩收益大

3. ~~**pytest_plugin** (2-3 天)~~
   - ⚠️ **优先级降低**: python_traceback_plugin v2 已实现异常去重、堆栈截断、链式异常压缩、异常摘要功能
   - pytest 输出主要是测试结果 + Python 异常，现有插件已覆盖异常部分
   - 可选：仅需新增测试结果聚合功能（工作量 1-2 天）

**工作量**: 6-10 天（如不含 pytest）或 8-13 天（如含 pytest）

#### 阶段 2: 中优先级（2 个月内）⭐⭐

4. ~~**jest_plugin** (2-3 天)~~
   - ✅ **已由 nodejs_plugin v2 覆盖**: 实现了 Jest 测试摘要功能
   - nodejs_plugin v2 新增功能包括 Jest 测试输出压缩
   - **不需要新增专用插件**

5. **gradle_plugin** (3-5 天)
   - Java/Kotlin 构建场景重要
   - 与 Maven 互补
   - ⚠️ **优先级降低**: maven_plugin v3 已实现 Javac 警告/错误压缩、JUnit 测试压缩、依赖下载折叠、构建摘要
   - Gradle 与 Maven 输出格式相似，现有插件已部分覆盖
   - 可选：仅需新增 Gradle 特有格式支持（工作量 2-3 天）

6. **bazel_plugin** (3-5 天)
   - 大型项目场景重要
   - Google 生态必备

**工作量**: 5-8 天（调整后）

#### 阶段 3: 低优先级（后续评估）⭐

7. **cmake_plugin** (2-3 天)
   - C/C++ 构建场景可选
   - ⚠️ **优先级降低**: gcc_log_plugin v2 已实现重复警告折叠、构建摘要、链接器输出压缩
   - CMake 输出主要是编译器调用 + 配置信息，现有插件已覆盖编译器部分
   - 可选：仅需新增 CMake 配置输出压缩（工作量 1-2 天）

8. **protobuf_plugin** (1-2 天)
   - 微服务场景可选
   - 使用频率中等

**工作量**: 2-5 天（调整后）

---

### 4.2 总体工作量（基于现有插件增强完成后的调整）

| 阶段     | 插件数量            | 工作量       | 预期收益        | 备注                                    |
| -------- | ------------------- | ------------ | --------------- | --------------------------------------- |
| 阶段 1   | 2-3                 | 6-13 天      | 压缩率 +15%     | pytest 可选（python_traceback v2 覆盖） |
| 阶段 2   | 1-2                 | 5-8 天       | 压缩率 +10%     | jest 已覆盖，gradle 可选（maven v3）    |
| 阶段 3   | 1-2                 | 2-5 天       | 压缩率 +5%      | cmake 可选（gcc_log v2）                |
| **总计** | **4-7**（原计划 8） | **13-26 天** | **压缩率 +30%** | 3 个插件已由现有增强覆盖                |

**调整说明**:
- ✅ **jest_plugin**: 已由 nodejs_plugin v2 完全覆盖，无需新增
- ⚠️ **pytest_plugin**: python_traceback_plugin v2 已覆盖异常处理，仅需测试结果聚合（可选）
- ⚠️ **gradle_plugin**: maven_plugin v3 已覆盖 Java 构建场景，仅需 Gradle 特有格式（可选）
- ⚠️ **cmake_plugin**: gcc_log_plugin v2 已覆盖编译器输出，仅需 CMake 配置压缩（可选）

---

### 4.3 与现有增强计划的协调

**✅ 现有插件增强已全部完成（2026-05-12）**

所有 10 个计划增强的插件已完成：

1. ✅ **rust_go_plugin** v3 - 压缩率 +58.3%
2. ✅ **maven_plugin** v3 - 压缩率 +35-40%
3. ✅ **gcc_log_plugin** v2 - 压缩率 +15-30%
4. ✅ **nodejs_plugin** v2 - 压缩率 +25%（含 Jest/Mocha 测试支持）
5. ✅ **vcs_svn_plugin** v2 - 压缩率 66.6%-88.4%
6. ✅ **vcs_hg_plugin** v2 - 压缩率 38.4%-47.6%
7. ✅ **vcs_git_plugin** v3 - 压缩率 49.2%（最高 73%）
8. ✅ **webpack_vite_plugin** v3 - 压缩率 +28%
9. ✅ **java_stack_plugin** v2 - 压缩率 +36.7%（新功能）
10. ✅ **python_traceback_plugin** v2 - 压缩率 +39.5%（新功能）

**建议顺序**（新增插件）:

1. **高优先级插件**（阶段 1，2 周）
   - 新增 terraform_plugin
   - 新增 ansible_plugin
   - ~~新增 pytest_plugin~~（可选，python_traceback_plugin v2 已部分覆盖）

2. **中优先级插件**（阶段 2，2 周）
   - ~~新增 jest_plugin~~（已由 nodejs_plugin v2 覆盖）
   - 新增 gradle_plugin（可选，maven_plugin v3 已部分覆盖）
   - 新增 bazel_plugin

3. **低优先级插件**（阶段 3，1 周）
   - 新增 cmake_plugin（可选，gcc_log_plugin v2 已部分覆盖）
   - 新增 protobuf_plugin

**调整后总工作量**: 5-7 周（约 1.5 个月）

---

## 五、总结

### 5.1 核心结论（2026-05-12 更新）

TokenSlim 的 **46 个插件**已覆盖主流场景，**10 个插件已完成 v2/v3 增强**（平均压缩率提升 +47-52%）。基于现有增强成果，在以下领域仍存在缺口：

| 领域         | 缺口                         | 推荐新增     | 调整说明                                       |
| ------------ | ---------------------------- | ------------ | ---------------------------------------------- |
| IaC/配置管理 | Terraform, Ansible           | ⭐⭐⭐ 强烈推荐 | 无变化                                         |
| 测试框架     | ~~pytest~~, ~~Jest~~         | ⚠️ 可选       | Jest 已覆盖，pytest 异常处理已覆盖（仅需聚合） |
| 构建工具     | ~~Gradle~~, Bazel, ~~CMake~~ | ⭐⭐ 推荐      | Gradle/CMake 已部分覆盖，Bazel 核心推荐        |
| 协议工具     | Protobuf                     | ⭐ 可选       | 无变化                                         |

**现有插件增强成果（2026-05-12）**:
- ✅ **nodejs_plugin v2**: Jest/Mocha 测试摘要（+25%）
- ✅ **python_traceback_plugin v2**: 异常去重、堆栈截断、链式异常、摘要（+39.5%）
- ✅ **maven_plugin v3**: Javac 警告、JUnit 测试、依赖下载、构建摘要（+35-40%）
- ✅ **gcc_log_plugin v2**: 重复警告、构建摘要、链接器输出（+15-30%）

### 5.2 推荐方案（基于 2026-05-12 现有插件增强完成后）

**核心推荐: 新增 4 个插件**（分 3 个阶段）
- **阶段 1**（高优先级）: terraform, ansible
- **阶段 2**（中优先级）: bazel
- **阶段 3**（低优先级）: protobuf

**可选推荐: 新增 3 个插件**（根据实际需求）
- **pytest_plugin**: 如需专门的测试结果聚合（python_traceback v2 已覆盖异常）
- **gradle_plugin**: 如需 Gradle 特有格式支持（maven v3 已覆盖 Java 构建）
- **cmake_plugin**: 如需 CMake 配置压缩（gcc_log v2 已覆盖编译器输出）

**已覆盖无需新增**:
- ~~**jest_plugin**~~: nodejs_plugin v2 已完全覆盖

### 5.3 预期收益（基于现有插件增强完成后）

**核心方案（4 个插件）**:
- **生态系统完整性**: 46 → 50 插件（+9%）
- **场景覆盖度**: 85% → 92%（+7%）
- **压缩率提升**: +25%（IaC/大型项目场景）
- **用户满意度**: +30%（覆盖 IaC 和 Bazel）

**完整方案（7 个插件，含可选）**:
- **生态系统完整性**: 46 → 53 插件（+15%）
- **场景覆盖度**: 85% → 95%（+10%）
- **压缩率提升**: +30%（IaC/测试/构建场景）
- **用户满意度**: +40%（覆盖更多工具链）

**已完成的现有插件增强收益（2026-05-12）**:
- ✅ **10 个插件增强完成**: 平均压缩率提升 +47-52%
- ✅ **290+ 测试案例**: 100% 通过审计，0 回归
- ✅ **37 个新功能**: 覆盖构建、测试、VCS、异常/堆栈场景

### 5.4 下一步行动（基于 2026-05-12 状态）

**✅ 已完成**: 所有 10 个现有插件增强（2026-05-12）

**建议行动**:

1. **评审新增插件需求**（1 天）
   - ✅ 确认 jest_plugin 已由 nodejs_plugin v2 覆盖，无需新增
   - ⚠️ 评估 pytest/gradle/cmake 是否需要专用插件（现有插件已部分覆盖）
   - ✅ 确认 terraform/ansible/bazel/protobuf 为核心新增目标

2. **实施核心新增插件计划**（3-4 周）
   - 阶段 1: terraform + ansible（2 周）
   - 阶段 2: bazel（1 周）
   - 阶段 3: protobuf（1 周）

3. **可选: 实施补充插件**（2-3 周）
   - pytest 测试结果聚合（1 周）
   - gradle 特有格式支持（1 周）
   - cmake 配置压缩（1 周）

**总工作量**: 3-7 周（核心 3-4 周，完整 5-7 周）

---

## 参考文档

- `docs/reports/plugin_enhancement/PLUGIN_ENHANCEMENT_ANALYSIS.md` - 现有插件增强分析（✅ 10/10 完成）
- `docs/archive/PLUGIN_ENHANCEMENT_FINAL_STATUS.md` - 插件增强最终状态（✅ 100% 完成）
- `docs/reports/IMPLEMENTATION_STATUS.md` - 项目实现状态
- `docs/reports/feature_implementation/RTK_TOKF_FEATURE_COMPLETION_STATUS.md` - 功能完成状态
- `docs/reports/plugin_enhancement/` - 各插件完成报告

---

## 更新历史

| 版本 | 日期       | 更新内容                                                            |
| ---- | ---------- | ------------------------------------------------------------------- |
| 1.0  | 2026-05-11 | 初始版本，分析新增插件需求                                          |
| 1.1  | 2026-05-12 | 基于现有 10 个插件增强完成，调整新增插件优先级和工作量估算          |
|      |            | - jest_plugin 已由 nodejs_plugin v2 覆盖，无需新增                  |
|      |            | - pytest/gradle/cmake 优先级降低（现有插件已部分覆盖）              |
|      |            | - 核心推荐从 8 个调整为 4 个（terraform, ansible, bazel, protobuf） |
|      |            | - 工作量从 19-31 天调整为 13-26 天                                  |
| 1.2  | 2026-05-13 | 追加本轮完成状态补充，保留 2026-05-12 原始分析正文                  |
|      |            | - 已完成 terraform/ansible/pulumi/cloudformation/helm/bazel/protobuf |
|      |            | - 每插件 12 case，总计 84 case，最终审计 regressed=0、frozen_changed=0 |
| 1.3  | 2026-05-13 | 追加缺口收敛计划与结果                                              |
|      |            | - 补齐 ndjson samples/showcase/audit                                 |
|      |            | - 增强 db_log 覆盖 MongoDB/Redis/PostgreSQL 专用场景                 |
|      |            | - 增强 gcc_log 覆盖 CMake/Ninja                                      |
|      |            | - 泛化 android_gradle 覆盖通用 Gradle                                |
|      |            | - 新增 pytest_plugin 并完成 12 case 审计冻结                         |
| 1.4  | 2026-05-13 | 收口总览审计与 run 路由集成                                          |
|      |            | - 非 VCS 总览更新为 484/484 all_pass、frozen=484                     |
|      |            | - VCS 总览更新为 328/328 all_pass、frozen=328                        |
|      |            | - 修复并冻结 rust_go/maven 漏过语义门禁的新增 case                   |
|      |            | - 补充 pytest/go/gradle/cmake/ninja/db CLI run 路由测试               |
| 1.5  | 2026-05-13 | 文档治理归位                                                        |
|      |            | - 本文件从根目录移动到 `docs/plans/`                                 |
|      |            | - 根目录只保留 README/CLAUDE/AGENTS/CODEX/context/组织规范            |
|      |            | - 参考文档链接更新到新目录                                           |
| 1.6  | 2026-05-13 | 新增通用云厂商日志剥壳层                                            |
|      |            | - 新增 `cloud_log_plugin` 并完成 12 case 审计冻结                     |
|      |            | - 全局审计更新为 55 插件、829/829 case frozen                         |
|      |            | - 明确云日志是 wrapper peel layer，脱壳后回到传统日志插件链           |
| 1.7  | 2026-05-13 | cloud_log_plugin v2 覆盖矩阵补齐                                     |
|      |            | - 扩展到 37 case，新增 OCI/腾讯云/华为云/Cloudflare 与 AWS/GCP/Azure/阿里云边界样本 |
|      |            | - 覆盖 tail/plain、table、CSV、JSONL 外壳和 Web/Java/Python/Node/DB/Syslog 内层日志 |
|      |            | - 全局审计更新为 55 插件、854/854 case frozen                         |

---

**文档版本**: 1.7
**最后更新**: 2026-05-13  
**作者**: Kiro AI Assistant / Codex update
## 2026-05-14 Artifact Summary Closure

`artifact_summary_plugin` has been added as the dedicated SARIF/JUnit XML build artifact summary layer. It is intentionally separate from `ci_log_plugin`: CI logs describe provider shells and steps, while SARIF/JUnit files are uploaded artifacts whose content needs semantic aggregation before generic JSON/XML.

Status: superseded by the 2026-05-15 baseline refresh. Current global audit: `p2_samples_gap_20260515`, 57 audited plugins, 1000 cases, 1000 frozen, 0 failures, 0 capability coverage gaps.
## 2026-05-14 Authoritative Closure

This plan is now a historical recommendation source, not the active task board. The current authoritative baseline is:

- Global audit: `p2_samples_gap_20260515`
- Audit health: 57 audited plugins, 1000 cases, 1000 frozen, 0 failures
- P0 route/capability index: complete
- P1 `cloud_log_plugin` provider-wrapper peel layer: complete, 43 frozen cases
- P2 `db_log_plugin` dedicated database diagnostics: complete, 19 frozen cases
- P3 `web_log_plugin` real access-log formats: complete, 47 frozen cases

Older counts in the original recommendation body, such as 37 cloud-log cases, 14 database-log cases, `970/970`, or `992/992`, are historical snapshots. Use `docs/audit/audit_health.md`, `docs/reports/plugin_capability_matrix.md`, `docs/reports/IMPLEMENTATION_STATUS.md`, and `docs/reports/P0_P3_DELIVERY_BASELINE.md` for current status.
