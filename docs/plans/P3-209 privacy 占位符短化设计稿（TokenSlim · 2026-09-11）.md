# P3-209 privacy 占位符短化设计稿（TokenSlim · 2026-09-11）

> 问题登记：`docs/reports/调用链代码复审问题清单（TokenSlim · 2026-09-05）.md` P3-209 行。
> 裁判标准：`AGENTS.md`「优化第一性原理」三问（理解优先 / 以 token 计量 / 机制自身过秤）。

## 一、问题陈述与实测基线

用户真实样本（2026-09-11，linkplay-server-201 的 `ls -l` 输出，含命令锚点两行）经真实
`CompressionPipeline` 实测：

| 口径 | 输入 | 输出 | 变化 |
|---|---|---|---|
| 字节 | 386B | 440B | **+14.0%** |
| token（管线估算） | 96 | 110 | **+14.6%** |

唯一接管插件=privacy（detect 1.0 无条件抢占）。`.tokenslim-redact.toml` 用户规则命中
`wiimu`（用户名/组名，重复词）→ `[TS_USER_RULE]`，6 处共 +24 tok，使脱敏产物反超原文。

**这不是 privacy 的功能缺陷**（脱敏本身正确且必要），而是**占位符字面量的 token 成本
从未被计量**：全 `TS_*` 家族同病——

| 现行占位符 | 估算 token/处 | 典型频次 |
|---|---|---|
| `[TS_USER_RULE]` | ≈6 | **极高**（用户规则命中重复词：用户名、组名、主机名片段） |
| `[TS_SECRET]` | ≈5 | 中（`key=/token=/password=` 赋值行） |
| `[TS_AWS_SECRET_ACCESS_KEY]` | ≈8 | 低 |
| `[TS_AWS_ACCESS_KEY_ID]` | ≈7 | 低 |
| `[TS_GITHUB_TOKEN]` / `[TS_BEARER_TOKEN]` / `[TS_LLM_API_KEY]` | ≈6 | 低 |
| `[TS_JWT]` / `[TS_DB_CREDENTIAL]` / `[TS_PRIVATE_KEY]` | ≈4~7 | 低 |

高频 × 单处高成本 = 用户规则场景净膨胀；凭证类低频但单处更贵。

## 二、安全边界（不可逾越，决定方案形态）

`privacy_plugin/types.rs` 现行设计（:243-283）有三条刻意选择，本工单不得破坏：

1. **不入字典引擎**（compress 刻意忽略 `_dict_engine`）——物理断掉「占位符 → 原文」还原路径；
2. **decompress 空操作**——占位符不携带可还原信息；
3. **全部用户规则命中共用同一占位符** `[TS_USER_RULE]`——不区分命中哪条规则，
   即占位符本身零信息量。

推论：**方案只能是缩短字面量本身，不能是"字典化"**。任何「短符号 → 原文」映射、
任何按命中规则区分的编号（`$U1`/`$U2` 隐含"第几处相同"这一信息，本身无害，
但若进字典即违 1）都触碰安全边界。且 `$` 前缀是字典符号表命名空间（`$Pn`/`$JEX`），
短占位符应**留在括号族**，避免与字典 token 解析相互干扰。

## 三、候选方案

### 方案 A：纯字面量短化（推荐起点）

保持语义分组前缀可读性，压缩长度：

| 现行 | 短化 | 估算 token |
|---|---|---|
| `[TS_USER_RULE]` | `[UR]` | 2 |
| `[TS_PRIVATE_KEY]` | `[PK]` | 2 |
| `[TS_AWS_ACCESS_KEY_ID]` | `[AWSID]` | 3 |
| `[TS_AWS_SECRET_ACCESS_KEY]` | `[AWSKEY]` | 3 |
| `[TS_GITHUB_TOKEN]` | `[GHTOK]` | 3 |
| `[TS_BEARER_TOKEN]` | `[BEARER]` | 3 |
| `[TS_JWT]` | `[JWT]` | 2 |
| `[TS_DB_CREDENTIAL]` | `[DBCRED]` | 3 |
| `[TS_LLM_API_KEY]` | `[LLMKEY]` | 3 |
| `[TS_SECRET]` | `[SEC]` | 2 |

- 预期收益：用户样本 6×6→6×2，净省 ≈24 tok（110→86，低于原文 96）；
- 理解风险：`[UR]` 脱离上下文不可自解释。缓解见方案 B 的 legend；
- 字面量改动的兼容影响：`decompress` 空操作不受影响；但**历史已压缩产物**中的
  长占位符与新产物短占位符并存（可接受——占位符本就不可逆、无跨版本协议承诺）。

### 方案 B：短字面量 + 按需 legend（理解优先的补强）

当文档内占位符出现次数 ≥ 阈值（建议 ≥3）时，在输出尾部追加一行图例：
`[UR]=redacted-user-rule [SEC]=redacted-secret (irreversible)`——只描述**类型**，
不含原文，不违安全边界。成本 1 行/文档（流式按 flush chunk 计），摊薄后可忽略。

- 收益：第一性原理第 1 问（理解优先）显式达标——LLM 无需猜测 `[UR]` 含义；
- 取舍：小样本（1~2 处）省下的 token 部分被 legend 吃掉，故设阈值。

### 方案 C：维持现状（否决）

`[TS_USER_RULE]` 的"自解释性"在高频场景被净膨胀抵消，违背第一性原理第 3 问。

### 附：架构级远期项（不在本工单范围，仅登记）

privacy 以 1.0 整片抢占后，切片**不再进入任何压缩插件**（用户样本中清单本体因此
零压缩）。「脱敏后再分发」（redact-then-redispatch：privacy 作为前置变换而非终态
接管）可让清单/代码族在脱敏产物上继续压缩，收益更大，但涉及调度器架构调整 +
「脱敏产物是否可作为下游压缩输入」的安全评审，需独立立项。

## 四、影响面

| 项 | 说明 |
|---|---|
| `src/plugins/privacy_plugin/types.rs` | `redact_text` 11 处字面量 + 模块文档 |
| `src/plugins/privacy_plugin/test.rs` | 断言占位符字面量的用例全部同步 |
| P2-79 复用方 | `sql_plugin/methods.rs:121-123`（`obfuscate_sensitive=true` 时调 `redact_with_builtin_patterns`）产物格式随之变化；`sql_plugin/test.rs` 含占位符字面量断言，需同步 |
| 冻结基线 | 命中 `[TS_*]` 的 case 冻结快照会 `frozen_changed`——须走正规重冻结（证据：旧占位符→新占位符的纯字面量替换 diff），禁止静默改基线 |
| 语义门禁 | 占位符不是路径/时间戳，不触碰 rule 4/5/7；`docs/prompts/semantic_audit_profiles.md` 无需改（grep 确认无 `[TS_*]` 引用） |
| `COMPRESSION.md` | 补占位符表（若走方案 B 一并写 legend 规则） |

## 五、验收标准

1. 重复占位符场景（≥3 处）token 净收益 > 0；用户 2026-09-11 `ls -l` 样本输出
   token ≤ 原文 96 tok（方案 B 下含 legend 仍须 ≤）；
2. 占位符仍为零信息量：同一占位符不区分命中规则/原文，无任何还原路径（安全语义不变）；
3. 全量 lib + 集成回归绿；四段审计全绿（重冻结 case 附字面量替换 diff 证据）；
4. `privacy.user_rule_count` 等 metadata 不变。

## 六、执行序（待用户批准后开工）

1. 实现方案 A 字面量替换 + privacy 单测同步；P2-79 复用方对账；
2. 方案 B legend 机制 + 阈值单测；
3. 四段审计（含重冻结与证据回填）；
4. `COMPRESSION.md` 占位符表更新 + 问题清单 P3-209 回填 ✅。

## 七、执行记录（2026-09-11，方案 A+B 已实施）

- 实现：`redact_text` 11 处字面量短化（§三 方案 A 表）+ 方案 B legend（`PLACEHOLDER_LEGEND` / `LEGEND_MIN_OCCURRENCES=3` / `append_legend_if_warranted`，仅在 compress 输出侧，detect 探测与 sql 复用路径不受影响）。
- 断言同步：`privacy_plugin/test.rs`（7 例，含新增阈值契约 `omits_legend_below_occurrence_threshold` 与图例安全契约 `legend_describes_types_not_values`）、`sql_plugin/test.rs`（P2-79 三例 + 负断言扩为双族）、`core/debug_audit.rs`（2 处 `[LLMKEY]`）。
- 回归：全量 lib **1272 通过**；步骤 1 全插件（唯一 FAIL=encoding_fallback，HEAD 对照确认**既有状态**：case_008/009 needs_fix、case_010/011 not_registered，与本工单无关）；步骤 3 首跑 transient regressed=1、**权威复跑 failed=0 / regressed=0 / semantic_gate_failed=0**；步骤 4 能力索引已刷新；i18n --strict PASS。
- 冻结基线：docs/audit/ 与 samples/ 中旧占位符 **0 命中**（grep 实证）→ **无需重冻结**。
- privacy 自身仍无 samples/showcase（从未进入 case 审计轨道），无 step 2 case；顺手清除了失败运行产生的未跟踪 scratch `docs/audit/privacy/`。

### 验收标准实证与修正（诚实记录）

| 原标准 | 实测 | 裁定 |
|---|---|---|
| ① 重复占位符场景 token 净收益 >0（相对旧占位符） | 用户样本 110→107 tok（-3） | ✅ 成立 |
| ①' 用户样本输出 token ≤ 原文 96 | 实测 107 > 96 | ❌ **不成立，且原理上不可达** |
| ② 占位符零信息量、无还原路径 | 图例仅含类型语义，安全契约测试守护 | ✅ 成立 |
| ③ 全量回归 + 审计全绿 | 见上 | ✅ 成立 |

**①' 证伪的根因（第一性原理第 2 问的深层教训）**：被掩码的短高频词（`wiimu`）本身仅
≈1 token，而任何占位符 ≥2 token——**对短重复标识符做掩码，token 膨胀是安全功能的固有
成本，不是实现缺陷**。字节口径 429B vs 旧方案 440B（-11B）已改善，但绝对不膨胀在保持
脱敏语义的前提下无解。该结论修正验收口径为「相对旧占位符净收益>0 + 膨胀最小化」；
绝对零膨胀的出路在架构级远期项（脱敏后再分发，让清单族在脱敏产物上继续压缩），
以及第一性原理的边界认识：**安全 > 节省，节省的裁判域是「未被安全功能触碰的部分」**。
