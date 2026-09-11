# VCS 侧 5 个 needs_fix case 修复战术提示词

> 当前状态（2026-05-13）：本文保留为历史修复处方；当前权威总览见 `docs/audit/vcs_case_semantic_audit.md`，VCS 已收敛为 **328/328 all_pass，needs_fix=0，frozen=328**。

> 使用方式：本文档与 `CLAUDE.md`（Compression Protocol V1）一起生效。
> `CLAUDE.md` 是通用宪法；`docs/prompts/non_vcs_classical_prompts.md` 是非 VCS 战术补充；
> 本文档是针对 VCS 侧 **5 个** 剩余 needs_fix case 的单点战术处方。
>
> 动手前必须先读 `CLAUDE.md`，再读本文档「第 0 节上下文」，最后读对应 case 小节。

---

## 0. 上下文

### 0.1 背景

VCS 318 case 在 4 闸门（G1_ROI / G2_ANSI_CLEAN / G3_NO_ERROR_LOSS / G4_NON_EMPTY）机器化审计下通过 313 个（98.4%）；
剩余 5 个 needs_fix 列在 `docs/audit/vcs_case_semantic_audit.md`：

|   #   |  plugin  |          case_id          | compression_pct |   失败闸门    |            根因分类            |
| :---: | :------: | :-----------------------: | --------------: | :-----------: | :----------------------------: |
|   1   | vcs_bzr  | case_319_bzr_status_short |            -2.9 |    G1_ROI     |    族 A 短样本 IR 标签扩张     |
|   2   | vcs_cvs  |   case_190_cvs_history    |            -5.3 |    G1_ROI     |    族 A 短样本 IR 标签扩张     |
|   3   | vcs_cvs  |    case_36_cvs_status     |            -7.8 |    G1_ROI     |    族 A 短样本 IR 标签扩张     |
|   4   |  vcs_gh  |  case_158_gh_issue_view   |            40.4 | G3_ERROR_LOST | 族 B 噪音过滤误吞 error 关键字 |
|   5   | vcs_glab | case_111_glab_issue_view  |            72.0 | G3_ERROR_LOST | 族 B 噪音过滤误吞 error 关键字 |

注意：审计表里的 `case_190` / `case_36` 原始标题在 `docs/audit/vcs_case_semantic_audit.md` 汇总行省略了后缀；
`src/plugins/vcs_cvs_plugin/showcase.rs` 里明确注册为 `("case_36", "cvs_status", "status")` 与 `("case_190", "cvs_history", "log")`，
本文档按真实场景名讨论。

### 0.2 4 闸门定义（本文档的硬门禁）

与 `docs/prompts/non_vcs_classical_prompts.md` 1.1 节完全一致：

- **G1_ROI**：`compression_pct >= 0`（法则 A）
- **G2_ANSI_CLEAN**：compact 中必须无 `0x1B` ESC 字节（法则 C）
- **G3_NO_ERROR_LOSS**：若 original 含 `(?i)error|fatal|panic`，compact 必须保留其中至少一个字面量（法则 D 防失忆）
- **G4_NON_EMPTY**：非空 original 必须产生非空 compact

数据源以 `scripts/audit_case_metrics.py` 生成的 snapshot JSON 为准。

### 0.3 可复用公共层

|            层            |                                          位置                                           |         适用本文档 case         |
| :----------------------: | :-------------------------------------------------------------------------------------: | :-----------------------------: |
|         ROI 兜底         | `crate::core::utils::roi::prefer_non_expanding(raw: &str, compacted: String) -> String` | **case 1/2/3**（族 A 直接套壳） |
| 异常关键字保留白名单思路 |                  `docs/prompts/non_vcs_classical_prompts.md` B.1 / B.2                  |  **case 4/5**（族 B 战术复用）  |

签名关键约束（摘自 `src/core/utils/roi.rs`）：

- 先用 `trim_end_matches('\n' | '\r')` 对齐比较；
- trimmed 相等但完整字节扩张时**仍回退 raw**（避免尾换行歧义让审计判负）；
- 允许等长输出通过。

### 0.4 硬约束（本文档所有 case 通用）

1. 禁止自创新压缩符号：不得引入 `$XXX|` 新 IR 标签，`ST:` / `LB:` / `AS:` / `DESC:` / `gl:` / `bb:` 等现有前缀可复用。
2. `compress()` 最外层必须包一层 `prefer_non_expanding(raw, compacted)`；缺失时必须补上。
3. Rust 注释必须中文。
4. 测试必须用 `std::fs::read_to_string` 读 `samples/<plugin>/...`，**禁止** hardcode 大段日志字符串。
5. 已冻结且 4 闸门均通过的其他 case `compact_hash` 不得漂移；违反时 `-FailOnFrozenChange` 会拦截 PR。
6. **禁止后补识别**：每个 case 修复后，若无法当轮通过 `-FreezeCase <case_id> -RequireSemanticGate`，不得绕过闸门强行冻结。

### 0.5 验证范式（所有 case 通用）

本文档所有命令必须使用 `tokenslim run` 前缀（非 Rust 内部单测时）。

```powershell
# 0) 单插件回归
tokenslim run cargo test --lib <plugin>_plugin::

# 1) 生成快照（含 snapshot JSON + diff）
tokenslim run python scripts/audit_case_metrics.py -Plugin <plugin> -Version v<N> -FailOnRegression -FailOnFrozenChange

# 2) 导出单 case 前后文本以便人工对比
tokenslim run python scripts/audit_case_metrics.py -Plugin <plugin> -Version v<N> -CaseId <case_id>
# 查看 docs/audit/<plugin>/cases/<case_id>/original.txt 和 compact.txt

# 3) 单 case 通过 4 闸门 → 冻结（硬门禁）
tokenslim run python scripts/audit_case_metrics.py -Plugin <plugin> -Version v<N> -FreezeCase <case_id> -RequireSemanticGate

# 4) 全库回归
tokenslim run cargo test --lib
```

脚本语义参考 `scripts/audit_case_metrics.py` 中 `Test-SemanticGates` 函数与 `-RequireSemanticGate` 分支——
闸门失败时 `-FreezeCase` 会直接抛错，绝不把未通过 case 冻结进 `frozen_cases.json`。

---

## 1. 族 A G1_ROI 违规（case 1/2/3 — 共用战术）

### 1.1 共同根因

三个 case 的压缩率分别为 -2.9% / -5.3% / -7.8%，全部是**短样本 + 行级 `ST:` 前缀**导致的微量扩张：

|     case     |                    original 每行格式                     |                      compact 每行格式                       |  单行增量 |
| :----------: | :------------------------------------------------------: | :---------------------------------------------------------: | --------: |
| bzr case_319 |     ` M  src/main.rs`（短状态码，前导空格 + 两空格）     |                     `ST:M src/main.rs`                      | +1 B / 行 |
| cvs case_190 | `R 2026-04-08 12:00 [alice] src/main.java: Resync point` | `ST:R 2026-04-08 12:00 [alice] src/main.java: Resync point` | +3 B / 行 |
| cvs case_36  |          `M src/plugins/vcs_plugin/methods.rs`           |          `ST:M src/plugins/vcs_plugin/methods.rs`           | +3 B / 行 |

三者在原文本总长度 < 250 B、单行字段少、可字典化的重复 token 极少的情况下，**`ST:` 前缀的字面开销**压过了任何节省。
这和非 VCS 族 A（rust_go / gcc_log / maven）的 short-sample 扩张是同类型缺陷。

### 1.2 共同修复战术（首选：ROI 兜底）

`src/plugins/vcs_bzr_plugin/methods.rs::compact_bzr_status_cmd`、
`src/plugins/vcs_cvs_plugin/methods.rs::compact_cvs_status` 及相关 `map_cvs_status` 调用路径、
以及 cvs_history 所在的 `compact_cvs_log_family_for_ai` 的短样本分支，
都需要在 `compress()` 入口（或 `compact_*_for_ai` 最外层）加：

```rust
use crate::core::utils::roi::prefer_non_expanding;

pub fn compact_bzr_status_for_ai(raw: &str) -> String {
    let out = anchor_guard(raw, || compact_bzr_status_cmd(raw));
    // ROI 兜底：扩张时回退原文，保证 G1_ROI 不违规
    prefer_non_expanding(raw, out)
}
```

CVS 侧对称改造 `compact_cvs_status_for_ai` 与 `compact_cvs_log_family_for_ai`（或其主分派函数）。

这把「`ST:` 前缀对短样本的净扩张」兜回透传，三个 case 的 `compression_pct` 直接变成 0%（或略正）。

### 1.3 共同修复战术（备选：短样本 fast-path）

如果希望保留 `ST:` 前缀对长样本（cvs_history 40 行级别、bzr status 50+ 行级别）的可读性收益，
可以在 `compress()` 顶部加短样本阈值，并仍保留 ROI 兜底作为最后一层护栏：

```rust
// 短样本（< 200 字节或 < 6 行）直接透传：ST: 前缀收益不足以抵消开销
const SHORT_SAMPLE_BYTES: usize = 200;
if raw.len() < SHORT_SAMPLE_BYTES {
    return raw.to_string();
}
```

先做 1.2 的 ROI 兜底（最小闭环），若后续发现中等长度样本还有边界 G1 失败，再加 1.3。**不得**两者都不加。

### 1.4 具体 case 交付要求

#### 1.4.1 case 1：`vcs_bzr / case_319_bzr_status_short`

- original（6 行含触发锚点 + 空行）：
  ```
  bzr status --short
   M  src/main.rs
   M  src/lib/utils.rs
  +A  src/auth/login.rs
  ?   tests/smoke_test.rs
  ```
- compact 现状（compression -2.9%）：
  ```
  bzr status --short
  ST:M src/main.rs
  ST:M src/lib/utils.rs
  ST:A src/auth/login.rs
  ST:? tests/smoke_test.rs
  ```
- 违规：`ST:` 前缀 + 尾部额外换行让 compact 比 original 多 3 字节左右。
- 修复接入点：`src/plugins/vcs_bzr_plugin/methods.rs` 的 `map_bzr_short_status` 调用入口 `compact_bzr_status_cmd`，
  在 `compact_bzr_status_for_ai` 最外层包 `prefer_non_expanding`。
- 保留断言：`tests.rs::test_status_short_case_319` 中 `c.contains("M src/main.rs")` 在「回退原文」路径下仍应满足
  （原文本身含 `M  src/main.rs`，匹配 `contains("M src/main.rs")` 的双空格子串也为 true）。**验证这条断言不会误判**；
  若因双空格破坏断言，改为 `assert!(c.contains("src/main.rs"))` 并保留锚点断言。

#### 1.4.2 case 2：`vcs_cvs / case_190_cvs_history`

- original：
  ```
  cvs history -c
  R 2026-04-08 12:00 [alice] src/main.java: Resync point
  M 2026-04-07 10:30 [bob] src/utils.java: Modified
  A 2026-04-06 09:00 [charlie] src/test.java: Added
  ```
- compact 现状（compression -5.3%）：
  ```
  cvs history -c
  ST:R 2026-04-08 12:00 [alice] src/main.java: Resync point
  ST:M 2026-04-07 10:30 [bob] src/utils.java: Modified
  ST:A 2026-04-06 09:00 [charlie] src/test.java: Added
  ```
- 违规：每行 `ST:` +3 B × 3 行 + 尾换行 ≈ +10 B，原文 ~190 B → -5.3%。
- 修复接入点：
  - `src/plugins/vcs_cvs_plugin/methods.rs` 的 `compact_cvs_history_lines`（legacy pass-through）或
    `compact_cvs_log_family_for_ai`（真实入口，根据 `cvs_subcommand_is` 路由）；
  - 在实际进入 `map_cvs_status` → `format!("ST:{} {}", ...)` 的最外层调用点包 `prefer_non_expanding`。
- 期望输出：这条 case 短且状态码 `R/M/A` 本身已经是一字符，**`ST:` 前缀净亏损**，宜回退原文，`compression_pct = 0`。

#### 1.4.3 case 3：`vcs_cvs / case_36_cvs_status`

- original（6 行，5 条状态 + 1 条锚点）：
  ```
  cvs status
  M src/plugins/vcs_plugin/methods.rs
  A samples/vcs/case_31_git_checkout_file.log
  R src/legacy/old_cvs_adapter.rs
  ? tmp/debug-notes.txt
  C src/plugins/vcs_plugin/parser.rs
  ```
- compact 现状（compression -7.8%）：
  ```
  cvs status
  ST:M src/plugins/vcs_plugin/methods.rs
  ST:A samples/vcs/case_31_git_checkout_file.log
  ST:R src/legacy/old_cvs_adapter.rs
  ST:? tmp/debug-notes.txt
  ST:C src/plugins/vcs_plugin/parser.rs
  ```
- 违规：5 条 `ST:` 前缀 × 3 B = +15 B，原文 ~210 B → -7.8%。
- 修复接入点：`compact_cvs_status_for_ai` 最外层包 `prefer_non_expanding`。
- 注意：`tests.rs::test_status_v_case_315` 等相邻 case 仍然期待压缩正收益（长样本），
  ROI 兜底只在短样本上回退原文，不影响其他 case 的 `ST:` 前缀输出。
  修复后必须跑 `cargo test --lib vcs_cvs_plugin::` 确认所有既有 case 通过。

### 1.5 族 A 验证清单

- [ ] `cargo test --lib vcs_bzr_plugin::`、`cargo test --lib vcs_cvs_plugin::` 全绿
- [ ] `case_319` / `case_190` / `case_36` 的 `compression_pct >= 0`
- [ ] 其余 bzr 12 + cvs 13 已冻结 case 的 `compact_hash` 未漂移（`-FailOnFrozenChange` 通过）
- [ ] `-FreezeCase <case_id> -RequireSemanticGate` 对 3 个 case 全部抛 `gate_check=PASS` 并完成冻结
- [ ] `docs/audit/vcs_case_semantic_audit.md` 下次重跑后这 3 行从 needs-fix 表消失

---

## 2. 族 B G3_ERROR_LOST 违规（case 4/5 — 共用战术）

### 2.1 共同根因：过度激进的「噪音过滤」吞掉了 error 字面量

两个 case 的 `compression_pct` 本身很健康（40.4% / 72%），**问题不在压缩率**，
而在：`is_gh_noise` / `is_glab_desc_boundary` 把 `Steps to reproduce:` / `Expected:` / `Actual:` / `Description:` 列为
「可丢噪音」，导致 issue / MR 正文里含 `Error 500` / `Error occurs` 的关键语义**整段被丢**。

这是 `CLAUDE.md` **法则 D 防失忆**的硬违规——`error|fatal|panic` 关键字被字典化 / 丢弃都算失忆，
与非 VCS 族 B（node_error / python_traceback / smart_code / java_stack）的 G3 违规同源。

### 2.2 共同修复战术：两层防护

**第一层（首选）：白名单不丢含保留词的行。**
在 `src/plugins/vcs_gh_plugin/methods.rs` 的 `is_gh_noise`、
`src/plugins/vcs_glab_plugin/methods.rs` 的 `is_glab_desc_boundary` 中，调用位置做**前置守卫**：

```rust
/// 保留词正则：法则 D 防失忆关键字。命中时绝不允许被 is_*_noise / is_*_boundary 判为可丢。
fn line_has_preserved_keyword(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    // 大小写不敏感匹配 error / fatal / panic / exception（Java 风格） / uncaught
    l.contains("error")
        || l.contains("fatal")
        || l.contains("panic")
        || l.contains("exception")
        || l.contains("uncaught")
}
```

调用 `is_gh_noise` / `is_glab_desc_boundary` 的位置改为：

```rust
// 即便行首是 "Actual:" / "Description:" 等结构化前缀，
// 若含法则 D 保留词，也必须保留原行（或至少保留关键短语）。
if is_gh_noise(trimmed) && !line_has_preserved_keyword(trimmed) {
    continue;
}
```

对 glab 的 `compact_glab_issue_view`（methods.rs 第 376 行起）同样改：
在 `is_glab_desc_boundary` 判定进入时，若 `line_has_preserved_keyword(trimmed)` 为真，
不触发 `in_desc = false`，而是继续把这些行收入 `desc` 列表。

**第二层（最终兜底）：`compress()` 尾部 G3 自检。**
`compact_gh_for_ai` / `compact_glab_for_ai` 返回前做：

```rust
use crate::core::utils::roi::prefer_non_expanding;
use regex::Regex;

pub fn compact_gh_for_ai(raw: &str) -> String {
    let out = /* 现有压缩逻辑 */;

    // G3 自检：若 raw 含法则 D 保留词但 out 未保留，触发兜底
    let re = Regex::new(r"(?i)error|fatal|panic").expect("valid regex");
    let out = if re.is_match(raw) && !re.is_match(&out) {
        // 兜底策略：把原文中第一条含保留词的行追加到 compact 末尾
        let rescued = raw
            .lines()
            .find(|l| re.is_match(l))
            .unwrap_or("");
        format!("{}\n{}", out, rescued)
    } else {
        out
    };

    prefer_non_expanding(raw, out)
}
```

第二层兜底必要时再加：**优先修第一层**（精准的语义保留），只有 case 本身太难精确拆字段时才用兜底。
本文档两个 case **都能通过第一层解决**。

### 2.3 具体 case 交付要求

#### 2.3.1 case 4：`vcs_gh / case_158_gh_issue_view`

- original（14 行）：
  ```
  gh issue view 42
  issue #42: Bug: Login fails with OAuth
  Labels: bug
  Assignees: alice
  Author: bob
  State: open

  Steps to reproduce:
  1. Go to login page
  2. Click OAuth button
  Expected: Login success
  Actual: Error 500
  ```
- compact 现状（compression 40.4%，但 G3_ERROR_LOST）：
  ```
  gh issue view 42
  issue #42:Bug: Login fails with OAuth
  LB:bug
  AS:alice
  AU:bob
  ST:open
  1. Go to login page
  2. Click OAuth button
  ```
  （`Expected: Login success` 和 `Actual: Error 500` 被 `is_gh_noise` 吞掉）
- 违规：丢失 `Error 500` 关键字，违反法则 D。
- 修复接入点：`src/plugins/vcs_gh_plugin/methods.rs` 的 `is_gh_noise`（第 692 行附近）与其调用位置。
- 期望修复后输出（至少保留一条含 error 的行，允许折叠为单行）：
  ```
  gh issue view 42
  issue #42:Bug: Login fails with OAuth
  LB:bug
  AS:alice
  AU:bob
  ST:open
  1. Go to login page
  2. Click OAuth button
  Actual: Error 500
  ```
- 判定：`compression_pct` 可能从 40.4% 降到 30+% 左右，**仍远高于 0**；G3 通过。

#### 2.3.2 case 5：`vcs_glab / case_111_glab_issue_view`

- original（20 行）：
  ```
  glab issue view 5
  !5 Important bug fix
  =====================================
  Author:     alice <alice@example.com>
  ...
  Description:
  This is a critical bug that needs to be fixed urgently.

  Steps to reproduce:
  1. Go to login page
  2. Enter credentials
  3. Click login
  Expected: User logged in
  Actual: Error occurs
  ```
- compact 现状（compression 72%，但 G3_ERROR_LOST）：
  ```
  glab issue view 5
  !5 OW:@alice AS:@bob ST:Open URL:gl:owner/repo/-/issues/5
  DESC: This is a critical bug that needs to be fixed urgently.
  ```
  （Steps / Expected / Actual 全部被 `is_glab_desc_boundary` 当边界丢掉）
- 违规：丢失 `Error occurs`，违反法则 D。
- 修复接入点：`src/plugins/vcs_glab_plugin/methods.rs::compact_glab_issue_view`（第 376 行起），
  以及 `compact_glab_mr_view`（第 254 行起，同架构，防御性同步修复）。
  关键函数 `is_glab_desc_boundary`（第 603 行）+ `is_glab_view_noise`（同文件）。
- 建议修复策略（**保持现有聚合输出风格**）：
  1. 把 Description 段里含保留词的行额外拼接到 `DESC:` 之后，或单独用 `ERR:` 前缀输出一条：
     ```
     DESC: This is a critical bug that needs to be fixed urgently.
     ERR: Actual: Error occurs
     ```
  2. 或整体折叠：`DESC: ... | Actual: Error occurs`。
- 判定：`compression_pct` 可能从 72% 降到 55–65% 左右；G3 通过。

### 2.4 族 B 验证清单

- [ ] `cargo test --lib vcs_gh_plugin::`、`cargo test --lib vcs_glab_plugin::` 全绿
- [ ] `case_158_gh_issue_view` 与 `case_111_glab_issue_view` 的 `compact` 含至少一个 `(?i)error|fatal|panic` 匹配
- [ ] 两个 case 的 `compression_pct` 仍然 `>= 0`
- [ ] 其余 gh 19 + glab 6 已冻结 case 的 `compact_hash` 未漂移
- [ ] `-FreezeCase <case_id> -RequireSemanticGate` 两个 case 抛 `gate_check=PASS` 并完成冻结
- [ ] `docs/audit/vcs_case_semantic_audit.md` 重跑后这 2 行从 needs-fix 表消失

---

## 3. 提交交付模板

每个 case 修复 PR 描述必须包含：

```
VCS needs_fix case: <case_id>
所属族群: A / B
失败闸门: G1_ROI / G3_ERROR_LOST
根因: <一句话>
修复策略: <一句话，引用本文档 1.2 / 1.3 / 2.2>
改动文件:
  - src/plugins/<plugin>/methods.rs  (compact_*_for_ai + 辅助函数)
  - src/core/utils/roi.rs            (仅复用，不修改)
回归:
  - cargo test --lib <plugin>_plugin:: OK
  - scripts/audit_case_metrics.py -Plugin <plugin> -Version v<N> -FailOnRegression -FailOnFrozenChange OK
  - scripts/audit_case_metrics.py -Plugin <plugin> -Version v<N> -FreezeCase <case_id> -RequireSemanticGate OK
  - cargo test --lib: <pass>/<total>, 0 failed, 0 warnings
```

## 4. 禁区清单（二次确认）

- [ ] **不得**引入新 IR 标签（自创 `$XXX|` 前缀）。
- [ ] **不得**把异常类名 / 保留词（Error / Fatal / Panic / Exception）字典化（族 B 硬约束）。
- [ ] **不得**绕过 `prefer_non_expanding`——所有 `compact_*_for_ai` 最外层都要包。
- [ ] **不得**用 hardcode 日志字符串写测试；复用 `vcs_bzr_plugin/tests.rs` / `vcs_cvs_plugin/tests.rs` 中 `read_case(...)` 模式。
- [ ] **不得**在本轮改动里顺手重构其他已通过 case（违反最小闭环原则）。
- [ ] **不得**使用 `-FreezeUnchanged`（脚本在 `-RequireSemanticGate` 下会直接抛错）。

## 5. 回应口令

> “首席架构师，我已接管 VCS needs_fix 战术上下文。通用宪法、4 闸门闸门规则、族 A/B 战术约束已加载。
> 本轮目标：把 VCS needs_fix 从 5 降至 0（bzr × 1、cvs × 2、gh × 1、glab × 1）。请下达具体 case 修复顺序。”
