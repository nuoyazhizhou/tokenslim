# TokenSlim 测试与质量门禁（公开版）

> 本文件面向**贡献者**：描述改完代码后必须跑什么、按什么顺序跑、产物在哪里。  
> 所有命令统一走 `tokenslim run <command>`，本地脚本入口见 `scripts/`。

---

## 1. 触发条件：什么时候必须跑这套流水线？

以下任何变更发生后，**必须按顺序**跑完 4 步并完成产物回收：

- 新增 / 删除 / 重命名 case（`samples/<plugin>/case_*`）
- 新增 / 删除 / 重命名插件（`src/plugins/<plugin>_plugin/`、`config/plugins/*.json`）
- 修改 `src/plugins/mod.rs`（增减 `pub mod`）
- 修改 showcase.rs（增减 case 注册）
- 修改插件 parser / rule / 任何影响压缩产物的代码
- 修改 `samples/<plugin>/case_NNN_*.scenario.yaml`（sidecar）

## 2. 简表：变更 → 必跑脚本

| 变更类型                  | 必跑脚本（按顺序）                    |
| ------------------------- | ------------------------------------- |
| 新增 case                 | 1 → 2 → 4 → 1（cap_index 刷新后回归） |
| 删除 case                 | 1 → 2 → 3 → 4 → 1                     |
| 新增插件                  | 1 → 3 → 4 → 1                         |
| 修改插件 parser / rule    | 2 → 3 → 4 → 1                         |
| 修改 showcase.rs          | 1 → 2 → 3 → 4 → 1                     |
| 修改 `src/plugins/mod.rs` | 1 → 3 → 4 → 1                         |
| 修改 sidecar              | 1 → 2 → 4                             |
| 批量改动（plugin 重构）   | 1 → 2 → 3 → 4 → 1（完整循环）         |

## 3. 串行依赖（不要乱）

```
1. audit_sample_case_quality.py    （先证明 case 本身合格）
  ↓
2. audit_case_metrics.py           （再证明压缩产物合格）
  ↓
3. audit_all_case_metrics.py       （最后证明全部插件 + 全局无 regression）
  ↓
4. generate_plugin_capability_index.py  （刷新能力索引，让下次审计的 KB 是新的）
  ↓
  回到 1（漂移检测会校验 cap_index.json 是否过期）
```

---

## 4. 步骤 1 — `audit_sample_case_quality.py`：物理 case 质量门禁

**目的**：证明 case 本身是"好 case"——真实、有锚点、覆盖目标命令族、case 数与 showcase.rs / mod.rs 一致。

```bash
tokenslim run python scripts/audit_sample_case_quality.py --plugin <plugin>   # 单插件
tokenslim run python scripts/audit_sample_case_quality.py --all               # 全插件
```

### 增量审计缓存（默认开启）

- 读 `case_quality_latest.json`，对每个 case 算 `sha256(content)` 查哈希索引。
- 命中 + 状态合法 → 复用结论（终端显示 `[cache] hits=N misses=M hit_rate=...`）。
- 改 case、改 lint 阈值、改 showcase.rs / mod.rs → 缓存失效，自动全量重跑。

### 回收产物

| 产物         | 路径                                                              | 必读项                            |
| ------------ | ----------------------------------------------------------------- | --------------------------------- |
| 案例质量报告 | `docs/audit/<plugin>/sample_quality/case_quality_report.md`       | `status=valid` 比例、`needs_fix` 清单 |
| 漂移报告     | `docs/audit/<plugin>/sample_quality/drift_audit_*.json`           | 7 条漂移轴的 warning / error      |
| 命令族覆盖   | `docs/audit/<plugin>/sample_quality/command_family_coverage.json` | 60 族 vs 实际覆盖数               |

### 回收后必须执行

1. 若 `status=valid` 比例 < 90% → 修正 case 后重跑，**不得带病进入步骤 2**。
2. 若 `drift_audit_*` 有任何 warning / error → 立刻修复：
   - `samples-vs-mod-rs` 多：移走 samples 目录或加 `pub mod`
   - `samples-vs-mod-rs` 少：建空 `samples/<plugin>_plugin/` 目录
   - `case-count-mismatch`：补 showcase.rs 注册或删孤儿 case
   - `sidecar-missing`：跑 `python scripts/generate_case_sidecars.py --plugin <plugin>` 补模板
   - `ghost-case`：showcase.rs 注册了但 samples 里没有，按需补文件或删注册
3. 若 `command_family_coverage` 缺失目标族 → 加 case 后从步骤 1 重跑。
4. CI 模式加 `--strict-drift` 让漂移发现时 `sys.exit(1)`。

---

## 5. 步骤 2 — `audit_case_metrics.py`：压缩语义 + 冻结门禁

**目的**：校验 `target/<plugin>_compact_showcase_report.txt` 中 original vs compact 的对齐、压缩、语义保真与冻结。

```bash
# 单插件全量回归（Windows：版本号手填当天日期，如 v20260612_r1）
tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --version vYYYYMMDD_r1 --require-semantic-gate --fail-on-regression

# 单 case 镜像导出
tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --version vX_r1 --case-id case_XXX

# 冻结新通过的 case
tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --version vX_r1 --freeze-case case_XXX --require-semantic-gate
```

### 回收产物

| 产物          | 路径                                            | 必读项                                             |
| ------------- | ----------------------------------------------- | -------------------------------------------------- |
| 快照 JSON/CSV | `docs/audit/<plugin>/<plugin>.vX_r1.{json,csv}` | 逐 case 的 Original / Compact / Compression%       |
| Diff          | `docs/audit/<plugin>/<plugin>.vX_r1.diff.md`    | `improved / regressed / unchanged / new / missing`  |
| Latest        | `docs/audit/<plugin>/<plugin>.latest.json`      | 滚动覆盖                                           |
| 冻结清单      | `docs/audit/<plugin>/frozen_cases.json`         | 哪些 case 已冻结、其 `compact_hash`                |
| 状态机        | `docs/audit/<plugin>/audit_state.json`          | `todo / auditing / frozen / waived`                |

### 回收后必须执行

1. **任何 `regressed` case** → 禁止收口；查 `case_XXX/compact.txt` 对比上一版本定位回退。
2. **任何 `frozen_changed_case`** → 状态自动回退到 `auditing`，必须重审 + 重新冻结或 `waived`。
3. **任何 `semantic_gate_failed`** → 压缩协议 V1（`docs/development/PLUGIN_DEVELOPMENT.md`）的法则之一被破坏；先修代码再重跑。
4. **新 unchanged case** → 建议 `--freeze-case case_XXX --require-semantic-gate` 锁定。
5. 任何 `new` / `missing` case → 确认是预期变更（不是误删 / 误增）。

---

## 6. 步骤 3 — `audit_all_case_metrics.py`：全插件健康检查（收口前必跑）

**目的**：聚合所有插件健康状态，输出全局健康报告。

```bash
tokenslim run python scripts/audit_all_case_metrics.py --version vYYYYMMDD_r1 --require-semantic-gate --fail-on-regression --fail-on-frozen-change --fail-on-any-failure
```

### 并发与冻结策略

- **禁止并行运行**同一插件的 `audit_case_metrics.py`（写同一份 `frozen_cases.json` / `audit_state.json`，会损坏）。
- 多插件批量审计必须用 `audit_all_case_metrics.py` 串行调度。
- `audit_sample_case_quality.py --all` 和 `audit_all_case_metrics.py` 不要同时跑（都写 `docs/audit/<plugin>/` summary）。
- 已冻结 case 若 `compression%` 与 `compact_hash` 均不变，不再人工复读。
- 脚本输出 `frozen_changed_case=<case_id>` → 该 case 必须解冻重审。
- 状态机自动维护 `audit_state.json`（`todo / auditing / frozen / waived`），被冻结 case 内容变化自动回退 `auditing`。

### 全局产物位置

| 产物             | 路径                                                                         |
| ---------------- | ---------------------------------------------------------------------------- |
| 全局索引         | `docs/audit/audit_index.json`                                                |
| 全局健康报告     | `docs/audit/audit_health.md`（failed_plugins 列表 + fail 原因）              |
| LLM 审计提示包   | `docs/audit/audit_review_prompt.md`（给二次 LLM 复盘用）                  |
| 路由误判回放清单 | `docs/audit/route_replay_cases.md`                                           |
| Case 镜像        | `docs/audit/<plugin>/cases/case_XXX/original.txt \| compact.txt \| summary.json` |

### 回收后必须执行

1. `--fail-on-any-failure` 让任意插件失败时非零退出；命令返回非 0 视为收口失败。
2. 阅读 `audit_health.md` 的 failed_plugins 段，按插件名回到步骤 2 单独修。
3. 若有 `route_replay_cases`，确认这些 case 已从 shell_session_plugin 迁出到专用插件。

---

## 7. 步骤 4 — `generate_plugin_capability_index.py`：刷新能力索引

**目的**：把 config / samples / showcase / audit 四个数据源重新聚合成 `plugin_capability_index.json`，让 LLM 审计和路由决策有最新数据。

```bash
tokenslim run python scripts/generate_plugin_capability_index.py
# 产物：docs/audit/plugin_capability_index.json + docs/reports/plugin_capability_matrix.md
```

### 回收后必须执行

1. 索引生成后**再回到步骤 1** 跑一次 `audit_sample_case_quality.py --plugin <plugin>`，触发 `find_capability_index_stale()` 检查，确认新索引比 `src/plugins/<plugin>_*` 新。
2. 若 `coverage_status` 出现新的 `missing_audit` / `config_only` / `source_only` → 回步骤 1 / 2 把对应插件补齐。
3. 若 `coverage_warnings` 出现 `declared_without_case_evidence:<claim>` → config 声明了能力但没 sample 证明，加 case 或删声明。

---

## 8. 路由诊断工具

- `tokenslim run --explain-route -- <command>` — 输出最终 route、候选 route、命中方式、优先级、插件链——判断"为什么这个命令进这个插件族"。
- `tokenslim explain-plugin --explain-command "<cmd>"` 或 `--input <log>` — 输出 selected plugin、why、alternatives、fallback_decision、retry_plugin、detector score / route match 与能力索引证据。`--explain-replay-out <path>` 生成路由误判回放 case 模板。
