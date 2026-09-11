<!-- version: 2026-06-11 | 被 AGENTS.md 引用。跑审计/动 case/改 showcase.rs/改 sidecar 时先读本文件 -->

# 变更后必跑脚本（详细参数与产物回收）

> 触发条件与 4 步概览见 `AGENTS.md`。本文件是完整执行手册。

## 前置：解释器与工作目录（先确认，否则第 1 步 LLM 模式必崩）

- **审计脚本必须用带 PyYAML 的解释器**：
  `C:/Users/wanpi/.workbuddy/binaries/python/envs/default/Scripts/python.exe`。
  裸 `python` 缺 `yaml` 时会静默退化到内置 `_MiniYaml`，而后者不支持嵌套映射——会把
  `tokenslim_kb/architecture.yaml` 的 `routing:` 解析成空串，随后
  `audit_llm_common.py::build_case_quality_prompt` 抛
  `AttributeError: 'str' object has no attribute 'get'`（无 `--llm-audit` 时不触发）。
- **工作目录用主仓** `C:/git_work/TokenSlim`；本项目 worktree（`Worktrees/TokenSlim/*`）的
  分支 ref 可能被外部进程清空，提交不具持久性。

## 触发条件

以下任何变更发生后，**必须按顺序**跑完 4 个脚本并完成回收，**禁止**只跑 1-2 个就声称"已审计"：

- 新增/删除/重命名 case（`samples/<plugin>/case_*.{log,json,hex,md,xml,txt,...}`）

  - 排除 `case_*.scenario.yaml`（sidecar）和多扩展名 `case_*.tar.gz`（元数据）

- 新增/删除/重命名插件（`src/plugins/<plugin>_plugin/`、`config/plugins/*.json`）

  - **插件配置只看** **`config/plugins/*.json`**；根目录 `config/*.toml` 是项目层配置，不触发审计

- 修改 `src/plugins/mod.rs`（增减 `pub mod`）

- 修改 showcase.rs（增减 case 注册）

- 修改插件 parser / rule / 任何影响压缩产物的代码

- 修改 `samples/<plugin>/case_NNN_*.scenario.yaml`（sidecar）

## 简表：变更 → 必跑脚本

| 变更类型                    | 必跑脚本（按顺序）                       |
| ----------------------- | ------------------------------- |
| 新增 case                 | 1 → 2 → 4 → 1（cap\_index 刷新后回归） |
| 删除 case                 | 1 → 2 → 3 → 4 → 1               |
| 新增插件                    | 1 → 3 → 4 → 1                   |
| 修改插件 parser / rule      | 2 → 3 → 4 → 1                   |
| 修改 showcase.rs          | 1 → 2 → 3 → 4 → 1               |
| 修改 `src/plugins/mod.rs` | 1 → 3 → 4 → 1                   |
| 修改 sidecar              | 1 → 2 → 4                       |
| 批量改动（plugin 重构）         | 1 → 2 → 3 → 4 → 1（按"完整循环"）      |

## LLM 与审计状态机（先看这段再跑脚本）

`audit_sample_case_quality.py` 是 **lint-only 默认 + LLM opt-in** 的工具，没有"已审计"外部状态机——每次跑都对 plugin 下所有 case 全量重跑 lint。

| 维度            | 现状                                                                                                                                  | 影响                                                   |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| LLM 默认开关      | **关闭**（lint-only）                                                                                                                   | 不传 `--llm-audit` / `--require-llm-audit` 时只跑确定性 lint |
| LLM 开启方式      | `--llm-audit` opt-in；无 key 时降级 lint-only（除非 `--require-llm-audit` 强失败）                                                              | 想要"真实环境判定"必须显式声明                                     |
| LLM 调用粒度      | **per-case**（内部逐 case 调）                                                                                                             | 1000+ case 全量开 LLM 慢且贵；**当前版本无 `--case-id` 单点参数**（传了会 `unrecognized arguments`），要省成本只能先 lint-only 定位再整插件开 LLM |
| 状态机           | 无 `audited/skipped/done` 外部状态                                                                                                       | `case_quality_latest.json` 只是滚动快照，不是增量缓存             |
| 跨次复用          | **不增量**（无 hash 比对跳过机制）                                                                                                              | 改一个 case 也会对 plugin 下所有 case 重跑；想省时间就 `--case-id`    |
| LLM 输出 status | `valid` / `needs_fix` / `duplicate` / `too_small` / `not_registered` / `title_mismatch` / `fabricated` / `routing_boundary_unclear` | `fabricated` 是 LLM 真实性仲裁专属桶                          |

```bash
# 1) 改完先跑 lint-only 全量（快、零 token 成本）
tokenslim run python scripts/audit_sample_case_quality.py --plugin <plugin>
# 2) 需要 LLM 真实性仲裁时整插件开（当前版本无 --case-id，无法单点复审）
tokenslim run python scripts/audit_sample_case_quality.py --plugin <plugin> --llm-audit --allow-llm-missing
# 3) CI 强一致（无 key 直接 sys.exit(1)）
tokenslim run python scripts/audit_sample_case_quality.py --plugin <plugin> --require-llm-audit --strict-drift
```

> LLM 送审的样本预览是**头+尾**采样（`_sample_snippet`）：短样本全量送审，超长样本取
> 前 3/4 + 尾 1/4。只取头部会让审查者看不到样本尾部的失败段与统计段，把「声明有失败但
> 预览里全是 ok」的合规大样本误判为 `fabricated`（`rust_go_plugin/case_017` 曾因此被误判）。

## 1. audit\_sample\_case\_quality.py — 物理 case 质量门禁（压缩前）

**目的**：证明 case 本身是"好 case"——真实、有锚点、覆盖目标命令族、case 数与 showcase.rs / mod.rs 一致。

```bash
tokenslim run python scripts/audit_sample_case_quality.py --plugin <plugin>   # 单插件
tokenslim run python scripts/audit_sample_case_quality.py --all               # 全插件
```

### 增量审计缓存（默认开启，`--no-cache` 强制全跑）

- 读 `case_quality_latest.json`，对每个 case 算 `sha256(content)` 查哈希索引。

- **命中 + cached** **`final_status ∈ {valid, not_registered, duplicate, title_mismatch, routing_boundary_unclear}`** **+ LLM 开关状态一致** → 复用结论。

- **不缓存** **`needs_fix`** **/** **`fabricated`**（缺陷判定可能翻转）。

- **LLM 开关切换会强制全跑**。终端输出 `[cache] hits=47 misses=1 hit_rate=97.9%`。

**触发** **`--no-cache`** **的场景**：改了 `audit_llm_common.py` / `audit_sample_case_quality.py` 的 lint 阈值；改了 `scripts/prompts/audit/case_quality/` 提示词模板；改了 `tokenslim_kb/` 项目上下文；改了 `src/plugins/mod.rs` / showcase.rs 注册。

### 回收产物（必读）

| 产物     | 路径                                                                | 必读项                           |
| ------ | ----------------------------------------------------------------- | ----------------------------- |
| 案例质量报告 | `docs/audit/<plugin>/sample_quality/case_quality_report.md`       | status=valid 比例、needs\_fix 清单 |
| 漂移报告   | `docs/audit/<plugin>/sample_quality/drift_audit_*.json`           | 7 条漂移轴的 warning/error         |
| 命令族覆盖  | `docs/audit/<plugin>/sample_quality/command_family_coverage.json` | 60 族 vs 实际覆盖数                 |

### 回收后必须执行

1. 若 `status=valid` 比例 < 90% → 修正 case 后重跑，**不得带病进入步骤 2**。
2. 若 `drift_audit_*` 有任何 warning/error → **必须立刻修复**：

   - `samples-vs-mod-rs` 多：移走 samples 目录或加 `pub mod`

   - `samples-vs-mod-rs` 少：建空 `samples/<plugin>_plugin/` 目录

   - `case-count-mismatch`：补 showcase.rs 注册或删孤儿 case

   - `sidecar-missing`：跑 `python scripts/generate_case_sidecars.py --plugin <plugin>` 补模板

   - `ghost-case`：showcase.rs 注册了但 samples 里没有，按需补文件或删注册
3. 若 `command_family_coverage` 缺失目标族 → 加 case 后从步骤 1 重跑。
4. CI 模式加 `--strict-drift` 让漂移发现时 `sys.exit(1)`。

## 2. audit\_case\_metrics.py — 压缩语义 + 冻结门禁（压缩后）

**目的**：校验 `target/<plugin>_compact_showcase_report.txt` 中 original vs compact 的对齐、压缩、语义保真与冻结。

```bash
# 单插件全量回归（Windows：版本号手填当天日期，如 v20260612_r1；PowerShell 可用 "v$(Get-Date -Format yyyyMMdd)_r1"）
tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --version vYYYYMMDD_r1 --require-semantic-gate --fail-on-regression
# 单 case 镜像导出（读 docs/audit/<plugin>/cases/case_XXX/original.txt 对比 compact.txt）
tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --version vX_r1 --case-id case_XXX
# 冻结新通过的 case
tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --version vX_r1 --freeze-case case_XXX --require-semantic-gate
```

### 回收产物（必读）

| 产物          | 路径                                              | 必读项                                                |
| ----------- | ----------------------------------------------- | -------------------------------------------------- |
| 快照 JSON/CSV | `docs/audit/<plugin>/<plugin>.vX_r1.{json,csv}` | 逐 case 的 Original/Compact/Compression%             |
| Diff        | `docs/audit/<plugin>/<plugin>.vX_r1.diff.md`    | `improved / regressed / unchanged / new / missing` |
| Latest      | `docs/audit/<plugin>/<plugin>.latest.json`      | 滚动覆盖                                               |
| 冻结清单        | `docs/audit/<plugin>/frozen_cases.json`         | 哪些 case 已冻结、其 `compact_hash`                       |
| 状态机         | `docs/audit/<plugin>/audit_state.json`          | `todo/auditing/frozen/waived`                      |

### 回收后必须执行

1. **任何** **`regressed`** **case** → 禁止收口；查 `case_XXX/compact.txt` 对比上一版本定位回退。
2. **任何** **`frozen_changed_case`** → 状态自动回退到 `auditing`，必须重审 + 重新冻结或 `waived`。
3. **任何** **`semantic_gate_failed`** → 9 条宪法规则（Anchor Guard / Anti-Amnesia / ROI Gate / Diff Defense / ...）之一被破坏；先修代码再重跑。
4. **新 unchanged case** → 建议 `--freeze-case case_XXX --require-semantic-gate` 锁定。
5. 任何 `new` / `missing` case → 确认是预期变更（不是误删/误增）。

## 3. audit\_all\_case\_metrics.py — 全插件健康检查（收口前必跑）

**目的**：聚合所有插件健康状态，输出全局健康报告 + LLM 审计提示包。

```bash
# Windows：版本号手填当天日期，如 v20260612_r1；PowerShell 可用 "v$(Get-Date -Format yyyyMMdd)_r1"
tokenslim run python scripts/audit_all_case_metrics.py --version vYYYYMMDD_r1 --require-semantic-gate --fail-on-regression --fail-on-frozen-change --fail-on-any-failure
```

### 并发与冻结策略

- 禁止并行运行同一插件的 `audit_case_metrics.py`（写同一份 `frozen_cases.json` / `audit_state.json`，会损坏）。

- 多插件批量审计必须用 `audit_all_case_metrics.py` 串行调度。

- `audit_sample_case_quality.py --all` 和 `audit_all_case_metrics.py` 不要同时跑（都写 `docs/audit/<plugin>/` summary）。

- 已冻结 case 若 `compression%` 与 `compact_hash` 均不变，不再人工复读。

- 脚本输出 `frozen_changed_case=<case_id>` → 该 case 必须解冻重审。

- 状态机自动维护 `audit_state.json`（`todo / auditing / frozen / waived`），被冻结 case 内容变化自动回退 `auditing`。

### 全局产物位置

| 产物        | 路径                                                                           |
| --------- | ---------------------------------------------------------------------------- |
| 全局索引      | `docs/audit/audit_index.json`                                                |
| 全局健康报告    | `docs/audit/audit_health.md`（failed\_plugins 列表 + fail 原因）                   |
| LLM 审计提示包 | `docs/audit/audit_review_prompt.md`（给人/二次 LLM 复盘用）                           |
| 路由误判回放清单  | `docs/audit/route_replay_cases.md`                                           |
| Case 镜像   | `docs/audit/<plugin>/cases/case_XXX/original.txt\|compact.txt\|summary.json` |

### 回收后必须执行

1. `--fail-on-any-failure` 让任意插件失败时非零退出；命令返回非 0 视为收口失败。
2. 阅读 `audit_health.md` 的 failed\_plugins 段，按插件名回到步骤 2 单独修。
3. `audit_review_prompt.md` 给二次 LLM 审计用——只在人类评审环节读。
4. 若有 `route_replay_cases`，确认这些 case 已从 shell\_session\_plugin 迁出到专用插件。

## 4. generate\_plugin\_capability\_index.py — 刷新能力索引（步骤 3 之后必跑）

**目的**：把 config/samples/showcase/audit 四个数据源重新聚合成 `plugin_capability_index.json`，让 LLM 审计和路由决策有最新数据。

```bash
tokenslim run python scripts/generate_plugin_capability_index.py
# 产物：docs/audit/plugin_capability_index.json + docs/reports/plugin_capability_matrix.md
```

### 回收后必须执行

1. 索引生成后**再回到步骤 1**跑一次 `audit_sample_case_quality.py --plugin <plugin>`，触发 `find_capability_index_stale()` 检查，确认新索引比 `src/plugins/<plugin>_*` 新。
2. 若 `coverage_status` 出现新的 `missing_audit` / `config_only` / `source_only` → 回步骤 1/2 把对应插件补齐。
3. 若 `coverage_warnings` 出现 `declared_without_case_evidence:<claim>` → config 声明了能力但没 sample 证明，加 case 或删声明。

## 串行依赖（不要乱）

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

> **内容分类器改动不在上述插件级流水线覆盖内**。改动版权应在 `content_classifier`（增删语义类别、改种子特征词、改 build.rs 聚合配置）时，除上述流水线外**必须**先跑 `cargo test --lib content_classifier`（含 syslog/ci\_log/cloud\_log 等 sweep 测试）；若分类器变化改变了某插件切片的路由（尤其剥皮类别增量接管），回到步骤 1/2 复核受影响插件的 case 质量与压缩语义。剥皮类别边界规则见 `COMPRESSION.md`「内容分类器：剥皮类别边界」。

## LLM 审计公共基座

三个 LLM 审计脚本（`audit_sample_case_quality.py` / `audit_case_metrics.py` / `audit_all_case_metrics.py`）共享 `scripts/audit_llm_common.py`：LLMConfig、call\_llm\_chat、提示词模板加载、知识库加载、漂移检测都在这里。

- 详细规范见 `docs/audit/llm_audit_common.md` 和 `scripts/prompts/audit/README.md`。

- 改动提示词（`scripts/prompts/audit/<kind>/<type>.md`）不需要改 Python 代码；新增审计目标只新建 `<type>.md` 一个文件。

## Case 审计即时决策（硬约束，禁止后补识别）

- 每审完一个 case 当场二选一：`通过冻结` 或 `需优化`；禁止"先跳过、后识别"。

- 若 `需优化`，立刻在任务看板新增任务项：`plugin`、`case_id`、`问题`、`建议改动`、`优先级`、`状态`。

- 任务项创建后立刻"修复并回归"或明确 `waived`（含理由）；禁止挂空任务。

- 只有当轮回归为 `improved/unchanged` 且语义通过才允许冻结；否则保持 `auditing`。

任务项模板：

```
- [ ] <plugin>/case_XXX | 问题: <一句话> | 建议: <可执行改动> | 优先级: P0/P1/P2 | 状态: todo/auditing/fixed/waived
```

## 路由诊断工具

- `tokenslim run --explain-route -- <command>`：输出最终 route、候选 route、命中方式、优先级、插件链——判断"为什么这个命令进这个插件族"。

- `tokenslim explain-plugin --explain-command "<command>"` 或 `--input <log>`：输出 selected plugin、why、alternatives、fallback\_decision、retry\_plugin、detector score/route match 与能力索引证据。`--explain-replay-out <path>` 生成路由误判回放 case 模板。

