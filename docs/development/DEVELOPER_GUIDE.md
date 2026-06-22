# TokenSlim 开发手册

> 面向接手项目的开发者。  
> 最后更新: 2026-05-13

---

## 一、接手顺序

1. 读 `README.md`，确认 TokenSlim 的用户入口和核心命令。
2. 读 `CLAUDE.md`，确认 Compression Protocol V1、审计流程和文档同步硬约束。
3. 读 `DOCS_ORGANIZATION.md`，确认文档和脚本应该放在哪里。
4. 读 `docs/development/ARCHITECTURE.md`，理解核心模块、CLI、插件和 Sidecar。
5. 读 `docs/design/PLUGIN_DEVELOPMENT_GUIDE.md`，再开始新增或增强插件。
6. 做真实样本扩充前，先读 `docs/development/TARGETED_EDGE_SAMPLE_PLAYBOOK.md`，按定向边界流程执行。

---

## 二、开发命令

所有项目命令优先走 `tokenslim run`：

```powershell
tokenslim run cargo check
tokenslim run cargo test --lib
tokenslim run cargo test --lib <plugin_name>
tokenslim run cargo clippy
```

调试时可以打开 tracing：

```powershell
$env:RUST_LOG="debug"
tokenslim run cargo test --lib <test_name> -- --nocapture
```

---

## 三、代码结构

| 路径 | 职责 |
| ---- | ---- |
| `src/main.rs` | CLI 二进制入口 |
| `src/bin/tokenslim-server.rs` | REST API Sidecar |
| `src/cli/` | 参数解析、run 包装、路由插件链 |
| `src/core/` | 流读取、压缩管线、字典、路径优化、追踪、诊断 |
| `src/plugins/` | 55 个插件目录和共享插件辅助 |
| `config/plugins/` | 插件配置和 run route 配置 |
| `samples/` | 插件 showcase/test 样本 |
| `scripts/` | 审计、基准、维护和脚手架脚本 |
| `docs/audit/` | 审计快照、case 镜像、冻结状态 |

---

## 四、插件开发流程

新增或增强插件时，按这个顺序收口：

0. 先用 `docs/development/TARGETED_EDGE_SAMPLE_PLAYBOOK.md` 判断这是“定向边界补强”还是“普通样本补充”。
1. 在 `samples/<plugin>_plugin/` 添加真实样本，不要在测试中手写长输入。
2. 更新 `src/plugins/<plugin>_plugin/showcase.rs`，保证每个 case 进入 showcase report。
3. 更新 `src/plugins/<plugin>_plugin/test.rs`，覆盖检测、压缩、ROI 和关键语义。
4. 在 `methods.rs` 中实现压缩逻辑，并用 `prefer_non_expanding(raw, compacted)` 做 ROI 门控。
5. 如涉及 `tokenslim run`，更新 `config/plugins/*.route.json`，用 `tokenslim run --explain-route <cmd>` 验证路由，并添加路由测试。
6. 生成 `target/*_compact_showcase_report.txt`。
7. 运行 `scripts/audit_case_metrics.ps1`，确认 `regressed=0`、`frozen_changed=0`。
8. 通过语义门禁后冻结 case。
9. 同步更新计划、报告、审计总览和 README。

---

## 五、审计命令

单插件审计：

```powershell
tokenslim run powershell -File scripts/audit_case_metrics.ps1 -Plugin <plugin> -Version <version> -FailOnRegression -FailOnFrozenChange
```

单插件审计并启用语义门禁：

```powershell
tokenslim run powershell -File scripts/audit_case_metrics.ps1 -Plugin <plugin> -Version <version> -FailOnRegression -FailOnFrozenChange -RequireSemanticGate
```

单 case 导出：

```powershell
tokenslim run powershell -File scripts/audit_case_metrics.ps1 -Plugin <plugin> -Version <version> -CaseId case_XXX
```

冻结 case：

```powershell
tokenslim run powershell -File scripts/audit_case_metrics.ps1 -Plugin <plugin> -Version <version> -FreezeCase case_XXX -RequireSemanticGate
```

全插件审计健康检查：

```powershell
tokenslim run powershell -File scripts/audit_all_case_metrics.ps1 -Version <version> -RequireSemanticGate -FailOnRegression -FailOnFrozenChange -FailOnAnyFailure
```

不要并行运行同一插件的多个 `audit_case_metrics.ps1` 进程；它们会写同一组 `frozen_cases.json` / `audit_state.json`。多插件审计统一使用 `audit_all_case_metrics.ps1`。

全局产物：

- `docs/audit/audit_index.json`
- `docs/audit/audit_health.md`

全局审计失败时，先看 `audit_health.md` 的失败插件表；LLM 只需要分析失败插件的 case 镜像，不需要通读全部通过 case。

---

## 六、贡献流程

贡献前请先阅读 [CONTRIBUTING.md](../../CONTRIBUTING.md) 和 [`PLUGIN_DEVELOPMENT.md`](./PLUGIN_DEVELOPMENT.md) / [`TESTING.md`](./TESTING.md)。
