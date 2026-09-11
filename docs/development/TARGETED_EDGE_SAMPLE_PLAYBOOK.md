# Targeted Edge Sample Playbook

> 目标：只补“会改变路由/语义质量”的边界样本，不做盲目堆量。  
> 适用：插件增强、路由推荐器增强、审计回归修复。  
> 最后更新：2026-05-15

---

## 1. 何时触发

满足任一条件即可启动本流程：

1. `explain-plugin` 输出出现 `review_recommended` 或 `review_and_retry`。
2. 审计通过但语义审阅发现“异常信号被吞掉/被平均掉”。
3. 路由命中正确，但 compact 输出缺少可决策信号（error/failed/5xx/slow 等）。
4. 样本来自真实环境，且能代表高频输入壳层（ANSI、wrapper、table/csv/json、relative-time 等）。

---

## 2. 不做什么

- 不因为“case 数少”直接批量补样本。
- 不为了追求压缩率牺牲错误信号保留。
- 不把插件改成业务 APM。
- 不在测试里手写超长字符串，必须落到 `samples/`。

---

## 3. 边界分类（优先级）

按 ROI 先后补：

1. **信号保留边界（P0）**  
   error/fatal/failed/rejected/conflict/panic/exception 被过滤或弱化。
2. **格式脱壳边界（P1）**  
   ANSI、table/csv/json wrapper、`remote:` 前缀、provider 壳层。
3. **字段稳定性边界（P1）**  
   时间列变体（`2 days ago`）、多空格/列错位、可选字段缺失。
4. **可读性边界（P2）**  
   字段顺序抖动、轻微噪音残留但不影响决策。

---

## 4. 最小执行环（单插件）

1. 明确一个薄弱点，只改一个行为面。  
2. 新增 1 个对应 `samples/<plugin>_plugin/case_xxx_*.log`。  
3. 更新 `showcase.rs` 把新 case 纳入报告。  
4. 更新 `tests.rs` 添加断言（至少含锚点 + 关键信号）。  
5. 跑插件测试：
   - `tokenslim run cargo test <plugin_name>::tests::`
6. 生成 showcase 报告：
   - `tokenslim run cargo test <plugin_name>::showcase::tests::generate_showcase_report -- --nocapture`
7. 跑单插件审计并导出 case：
   - `tokenslim run powershell -File scripts/audit_case_metrics.ps1 -Plugin <plugin> -Version <version> -ExportCases -RequireSemanticGate -FailOnRegression -FailOnFrozenChange`
8. 审计通过后冻结新 case：
   - `tokenslim run powershell -File scripts/audit_case_metrics.ps1 -Plugin <plugin> -Version <version> -FreezeCase <case_id> -RequireSemanticGate`
9. 文档同步：
   - `docs/reports/IMPLEMENTATION_STATUS.md`
   - `docs/tasks/OPTIONAL_BACKLOG.md`（如属于 backlog 事项收口）
   - 必要时更新 `README.md` 与能力矩阵

---

## 5. Case 设计模板

命名建议：

- `case_<id>_<plugin>_<boundary>.log`
- 例：`case_223_gerrit_remote_error_ansi.log`

测试断言建议（最少三条）：

1. **命令锚点保留**：`starts_with("<cmd>")`
2. **关键信号保留**：`contains("!error")` / `contains("SLOW")` 等
3. **边界目标命中**：如 `!contains('\x1b')`、relative-time 可解析

---

## 6. 完成定义（DoD）

一个“定向边界样本增强”任务完成，必须同时满足：

1. 新 case 在 `showcase`、`test`、`audit case mirror` 三处均可追踪。
2. `semantic_gate_failed=0`。
3. 新 case 已冻结，且 `state_todo=0`。
4. 没有引入 frozen 漂移（`frozen_changed=0`, `frozen_missing=0`）。
5. 文档状态与代码状态一致（无“已完成但文档仍待办”）。

---

## 7. 快速排查清单

- 看 `docs/audit/route_replay_cases.md`：是否有 explain 回放证据。
- 看 `docs/audit/<plugin>/cases/<case_id>/original.txt|compact.txt`：是否真的保留了异常信号。
- 看 `docs/audit/<plugin>/frozen_cases.json`：新 case 是否冻结成功。
- 看 `docs/reports/plugin_capability_matrix.md`：样本/审计/冻结计数是否同步。

