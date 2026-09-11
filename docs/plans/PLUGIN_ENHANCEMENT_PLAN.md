# 插件增强计划 - 统一质量门控体系

## 目标
为所有新增插件和已有插件的增强建立统一的质量门控体系，确保：
1. 每个插件都有完整的测试用例（samples 目录）
2. 每个插件都有单元测试（tests.rs）
3. 每个插件都有压缩效果报告（showcase.rs）
4. 所有测试用例都经过审计和冻结

## 质量门控标准（参考 vcs_git_plugin）

### 1. 测试用例结构
```
samples/<plugin_name>/
├── case_001_<scenario>.log
├── case_002_<scenario>.log
└── ...
```

### 2. 测试代码结构
```rust
// tests.rs
#[cfg(test)]
mod tests {
    fn read_case(name: &str) -> String {
        // 从 samples/<plugin_name>/<name>.log 读取
    }
    
    #[test]
    fn test_case_001() {
        let raw = read_case("case_001_xxx");
        let compressed = compress_function(&raw);
        // 断言验证
    }
}
```

### 3. Showcase 结构
```rust
// showcase.rs
#[cfg(test)]
mod tests {
    #[test]
    fn generate_showcase_report() {
        // 遍历所有 cases
        // 生成压缩报告到 target/<plugin_name>_showcase_report.txt
    }
}
```

### 4. 审计流程
```bash
# 1. 运行 showcase 生成报告
cargo test --package tokenslim --lib plugins::<plugin_name>::showcase::tests::generate_showcase_report

# 2. 运行审计脚本
powershell -File scripts/audit_case_metrics.ps1 -Plugin <plugin_name> -Version v1_r1

# 3. 审计每个 case
powershell -File scripts/audit_case_metrics.ps1 -Plugin <plugin_name> -Version v1_r1 -CaseId case_001

# 4. 冻结通过的 case
powershell -File scripts/audit_case_metrics.ps1 -Plugin <plugin_name> -Version v1_r1 -FreezeCase case_001
```

## 需要增强的插件清单

### 第一批：构建工具类插件（高优先级）

#### 1. gcc_log_plugin ✅ 已完成
**当前状态**：
- ✅ 已有 17 个 cases（001-017）
- ✅ ErrorClassifier 集成完成（BuildStats 结构体）
- ✅ 重复警告折叠（阈值：3）
- ✅ 构建摘要生成（[SUMMARY] N errors, M warnings）
- ✅ 链接器输出压缩（$LD 标记）
- ✅ CMake configure/generate 支持（case_016）
- ✅ Ninja progress 支持（case_017）
- ✅ tests.rs 和 showcase.rs 已更新
- ✅ 审计流程已完成

**完成日期**: 2026-05-12

#### 2. maven_plugin ✅ 已完成
**当前状态**：
- ✅ v3 增强完成（2026-05-12）
- ✅ 错误/警告分类
- ✅ 依赖冲突检测
- ✅ 测试失败摘要
- ✅ 编译错误折叠
- ✅ 完整测试用例和 showcase
- ✅ 压缩率 +35-40%

**完成报告**: `docs/reports/plugin_enhancement/MAVEN_V3_SUMMARY.md`

#### 3. nodejs_plugin / node_error_plugin ✅ 已完成
**当前状态**：
- ✅ v2 增强完成（2026-05-12）
- ✅ npm/yarn 错误分类
- ✅ 依赖警告折叠
- ✅ 构建失败摘要
- ✅ 堆栈跟踪压缩
- ✅ 完整测试用例和 showcase
- ✅ 压缩率 +25%

**完成报告**: `docs/reports/plugin_enhancement/NODEJS_V2_SUMMARY.md`

#### 4. rust_go_plugin ✅ 已完成
**当前状态**：
- ✅ v3 增强完成（2026-05-12）
- ✅ cargo/go build 错误分类
- ✅ 编译警告折叠
- ✅ 测试失败摘要
- ✅ 依赖更新压缩
- ✅ 完整测试用例和 showcase（含 case_015-018）
- ✅ 压缩率 +58.3%

**完成报告**: `docs/reports/plugin_enhancement/RUST_GO_V3_SUMMARY.md`

### 第二批：异常/堆栈插件（已完成）✅

#### 5. java_stack_plugin ✅ 已完成
**当前状态**：
- ✅ 已完成 v2 增强（2026-05-12）
- ✅ 新增 4 个功能
- ✅ 新增 4 个测试案例（case_013-016）
- ✅ 所有 16 个案例通过审计
- ✅ 平均压缩率提升 36.7%（新功能）

**增强功能**：
- 相同堆栈去重（55.4% 压缩率）
- 深层堆栈截断（41.1% 压缩率）
- 异常摘要生成（2.7% 压缩率）
- Suppressed 异常压缩（47.6% 压缩率）

**完成报告**：
- `docs/reports/plugin_enhancement/JAVA_STACK_V2_COMPLETION_REPORT.md`

#### 6. python_traceback_plugin ✅ 已完成
**当前状态**：
- ✅ 已完成 v2 增强（2026-05-12）
- ✅ 新增 4 个功能
- ✅ 新增 4 个测试案例（case_013-016）
- ✅ 所有 15 个案例通过审计
- ✅ 平均压缩率提升 39.5%（新功能）

**增强功能**：
- 相似异常去重（65.8% 压缩率）
- 深层堆栈截断（46.8% 压缩率）
- 链式异常压缩（2.0% 压缩率）
- 异常摘要生成（43.5% 压缩率）

**完成报告**：
- `docs/reports/plugin_enhancement/PYTHON_TRACEBACK_V2_COMPLETION_REPORT.md`

### 第三批：其他需要增强的插件

#### 6. dotnet_plugin
**增强点**：
- MSBuild 错误分类
- NuGet 依赖警告
- 测试失败摘要

#### 7. xcode_log_plugin
**增强点**：
- Xcode 编译错误分类
- 链接器警告折叠
- 构建摘要

#### 8. android_gradle_plugin
**增强点**：
- Gradle 错误分类
- 依赖冲突检测
- 构建摘要

#### 9. spring_boot_plugin
**增强点**：
- Spring Boot 启动错误分类
- Bean 冲突检测
- 异常堆栈压缩

## 实施时间表 ✅ 全部完成

| 批次 | 插件 | 完成日期 | 状态 |
|------|------|---------|------|
| Week 1 | gcc_log_plugin | 2026-05-12 | ✅ |
| Week 2 | maven_plugin | 2026-05-12 | ✅ |
| Week 3 | nodejs_plugin / node_error_plugin | 2026-05-12 | ✅ |
| Week 4 | rust_go_plugin | 2026-05-12 | ✅ |
| 第二批 | java_stack / python_stack / android_gradle / spring_boot | 2026-05-12 | ✅ |

## 审计脚本状态

`audit_case_metrics.ps1` 已扩展为通用插件审计工具，支持 VCS 与 non-VCS 插件的版本快照、case 镜像、语义门禁和冻结管理。

### 新增参数
```powershell
-Plugin <plugin_name>  # 插件名称，默认 vcs_git
-ReportFile <path>     # showcase 报告路径，默认 target/<plugin>_showcase_report.txt
```

### 目录结构
```
docs/audit/<plugin>/
├── <plugin>.<version>.json
├── <plugin>.<version>.csv
├── <plugin>.<version>.diff.md
├── latest.json
├── frozen_cases.json
├── audit_state.json
└── cases/
    └── case_XXX/
        ├── original.txt
        ├── compact.txt
        └── summary.json
```

## 成功标准

每个插件必须满足：
1. ✅ 通常至少 12 个 showcase/test case 覆盖主要场景；窄插件需在计划中说明豁免理由
2. ✅ 所有测试用例通过单元测试
3. ✅ Showcase 报告生成 `target/*_compact_showcase_report.txt`
4. ✅ 所有 cases 经过审计并冻结
5. ✅ 审计状态为 `frozen` 或 `waived`（含理由）

## 收口状态

1. **项目完成** ✅：所有 10 个既有插件增强已完成
2. **新增插件收敛** ✅：Terraform/Ansible/Pulumi/CloudFormation/Helm/Bazel/Protobuf 已完成
3. **补充增强收敛** ✅：ndjson、db_log、gcc_log、android_gradle、pytest、rust_go、maven 已完成
4. **文档治理** ✅：本计划已移动到 `docs/plans/`，根目录只保留入口文档
5. **下一阶段建议**：优先推进 run 路由可解释、能力索引可视化和高频插件深挖；审计自动化、CI/CD 日志场景评估、cloud_log v2、web_log v3 access IR 已完成并冻结

## 参考文档

- 压缩协议：`../../CLAUDE.md` § Compression Protocol V1
- 审计流程：`../../CLAUDE.md` § 审计流程约束
- VCS Git 实现：`../../src/plugins/vcs_git_plugin/`
- 审计脚本：`../../scripts/audit_case_metrics.ps1`
> Historical plan note (updated 2026-05-14): this document is retained as a legacy plugin quality-gate template and completion narrative.
> It is not the active task board for current delivery gating.
> Current authoritative status must be read from:
> `docs/audit/audit_health.md`, `docs/audit/audit_index.json`, `docs/reports/plugin_capability_matrix.md`, `docs/reports/P0_P3_DELIVERY_BASELINE.md`, and `docs/reports/DELIVERY_GOVERNANCE_REPORT.md`.
