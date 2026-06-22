# Audit LLM Knowledge-Base 重构设计

> 状态：草稿 v2（待评审）
> 适用范围：`scripts/audit_sample_case_quality.py`、`scripts/audit_case_metrics.py`、未来新增的 LLM 审计脚本
> 不在范围：CLI 入口、压缩引擎、`tokenslim` workspace 工具本身
> 相对 v1 的变更：
> 1. 插件字典**复用** `docs/audit/plugin_capability_index.json`，不再新建 `tokenslim_kb/plugins/*.yaml`
> 2. case 场景改用 **sidecar** 形式 `samples/<plugin>/case_NNN_xxx.scenario.yaml`，不再集中到 `tokenslim_kb/scenarios/`
> 3. 新增 **Preflight Drift Detection**（§12）—— 7 条漂移轴，新插件/新 case/缺 sidecar 全部覆盖
> 4. 新增 **Source Code Parsing Strategy**（§13）—— 决定用 regex + 3 道护栏，**不**用 tree-sitter
> 5. 读取层 import 自 `scripts/generate_plugin_capability_index.py`（.ps1 已废弃），不重写 file walking

---

## 1. 背景与目标

### 1.1 现状问题

`scripts/` 下两个 audit 脚本都把 LLM 当成"通用审计员"用，但 LLM 实际上**不认识 TokenSlim**：

- `audit_sample_case_quality.py` 的 LLM_SYSTEM_PROMPT 在 prompt 内硬编码 shell 专属规则（R1-R7 是 shell prompt 风格、错误格式、locale），靠 `TYPE_REALISM_RULES` 做类型 dispatch。
- `audit_case_metrics.py` 用 `TOKENSLIM_AUDIT_SYSTEM_PREFIX` + `tactical_rules` 拼装 system prompt。
- 两者各自实现了基本相同的 `call_llm`（env 读取、HTTP、retry、markdown 围栏剥离），共 ~145 行重复代码。
- 1000+ 个 case 的审计完全靠"通用 prompt"做真实性判定，**没有**项目专属知识：
  - LLM 不知道 TokenSlim 是压缩库 vs. 备份库 vs. CI 库。
  - LLM 不知道这个 case 在该插件的 sample 矩阵里"目标场景"是什么。
  - LLM 不知道"这条 case 期望证明的能力"是什么。
- `audit_sample_case_quality.py` 只审计**单插件** case 内容，**不**审计 samples/ 与 `mod.rs` / `config/` / `tokenslim_kb/architecture.yaml` / `plugin_capability_index.json` 的**架构一致性**。后果：
  - 新增 `samples/<new_plugin>_plugin/` 不会被发现
  - 新增 `case_NNN_xxx.log` 但忘注册到 `showcase.rs` 不会被发现（除了现有的 `not_registered` 状态）
  - 案例缺 sidecar 不会被发现
  - `plugin_capability_index.json` 过期不会被提示

### 1.2 设计目标

- **G1（去重）**：把 LLM HTTP 调用、env 解析、retry、markdown 剥离逻辑收拢到一处。
- **G2（外置）**：把硬编码在 .py 里的 prompt 模板抽到磁盘 `.md` / `.yaml` 文件，0 代码改动即可编辑。
- **G3（知识库）**：用结构化 YAML 描述项目身份、case 场景（sidecar），作为 LLM system prompt 的"上下文注入层"。插件字典**复用** `plugin_capability_index.json`，不重复造。
- **G4（cascade）**：缺文件时自动 fallback，不让"忘建一个 yaml"导致 LLM 审计彻底断掉。
- **G5（回归安全）**：重构后 shell_session 状态 63/10/4/3、跨插件 100% valid 基线**零回退**。
- **G6（可扩展）**：新增 audit 目标 / 插件类型 / 提示词版本时，不改 audit caller 代码。
- **G7（架构漂移检测）**：在 `audit_sample_case_quality.py` 入口加 preflight 阶段，**7 条漂移轴**自动检测：新插件 / 新 case / case 缺 sidecar / ghost case / KB 漂移 / cap_index 过期 等。跑一次脚本发现所有架构不一致。

### 1.3 非目标

- 不替代 `audit_case_metrics.py` 的语义对齐（semantic gate）逻辑。
- 不重写"压缩比 / 语义冻结"等数值指标。
- 不改 `audit_all_case_metrics.py`（它本身不调 LLM，只是聚合）。
- 不改 `showcase.rs` / sample 物理文件。

---

## 2. 架构总览

### 2.1 现状代码地图

```
scripts/
  audit_sample_case_quality.py   # case 质量审计（前置门禁）
    ├─ call_llm()                # L1486-1548   重复点 A
    ├─ LLM_SYSTEM_PROMPT*        # L1236-1444   重复点 B
    ├─ TYPE_REALISM_RULES dict   # L1568-1800   重复点 C
    ├─ build_llm_prompt(type)    # L1824-1834
    ├─ build_llm_user_prompt()   # L1551-1567
    └─ normalize_llm_judgment()  # L1881        业务专属
  audit_case_metrics.py          # 压缩语义审计（后置门禁）
    ├─ call_llm()                # L465-549     重复点 A
    ├─ build_llm_system_prompt() # L110-122     重复点 B
    └─ test_llm_gate()           # L555         业务专属
  audit_all_case_metrics.py      # 聚合执行器（不调 LLM）
  audit_review_prompt.md         # 已有：审计员使用的 markdown 提示词
  prompts/                       # 已有：VCS 类战术提示词（独立于本设计）
```

### 2.2 重构后代码地图

```
scripts/
  audit_llm_common.py            # ★ 新建：基座（见 §3）
  tokenslim_kb/                  # ★ 新建：项目知识库（见 §4）— 仅 2 个全局文件
    project.yaml                 # 身份卡（必填）
    architecture.yaml            # 模块拓扑（可从 plugin_capability_index 派生）
  prompts/audit/                 # ★ 新建：审计 LLM 提示词模板（见 §5）
    case_quality/
      _base.md / _footer.md
      shell.md / access_log.md / data_struct.md / vcs.md / build.md / error_trace.md / default.md
    case_metrics/
      _base.md
      <plugin_type>.md
  audit_sample_case_quality.py   # 改：preflight 阶段新增 + LLM 代码段 import（见 §7）
  audit_case_metrics.py          # 改：替换 LLM 代码段为 import（见 §7.2）
  audit_all_case_metrics.py      # 不动

samples/<plugin>/                                  # 物理 case（不变）
  case_NNN_xxx.log                                 #   现有
  case_NNN_xxx.scenario.yaml                       # ★ 新增 sidecar（LLM 场景上下文）

docs/audit/                                        # 现有审计产物
  plugin_capability_index.json                     # ★ 复用为插件字典（生成器是 generate_plugin_capability_index.py）
  <plugin>/*.latest.json / frozen_cases.json       # 既有

scripts/                                           # 现有
  generate_plugin_capability_index.py              # ★ 复用为"读函数"源（写文件仍由它做）
```

### 2.3 数据流

```
[audit_sample_case_quality.py]
  │
  ├─ 0. Preflight（新增，见 §12）
  │     import generate_plugin_capability_index.{count_case_files, get_showcase_text, ...}
  │     import audit_llm_common.{parse_mod_rs_registry, walk_physical_samples_with_sidecar, ...}
  │     → drift_findings[]   (print + 落 docs/audit/<plugin>/drift/drift_report.json)
  │     exit(1) if --strict-drift
  │
  ├─ 1. build_case_quality_prompt(plugin, case_id)
  │     │  cascade load:
  │     ├── project = load(tokenslim_kb/project.yaml)            ← 必填
  │     ├── architecture = load(tokenslim_kb/architecture.yaml)  ← 可派生
  │     ├── plugin_index = load(docs/audit/plugin_capability_index.json)
  │     │     → 找到 plugin 转 narrative 注入 prompt
  │     ├── scenario = load(samples/<plugin>/<case_id>.scenario.yaml)  ← sidecar，缺则兜底
  │     ├── base + type_specific + footer from scripts/prompts/audit/
  │     <─ system_prompt
  │
  ├─ 2. call_llm_chat(config, user_prompt, system_prompt)
  │     → HTTP POST chat/completions
  │     → 401/403 raise / 408/429/5xx retry
  │     <─ parsed JSON dict
  │
  └─ 3. normalize_llm_judgment (业务专属)
```

---

## 3. 公共基座 `audit_llm_common.py`

### 3.1 导出 API

#### 3.1.1 LLM 调用（既有）

```python
@dataclass
class LLMConfig:
    api_key: str
    base_url: str
    model: str
    max_tokens: int
    timeout: int
    retries: int
    retry_sleep: int
    json_mode: bool
    reasoning_effort: Optional[str]   # o1/o3 模型专用
    audit_kind: str                  # "case_quality" | "case_metrics" | 自定义

    @classmethod
    def from_env(cls, env, *, max_tokens=1024, timeout=600, retries=3, retry_sleep=3,
                 audit_kind="case_quality") -> "LLMConfig": ...

def call_llm_chat(config: LLMConfig, user_prompt: str, system_prompt: str,
                  *, max_tokens_override: Optional[int] = None,
                  reasoning_effort_override: Optional[str] = None) -> Optional[dict]:
    """统一 HTTP 调用。返回已 parse 的 JSON dict；失败返回 None。"""

def add_llm_args(parser: argparse.ArgumentParser) -> None:
    """注册 --llm-audit / --require-llm-audit / --allow-llm-missing。"""

def has_llm_available(env) -> bool:
    """是否配置了 OPENAI_API_KEY。"""

def load_prompt_template(audit_kind: str, plugin_type: str) -> Tuple[str, str]:
    """cascade 加载 (base, type_specific) prompt 模板。"""

def build_case_quality_prompt(plugin_type: str,
                              plugin: str = "",
                              case_id: str = "",
                              kb: Optional[dict] = None) -> str:
    """给 audit_sample_case_quality.py 用的 system prompt 拼装器。
    kb=None 时按需从 tokenslim_kb/ + samples/<plugin>/sidecar 加载。"""

def build_case_metrics_prompt(plugin_type: str,
                              tactical_rules: str = "",
                              kb: Optional[dict] = None) -> str:
    """给 audit_case_metrics.py 用的 system prompt 拼装器。"""
```

#### 3.1.2 知识库加载（新增）

```python
def load_yaml(path: str) -> Optional[dict]:
    """cascade 加载。文件不存在返回 None；解析错误打 warning 后返回 None。"""

def load_project_kb() -> dict:
    """加载 tokenslim_kb/project.yaml。缺则 raise。"""

def load_plugin_capability_index(path: str = "docs/audit/plugin_capability_index.json") -> Optional[dict]:
    """加载生成器写的索引 JSON。"""

def find_plugin_in_index(index: dict, plugin: str) -> Optional[dict]:
    """从索引中找单个 plugin 记录，找不到返回 None。"""

def plugin_index_to_narrative(plugin_record: dict) -> str:
    """把 {description, capability_tags, detect_patterns, route_keywords, priority} 
    拼成 LLM 友好的 narrative 段落（~10 行英文）。"""

def load_case_scenario_sidecar(samples_dir: str, plugin: str, case_id: str) -> Optional[dict]:
    """加载 samples/<plugin>/<case_id>.scenario.yaml，缺返回 None。"""
```

#### 3.1.3 源码 / 物理文件读取（新增，给 preflight 用）

```python
def parse_mod_rs_registry(mod_rs_path: str) -> Set[str]:
    """从 src/plugins/mod.rs 抓取所有 `pub mod <name>;` 声明。
    见 §13 regex 策略 + 3 道护栏。"""

def walk_physical_samples(samples_dir: str) -> Dict[str, List[str]]:
    """samples/ → {plugin_name: [case_filename, ...]}。
    委托 generate_plugin_capability_index.count_case_files() 实现，不重写。"""

def walk_physical_samples_with_sidecar(samples_dir: str) -> Dict[str, Dict[str, bool]]:
    """samples/ → {plugin_name: {case_filename: has_sidecar_bool}}。
    1000+ 文件遍历，O(n) 不分块。"""

def list_ghost_cases(samples_dir: str, plugin: str) -> List[str]:
    """在 showcase.rs 登记但 samples/<plugin>/ 下找不到 .log 文件的 case_id 列表。"""

def find_capability_index_stale(samples_dir: str, source_dir: str,
                                index_path: str) -> bool:
    """比较 samples/ + src/ 最新 mtime 与 plugin_capability_index.json 的 generated_at。
    True 表示 cap_index 过期。"""
```

### 3.2 行为契约

| 行为                          | 契约                                                                    |
| ----------------------------- | ----------------------------------------------------------------------- |
| API key 缺失                  | `call_llm_chat` 返回 `None`（**不抛异常**），由 caller 决定走 lint-only |
| 401/403                       | 立即 raise `AuthenticationError`（**不重试**）                          |
| 408/429/5xx                   | 重试 `retries` 次，指数退避 `retry_sleep * 2^attempt + jitter`          |
| 4xx（除 401/403/408/429）     | 立即 raise `NonRetryableHTTPError`                                      |
| JSON 解析失败                 | 返回 `None`（**不抛**），由 caller 降级                                 |
| Markdown 围栏剥离             | 识别 `` ```json ... ``` `` 和 `` ``` ... ``` `` 两种                    |
| `response_format=json_object` | 默认开启；`json_mode=False` 时关闭                                      |
| `reasoning_effort`            | 仅当模型名包含 `o1` / `o3` 时附加                                       |

### 3.3 错误类型

```python
class LLMError(Exception): pass
class AuthenticationError(LLMError): pass       # 401/403
class NonRetryableHTTPError(LLMError): pass    # 其他 4xx
class LLMCallExhausted(LLMError): pass         # 重试用尽
```

caller 只需 `try/except LLMError` 兜底（除非想区分鉴权失败 vs 临时网络）。

---

## 4. 知识库 `tokenslim_kb/`

### 4.1 四层数据契约

#### Layer 1: `project.yaml`（项目身份卡）

```yaml
project: tokenslim
display_name: "TokenSlim"
mission: "通用结构化日志/命令输出压缩库，把 100KB raw log 压到 5KB 同时保留可读性"
tagline: "Compress logs. Keep intent."

principles:
  - "绝对保留：prompt / 错误信息 / 退出码 / 关键路径 / 时间戳"
  - "推荐压缩：ANSI 颜色 → 去；spinner → 合并；重复 warn → 计数"
  - "防失忆红线：移除前必须先在 dict 留下路径"

target_users:
  - "SRE 排查事故时给 LLM 喂日志"
  - "DevOps CI 跑完后灌给 LLM 分析"
  - "log-heavy agent 开发者减少 token 消耗"

non_goals:
  - "不是 compression（gzip/zstd）替代品——只做结构化精简"
  - "不做语义改写或翻译"
  - "不做 LLM 缓存或 routing 决策"

data_formats_supported:
  - shell_session   # 主战场
  - access_log      # nginx / apache / CLF
  - data_struct     # YAML / JSON / XML / ndjson
  - vcs             # git / svn / hg / p4 等命令输出
  - build           # gcc / xcode / maven / cargo / dotnet
  - error_trace     # Python / Node / Java / PHP 错误栈
```

#### Layer 2: `architecture.yaml`（模块拓扑）

```yaml
routing:
  principle: "专用插件优先 → 通用 shell 兜底"
  priority_higher_first: true
  detection_phase: "plugin dispatcher 在压缩前判定"

plugin_hierarchy:
  - family: vcs
    members: [vcs_git_plugin, vcs_svn_plugin, vcs_hg_plugin, ...]
  - family: build
    members: [gcc_log_plugin, xcode_log_plugin, ...]
  - family: error_trace
    members: [python_traceback_plugin, node_error_plugin, ...]
  - family: data_struct
    members: [yaml_plugin, json_plugin, ...]
  - family: access_log
    members: [web_log_plugin, syslog_plugin, ...]
  - family: shell
    members: [shell_session_plugin]   # 唯一兜底

shell_session_plugin_role: "未被专用插件识别的命令 session"
compression_pipeline:
  - "detect → 选插件"
  - "compress → 各插件专属压缩"
  - "freeze → 写 compact"
  - "rehydrate → 还原时按需展开"
```

#### Layer 3：插件字典（**复用** `docs/audit/plugin_capability_index.json`）

不再单独维护 `tokenslim_kb/plugins/<plugin>.yaml`。原因：
- 现有 `generate_plugin_capability_index.py` 已 100% 自动生成结构化 JSON（含 61 个插件的 `description` / `capability_tags` / `detect_patterns` / `route_keywords` / `priority`）。
- 单写一份 yaml 会出现"两个真相源"漂移。

**新职责链**：
```
generate_plugin_capability_index.py    ←  生成器
   ↓ 写
docs/audit/plugin_capability_index.json  ← 单一来源
   ↓ 读
audit_llm_common.find_plugin_in_index(...)
audit_llm_common.plugin_index_to_narrative(...)   ← 转成 LLM 友好段落
```

`plugin_index_to_narrative` 输出示例（注入到 prompt 的 `{{ plugin.narrative }}`）：

```text
Plugin: shell_session_plugin
Type: shell
Priority: 10
Mission: Compress arbitrary shell session blocks into compact readable form; 
         acts as the fallback for unrecognized command families.
Capability tags: prompt-preserve, error-block-preserve, padding-collapse
Detection: First line must look like a shell prompt (`PS C:\> ...`, 
           `user@host:~$`, or `C:\...>`).
Routes first to: vcs, build, error_trace, data_struct, access_log (if detect_patterns match)
```

#### Layer 4：case 场景（**sidecar** 形式 `samples/<plugin>/case_NNN_xxx.scenario.yaml`）

不集中存到 `tokenslim_kb/scenarios/`，**与 case 物理文件同目录**。原因：
- 100% 跟 case 走，git diff 直观
- 增 case 必增 sidecar，缺了就漂移（preflight 自动告警）
- 跨人协作时场景注释和 case 文件 PR 一起 review

```yaml
# case_001_bash_success.scenario.yaml
case_id: case_001_bash_success
plugin: shell_session_plugin
generated_from: "showcase.rs + case_id naming"
generated_at: "2026-06-08T10:00:00Z"

scenario: "bash 简单 ls -l 成功，3-5 行"
target_capability: "压缩 padding + 保留时间戳 + 路径"
expected_keep:
  - "./src"
  - "drwxr-xr-x"
expected_compress:
  - "ls -l 颜色字符"
  - "size padding 空格"

# 注释规范：scenario 是"该 case 试图证明什么"，不是"case 实际内容"
```

**fallback 行为**（缺 sidecar 时）：
- preflight 阶段：漂移告警（不阻塞）
- LLM 注入：跳过 `{{ scenario.* }}` 段，只注入 plugin 的 mission / 检测规则
- 不影响 lint-only 跑（仅缺 scenario context，case 仍可审计）

### 4.2 Cascade 加载顺序

```
build_case_quality_prompt(plugin, case_id):
  1. project = load_yaml(tokenslim_kb/project.yaml)
     ↓ 缺则 raise FileNotFoundError（项目身份卡必须有）

  2. architecture = load_yaml(tokenslim_kb/architecture.yaml)
     ↓ 缺则用 {}（架构上下文是 nice-to-have，可从 cap_index 派生）

  3. plugin_record = find_plugin_in_index(load_plugin_capability_index(), plugin)
     ↓ 缺则 narrative = "[no plugin capability index available; judge on structural rules only]"

  4. scenario = load_case_scenario_sidecar(samples_dir, plugin, case_id)
     ↓ 缺则 scenario = None（preflight 会漂移告警，prompt 走 fallback）

  5. base = read(prompts/audit/case_quality/_base.md)
     ↓ 缺则用代码内嵌 fallback string

  6. type_specific = read(prompts/audit/case_quality/<plugin_type>.md)
     ↓ 缺则用 prompts/audit/case_quality/default.md
     ↓ 也缺则用代码内嵌 fallback string

  7. footer = read(prompts/audit/case_quality/_footer.md)
     ↓ 缺则用代码内嵌 fallback string

  8. 模板替换：
     system_prompt = base
                  .replace("{{ project.mission }}", project.mission)
                  .replace("{{ plugin.narrative }}", plugin_index_to_narrative(plugin_record))
                  .replace("{{ scenario.scenario }}", scenario["scenario"] if scenario else "[no scenario sidecar]")
                  ...
                  + "\n\n" + type_specific
                  + "\n\n" + footer
```

**核心原则**：永远不抛"文件缺失"异常（除了 project.yaml），所有外部资源都有 fallback。

---

## 5. 提示词模板

### 5.1 `prompts/audit/case_quality/_base.md`

```markdown
# Your Identity

You are TokenSlim's **sample case quality auditor**.

**TokenSlim** is {{ project.display_name }} — {{ project.mission }}.

This audit is its **pre-compression gate**: it determines whether a physical
sample file under `samples/<plugin>/` is real, well-shaped, and decision-useful
for the plugin's compression algorithm.

**Tagline**: {{ project.tagline }}

# Audit principles

You operate as a "real-environment witness proxy". The user trusts you to flag
cases that look LLM-fabricated rather than captured from a real terminal session.

You are not writing documentation. You are not editing files. You judge.

# Plugin under review (from docs/audit/plugin_capability_index.json)

{{ plugin.narrative }}

# Per-case target

{% if scenario %}
- **Target scenario**: {{ scenario.scenario }}
- **Capability this case aims to prove**: {{ scenario.target_capability }}
- **Expected to be kept**: {{ scenario.expected_keep }}
- **Expected to be compressed**: {{ scenario.expected_compress }}
{% else %}
- **No scenario sidecar available** (missing `samples/<plugin>/<case_id>.scenario.yaml`).
  Judge on structural / realism rules only.
{% endif %}
```

### 5.2 `prompts/audit/case_quality/shell.md`（类型专属真实性规则）

把现 `TYPE_REALISM_RULES["shell"]` 的 R1-R7 块从 .py 抽到这里（见 §7.1.3）。

### 5.3 `prompts/audit/case_quality/_footer.md`

```markdown
# Decision discipline

- If ANY realism tell exists, return status="fabricated" and list the tells in
  `realism_audit.fabrication_indicators`.
- "fabricated" can coexist with "valid" structurally. A case can pass
  structural rules and still be fabricated. Return "fabricated" and let the
  synthesis step surface both.
- "fabricated" is content-level; routing / not_registered / duplicate take
  priority in synthesis.

# Required JSON schema

```json
{
  "status": "one of nine labels (including 'fabricated')",
  "confidence": 0.0-1.0,
  "explanation": "中文 1-3 句解释",
  "fix_hint": "简短修复建议或空字符串",
  "duplicate_of": "case_xxx_... or empty",
  "realism_audit": {
    "shell_personality_ok": bool,
    "line_length_entropy_ok": bool,
    "error_cause_effect_ok": bool,
    "error_multiline_ok": bool,
    "empty_output_authentic": bool,
    "exit_stderr_consistent": bool,
    "locale_plausible": bool,
    "fabrication_indicators": ["具体 tell #1", ...]
  }
}
```

# Output

Return JSON only. No markdown wrapper.
```

### 5.4 `prompts/audit/case_metrics/_base.md`（给 audit_case_metrics.py 用）

```markdown
# Your Identity

You are TokenSlim's **semantic compression auditor**.

TokenSlim is {{ project.mission }}.

This audit is its **post-compression gate**: it verifies that the compressed
output preserved all decision-critical semantics relative to the original.

# Plugin under test (from docs/audit/plugin_capability_index.json)

{{ plugin.narrative }}

# Tactical rules (per-plugin-type)

{{ tactical_rules }}

# Required JSON schema

```json
{
  "verdict": "pass" | "fail" | "warning",
  "lost_semantics": ["what was lost"],
  "preserved_critical": ["what was kept"],
  "fix_hint": "...",
  "confidence": 0.0-1.0
}
```
```

---

## 6. 治理与约束

### 6.1 单一来源原则

- **不允许** 任何 .py 文件再硬编码完整 system prompt 字符串。
- **允许** .py 文件保留：
  - cascade 加载逻辑
  - 模板替换逻辑
  - 业务专属 `normalize_*` 函数
- 兜底用的"代码内嵌最小 stub"必须显式注释 `# FALLBACK: prefer file-based template; this string is the last resort`。

### 6.2 提示词版本化

- `prompts/audit/case_quality/shell.md` 顶部必须有 `version: v1` 注释行。
- 修改提示词时必须 bump version，方便回归对比。
- LLM 调用结果写回 JSON 时带上 `prompt_version` 字段。

### 6.3 知识库准入

- `tokenslim_kb/project.yaml` **必须由人工维护**（机器生成 = 幻觉源头）。
- `tokenslim_kb/architecture.yaml` 50% 人工 + 50% 可从 `plugin_capability_index.json` 派生。允许生成器辅助生成。
- `docs/audit/plugin_capability_index.json` 由 **`scripts/generate_plugin_capability_index.py`** 自动生成，是插件字典的**单一真相源**。本设计不重写。
- `samples/<plugin>/case_NNN_xxx.scenario.yaml` **100% 可从 `showcase.rs` 标题 + case_id 命名 + case 文件首行哈希自动生成**（`scenario` 段先用 LLM 一次跑出底稿，再人工校对；`expected_keep` / `expected_compress` 完全手写）。
- sidecar 是**审计增强**而非**审计前置**：缺 sidecar → preflight 漂移告警 + LLM 走 fallback 上下文，case 仍可审计。
- **`tokenslim_kb/plugins/` 和 `tokenslim_kb/scenarios/` 不再存在**（v1 设计废除）。

### 6.4 Schema 校验（可选，第二阶段）

- 提供 `tokenslim_kb/schemas/plugin.schema.json`、`tokenslim_kb/schemas/scenario.schema.json`。
- `audit_llm_common.py` 加载时用 `jsonschema` 校验（如果可用），不通过则打 warning 但**不阻塞**（保持 cascade 原则）。

### 6.5 兼容性

- 旧 `audit_sample_case_quality.py` 的 `LLM_SYSTEM_PROMPT` 保留为模块级 constant，等价于"`build_case_quality_prompt("shell")` 在所有文件都缺失时"的输出。
- 旧 `audit_case_metrics.py` 的 `TOKENSLIM_AUDIT_SYSTEM_PREFIX` 同上。
- 现有所有 `--llm-audit` / `--require-semantic-gate` 命令行参数**保持不变**。

---

## 7. 改造范围（按脚本拆分）

### 7.1 `audit_sample_case_quality.py`

#### 删除（搬到 `audit_llm_common.py`）
- `LLM_SYSTEM_PROMPT`（L1236-1386）
- `LLM_SYSTEM_PROMPT_BASE`（L1391-1444）
- `LLM_SYSTEM_PROMPT_FOOTER`（L1445-1484）
- `call_llm()`（L1486-1548）
- `TYPE_REALISM_RULES`（L1569-1822）
- `build_llm_prompt()`（L1824-1834）
- `build_llm_user_prompt()`（L1551-1567）

#### 保留（业务专属）
- `normalize_llm_judgment()`（L1881）
- 主循环里的 `llm_judgment["status"]` 判定逻辑
- `--llm-audit` / `--require-llm-audit` / `--allow-llm-missing` argparse 行为（参数定义改用 `add_llm_args(parser)`）

#### 新增
- `from audit_llm_common import call_llm_chat, build_case_quality_prompt, add_llm_args, has_llm_available, LLMConfig`
- **Preflight 阶段**（见 §12、`§7.4`）：在主审计循环前调用 `drift_audit()`，输出 `drift_findings` 并落 `docs/audit/<plugin>/drift/drift_report.json`。
- `--strict-drift` argparse：有 preflight finding 时 exit(1)。

#### 文件大小预期
- 改造前：2843 行
- 改造后：~2580 行（~340 行搬到 common + ~75 行新增 preflight）

### 7.2 `audit_case_metrics.py`

#### 删除
- `call_llm()`（L465-549）
- `build_llm_system_prompt()`（L110-122）

#### 保留
- `TOKENSLIM_AUDIT_SYSTEM_PREFIX`（L48）作为 fallback constant
- `test_llm_gate()` 业务逻辑（改用 `call_llm_chat`）
- `--require-semantic-gate` argparse 行为

#### 新增
- `from audit_llm_common import call_llm_chat, build_case_metrics_prompt, LLMConfig`

### 7.3 `audit_all_case_metrics.py`

- **不动**。它是聚合执行器，间接调上面两个脚本。

### 7.4 Preflight 阶段（新增）

见 §12 详述。核心增量：

```python
# 在 audit_sample_case_quality.py main() 入口、参数解析之后立即调用
from generate_plugin_capability_index import (
    count_case_files, parse_showcase_rs_cases, get_showcase_text,
)
from audit_llm_common import (
    parse_mod_rs_registry, walk_physical_samples_with_sidecar,
    list_ghost_cases, find_capability_index_stale, drift_audit,
    load_plugin_capability_index,
)

def preflight(plugin: str, args) -> List[Dict]:
    findings = drift_audit(
        plugin=plugin,
        samples_dir=args.samples_dir,
        source_dir=args.source_dir,
        mod_rs_path="src/plugins/mod.rs",
        cap_index_path="docs/audit/plugin_capability_index.json",
    )
    if findings:
        for f in findings:
            print(f"  [drift] {f['severity']:7s} {f['axis']:24s} {f['message']}")
    return findings

# main()
findings = preflight(plugin, args)
if args.strict_drift and findings:
    sys.exit(1)
```

---

## 8. 实施计划（7 个阶段，~11 工作日）

### 阶段 0：源码读取层抽离（**新插入**，0.5 天）

- 在 `scripts/audit_llm_common.py` 里实现 `parse_mod_rs_registry`、`walk_physical_samples_with_sidecar`、`list_ghost_cases`、`find_capability_index_stale`（见 §13 regex 策略）。
- 实现 `drift_audit()` 聚合函数（7 条漂移轴，见 §12）。
- 单元测试：用 `tests/fixtures/synthetic_repo/`（含故意造出的 5 处漂移）验证 preflight 找出全部。

**Gate**：合成 repo 上 `drift_audit()` 找出 ≥ 5/5 漂移；干净 repo 上 0 findings。

### 阶段 1：基座（0.5 天）

- 新建 `scripts/audit_llm_common.py`
- 写 `LLMConfig` / `call_llm_chat` / 错误类型
- 写 `add_llm_args` / `has_llm_available`
- 写 `load_prompt_template` cascade 框架（先不接业务）
- 写 `load_yaml` / `load_project_kb` / `load_plugin_capability_index` / `load_case_scenario_sidecar`（stage 3 也会用到，先写好）
- 单元测试：`test_audit_llm_common.py`
  - mock urllib，验证 401/403/5xx/超时 重试逻辑
  - 验证 markdown 围栏剥离
  - 验证 fallback cascade
  - 验证 cascade 加载顺序在缺 project.yaml 时 raise、其他缺文件时降级

**Gate**：单元测试通过 + `python -c "from audit_llm_common import LLMConfig, drift_audit"` 不报错。

### 阶段 2：抽 `prompts/audit/` 目录（0.5 天）

- 新建 `scripts/prompts/audit/case_quality/` + `case_metrics/`
- 把现 `LLM_SYSTEM_PROMPT_BASE` / `FOOTER` 拆到 `_base.md` / `_footer.md`
- 把 6 套 `TYPE_REALISM_RULES` 抽到 `shell.md` / `access_log.md` / `data_struct.md` / `vcs.md` / `build.md` / `error_trace.md` / `default.md`
- 把 `TOKENSLIM_AUDIT_SYSTEM_PREFIX` 抽到 `case_metrics/_base.md`
- **不动** `audit_sample_case_quality.py` / `audit_case_metrics.py` 任何业务代码

**Gate**：`python -c "import audit_llm_common; print(audit_llm_common.build_case_quality_prompt('shell', plugin='shell_session_plugin'))"` 输出与原 `LLM_SYSTEM_PROMPT` **字节级一致**。

### 阶段 3：建 `tokenslim_kb/` + sidecar 工具（1.5 天）

- 写 `project.yaml`（人工）
- 写 `architecture.yaml`（人工 + 工具辅助派生）
- 写 `scripts/generate_case_scenario_sidecars.py`：
  - 输入：`showcase.rs` 标题 + case_id 命名 + case 文件首行
  - 输出：每个 `samples/<plugin>/case_NNN_xxx.scenario.yaml` 的底稿（scenario 段先用 LLM 跑一次 + 人工校对；expected_keep/expected_compress 完全手写）
- 跑 `audit_sample_case_quality.py --plugin shell_session_plugin --llm-audit`，对比改造前后的 LLM 判定一致性（用 `--save-llm-prompt` 把 prompt 落地 + diff）

**Gate**：shell_session 80 cases 的 LLM 判定结果与改造前**重合度 ≥ 90%**。

### 阶段 4：改 `audit_sample_case_quality.py`（1.5 天）

- 接入 preflight 阶段（`drift_audit` + `--strict-drift`），加 ~75 行
- 删除 L1236-1834 的所有 LLM 相关代码
- 改 import：`from audit_llm_common import ...`
- 改 3 个调用点：
  - `call_llm_chat(config, user_prompt, system_prompt)` 替换 `call_llm(env, prompt, system_prompt=...)`
  - `build_case_quality_prompt(plugin_type, plugin=..., case_id=...)` 替换 `build_llm_prompt(plugin_type)` + `build_llm_user_prompt(...)`
  - `add_llm_args(parser)` 替换 3 个手写 argparse

**Gate**：shell_session 状态分布 63/10/4/3 不变 + 跨插件 needs_fix 不回退（参考基线表 §9.1）+ preflight 在干净 repo 上 0 findings。

### 阶段 5：改 `audit_case_metrics.py`（0.5 天）

- 删除 L110-122、L465-549
- 改 import
- 改 1 个调用点：`call_llm_chat(...)` + `build_case_metrics_prompt(...)`

**Gate**：`--require-semantic-gate` 行为与改造前一致（跑 1 个 case 验证）。

### 阶段 6：跨插件回归 + 文档（1 天）

- 跑 `audit_all_case_metrics.py` 全量
- 对比改造前后 needs_fix 分布表（见 §9.1）
- 抽样 5-10 个 case 做 LLM 评审对比（diff 改造前后 LLM 输出）
- 跑 `generate_plugin_capability_index.py` 验证 cap_index 仍可正常生成（避免我新增的 sidecar walker 把它搞坏）
- 写 `docs/design/audit_llm_kb_design.md` 之外的 README（如果需要）
- 在 `CLAUDE.md` 加一条："新增 LLM 审计脚本时，import `audit_llm_common`"

**Gate**：基线表 §9.1 零回退 + 抽样 LLM 评审差异在"可解释"范围内 + cap_index 生成器未受影响。

---

## 9. 验收标准

### 9.1 基线表（重构后必须保持）

| 插件                                                                                                                                                                                                                                                                                                                   | Cases | 状态分布（valid/routing/title/needs_fix/...） |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- | --------------------------------------------- |
| shell_session                                                                                                                                                                                                                                                                                                          | 80    | 63/10/3/4                                     |
| web_log                                                                                                                                                                                                                                                                                                                | 48    | 48/0/0/0                                      |
| vcs_git                                                                                                                                                                                                                                                                                                                | 87    | 87/0/0/0                                      |
| yaml                                                                                                                                                                                                                                                                                                                   | 14    | 12/0/0/2                                      |
| php_ruby                                                                                                                                                                                                                                                                                                               | 12    | 12/0/0/0                                      |
| nodejs                                                                                                                                                                                                                                                                                                                 | 23    | 22/0/0/1                                      |
| android_gradle                                                                                                                                                                                                                                                                                                         | 22    | 21/0/0/1                                      |
| xcode_log / git_diff / xml_html / python_traceback / json / node_error / ndjson / maven / java_stack / pytest / rust_go / gcc / bazel / dotnet / spring_boot / unity_unreal / webpack_vite / ci_log / ansible / terraform / kubernetes_docker / markdown / protobuf / template_driven / cloudformation / helm / pulumi | —     | 100% valid                                    |

### 9.2 量化收益

- 重复 `call_llm` 实现：从 2 处 → 1 处（消除 ~85 行重复）
- prompt 硬编码字符串：从 2 处 → 0 处（全部外置到 .md）
- 重复 file walker（samples/）实现：从 2 处 → 1 处（sidecar walker 是新加的；count_case_files 复用 generate_plugin_capability_index.py）
- 换 LLM 模型所需修改：`scripts/audit_*` × 0 + `.env` × 1
- 换插件所需修改：`scripts/audit_*` × 0 + `docs/audit/plugin_capability_index.json` × 0（生成器自动更新）
- 换审计目标（某 case 场景）所需修改：`scripts/audit_*` × 0 + `samples/<plugin>/<case_id>.scenario.yaml` × 1
- 新增 LLM 审计脚本接入成本：~10 行 import
- **新增**：跑一次 `audit_sample_case_quality.py` 自动发现 7 类架构漂移（不需 ls/手动对比）

### 9.3 不可接受回退

- ✗ shell_session 状态分布变化
- ✗ 跨插件 100% valid 插件数量减少
- ✗ LLM 误判率上升（用 §8 阶段 3 的 diff 验证）
- ✗ `audit_all_case_metrics.py` 全量时间增加 > 2x
- ✗ preflight 在干净 repo 上产生 false positive（"我有空文件 sidecar 漂移但其实那是 empty 边界"）
- ✗ preflight 把已知的"故意漂移"（eg. 一次性 demo plugin）当 error 阻塞
- ✗ cap_index 生成器被 sidecar walker 改坏（生成内容变化）

---

## 10. 风险与缓解

| 风险                                                           | 概率 | 影响 | 缓解                                                                                       |
| -------------------------------------------------------------- | ---- | ---- | ------------------------------------------------------------------------------------------ |
| 抽到 .md 后 prompt 行为变化（换行 / 编码）                     | 高   | 中   | 阶段 2 字节级对比 gate                                                                     |
| cascade fallback 把"忘建的 KB"掩盖掉                           | 中   | 高   | 所有 fallback 必须有 warning log + metrics 计数                                            |
| 知识库数据漂移（手写后忘了同步）                               | 高   | 中   | 提供 `python -m tokenslim_kb verify` 校验工具（第三阶段）                                  |
| LLM 因 prompt 变长反而判定更慢/更贵                            | 中   | 低   | prompt 总长限制 ≤ 8K tokens（KB 字段约束）                                                 |
| 阶段 3 LLM 判定一致性 < 90%                                    | 中   | 中   | 抽样 10 个 case 人工审；不通过则回滚到阶段 2                                               |
| preflight regex 误把注释/字符串里的 `pub mod` 当真声明         | 中   | 高   | §13 三道护栏：剥注释 + 保守+宽容匹配 + 白名单烟雾测试                                      |
| preflight 把"故意漂移"当 error 阻塞                            | 中   | 中   | `--strict-drift` 默认 False；提供 `--drift-allow` 白名单（plugin 维度）                    |
| preflight 在 1000+ case 上遍历变慢                             | 低   | 低   | walk 用 os.scandir + 一次 os.listdir 缓存，O(n) 不分块；本地测时 50ms 以内                 |
| sidecar 文件命名漂移（case_id 含特殊字符）                     | 低   | 中   | 命名规则用 `re.fullmatch(r'case_\d{3}_[a-z0-9_]+', stem)`；不匹配则不读 sidecar 而非 raise |
| `generate_plugin_capability_index.py` 拒绝 sidecar walker 复用 | 中   | 中   | stage 0 不直接 import，先复制函数体；待 stage 6 后跟维护者协商引入正式 import              |
| LLM 不认识 `{{ plugin.narrative }}` 占位符格式                 | 低   | 低   | base prompt 顶部固定包含 "你将看到以下结构化字段" 提示，让 LLM 容错                        |

---

## 11. 评审 checklist

- [ ] G1-G7 设计目标是否覆盖
- [ ] KB 四层数据契约字段是否合理（缺/多/过细）
- [ ] cascade fallback 顺序是否符合直觉
- [ ] **§12 7 条漂移轴**是否覆盖所有用户提出的 4 类架构不一致场景（新增插件 / 新增 case / 缺 sidecar / cap_index 过期 + mod.rs 漂移 + samples 漂移 + ghost case）
- [ ] **§13 regex vs tree-sitter** 决策是否接受（"不引入额外依赖 + 3 道护栏"）
- [ ] 阶段划分（0→6，7 阶段）是否过细 / 过粗
- [ ] 9.1 基线表是否需要补其他插件
- [ ] 风险缓解措施是否完备
- [ ] 是否需要更多单元测试覆盖

---

## 附录 A：参考材料

- 现 `audit_sample_case_quality.py` L1236-1834（LLM 相关代码段）
- 现 `audit_case_metrics.py` L48, L110-122, L465-549
- 现 `scripts/audit_review_prompt.md`（审计员使用的 markdown）
- 现 `docs/prompts/*.md`（VCS 类战术提示词，可作为 prompt 模板的样例）
- 现 `scripts/generate_plugin_capability_index.py`（**唯一** plugin_capability_index.json 生成器，preflight import 它的读函数）
- 现 `docs/audit/plugin_capability_index.json`（生成器产物，作为 Layer 3 单一真相源）
- 现 `src/plugins/mod.rs`（plugin 注册清单，正则解析目标）
- CLAUDE.md 压缩协议 V1（不动，仅参考）

---

## 12. Preflight Drift Detection（前置漂移检测）

### 12.1 设计动机

用户 4 个原始诉求：
1. `audit_sample_case_quality.py` 应该发现 **samples 目录多了一个插件**
2. 应该发现 **某插件中多了一个 case**（物理文件存在但未注册到 showcase.rs）
3. 应该发现 **某 case 没有 `case_001_bash_success.scenario.yaml`**（sidecar 缺失）
4. 应该发现 `samples/architecture.yaml` / `src/plugins/` / `src/plugins/mod.rs` 三方**不一致**

外加隐含但有用的 3 条：
5. **ghost case**：showcase.rs 登记了但 `samples/<plugin>/` 找不到 .log 文件
6. **cap_index 过期**：`plugin_capability_index.json` 的 `generated_at` 早于 samples/ 或 src/ 的最新 mtime
7. **plugin 字典缺失**：被审插件在 `plugin_capability_index.json` 中**查不到**（生成器跑了但漏了该 plugin）

### 12.2 7 条漂移轴（5 源 × 1 缺失轴）

| #   | 漂移轴                                      | 检测函数                                               | severity | 阻塞？ |
| --- | ------------------------------------------- | ------------------------------------------------------ | -------- | ------ |
| 1   | `samples/` 多了一个未注册 plugin            | `walk_physical_samples()` vs `parse_mod_rs_registry()` | warning  | 否     |
| 2   | plugin 物理 case 数 ≠ showcase.rs 登记数    | `count_case_files()` vs `parse_showcase_rs_cases()`    | warning  | 否     |
| 3   | case 缺 sidecar                             | `walk_physical_samples_with_sidecar()`                 | warning  | 否     |
| 4   | showcase.rs 登记了但物理 case 缺失（ghost） | `list_ghost_cases()`                                   | warning  | 否     |
| 5   | plugin 字典缺失（cap_index 漏该 plugin）    | `find_plugin_in_index()`                               | warning  | 否     |
| 6   | cap_index 过期（mtime 检查）                | `find_capability_index_stale()`                        | info     | 否     |
| 7   | samples 架构与 mod.rs 不一致                | `parse_mod_rs_registry()` vs `walk_physical_samples()` | warning  | 否     |

**默认 severity 都是 warning**——preflight 只**打印**漂移，不影响审计主流程。`--strict-drift` 升级为 error 并 `sys.exit(1)`，作为 CI gate。

### 12.3 实现位置

- **核心读函数** 全部在 `audit_llm_common.py`（§3.1.3）。
- **聚合** `drift_audit()` 在 `audit_llm_common.py`，接收 `plugin: str` + 路径字典，返回 `List[DriftFinding]`。
- **调用** 在 `audit_sample_case_quality.py` 的 `main()` 入口，参数解析之后立即执行。
- **复用 `generate_plugin_capability_index.py`** 的 4 个读函数：`count_case_files` / `parse_showcase_rs_cases` / `get_showcase_text` / `get_showcase_mtime`。理由：避免与生成器的 file walking 逻辑两份实现不一致。

### 12.4 边界（与 `generate_plugin_capability_index.py` 的分工）

| 职责                                    | 哪个脚本                                              |
| --------------------------------------- | ----------------------------------------------------- |
| 写 `plugin_capability_index.json`       | **`generate_plugin_capability_index.py`**（不变）     |
| 写 `markdown 矩阵报告`                  | **`generate_plugin_capability_index.py`**（不变）     |
| 提供 read 函数（`count_case_files` 等） | **`generate_plugin_capability_index.py`**（导入即可） |
| 7 条漂移轴检测                          | **`audit_llm_common.py`**（新增）                     |
| preflight 入口、CLI 报告、退出码        | **`audit_sample_case_quality.py`**（新增 ~75 行）     |

### 12.5 CLI 行为

```text
$ python scripts/audit_sample_case_quality.py --plugin shell_session_plugin

[preflight] scanning drift across samples/, src/plugins/, mod.rs, cap_index
[preflight] [WARN ] samples-vs-mod-rs        Plugin 'foo_plugin' exists in samples/ but not in mod.rs
[preflight] [WARN ] sidecar-missing          3 cases in shell_session_plugin/ have no .scenario.yaml
[preflight] [INFO ] cap-index-stale          plugin_capability_index.json older than src/ (2.3h)
[preflight] 3 findings, 0 errors
[main]      running 80 case audits ...
```

```text
$ python scripts/audit_sample_case_quality.py --plugin shell_session_plugin --strict-drift
[preflight] [WARN ] sidecar-missing          ...
[preflight] 3 findings
ERROR: --strict-drift is set and 3 drift findings present. Fix or pass --drift-allow=shell_session_plugin.
exit code 1
```

### 12.6 报告落盘

每次 preflight 写一份 `docs/audit/<plugin>/drift/drift_report.json`：

```json
{
  "generated_at": "2026-06-08T12:00:00Z",
  "plugin": "shell_session_plugin",
  "findings": [
    {
      "axis": "sidecar-missing",
      "severity": "warning",
      "message": "3 cases missing scenario sidecar",
      "cases": ["case_002_xxx", "case_005_xxx", "case_017_xxx"]
    }
  ],
  "summary": {"warning": 3, "info": 0, "error": 0}
}
```

便于 PR review 看到"这次提交新加了哪条漂移"。

### 12.7 不重复造 — 与既有 `not_registered` 状态的关系

`audit_sample_case_quality.py` 主循环已经会输出 `not_registered` 状态（case 物理存在但 showcase.rs 找不到）。**preflight 不再覆盖**该状态，避免双计数；只在 LLM 审计前把 ghost case 列表**告知 LLM**：

```text
"Note: The following case_ids are listed in showcase.rs but have no physical sample file:
ghost_001, ghost_002 — please flag any related artifacts you encounter as 'ghost'."
```

注入到 `_base.md` 末尾的 `[preflight_context]` 段。

---

## 13. Source Code Parsing Strategy（regex vs tree-sitter）

### 13.1 决策

**采用正则表达式**，**不**引入 tree-sitter。理由：

| 维度             | regex                                                  | tree-sitter                                   |
| ---------------- | ------------------------------------------------------ | --------------------------------------------- |
| 依赖             | 标准库 `re`，0 依赖                                    | 需 pip install tree-sitter + 每个语言 grammar |
| 启动开销         | 0                                                      | 加载 grammar ~50ms/CI                         |
| 解析精度         | 需自己写护栏                                           | 天然 AST 正确                                 |
| 维护成本         | 一旦写好不依赖上游                                     | Rust grammar 更新跟不跟语言新语法是赌博       |
| 本场景匹配       | mod.rs 6-7 行固定格式，正则完美                        | 杀鸡用牛刀                                    |
| showcase.rs 解析 | 已有 `parse_showcase_rs_cases`（字符级 state machine） | 已稳定，零回归                                |

### 13.2 三道护栏

`parse_mod_rs_registry` 面对 3 类陷阱：

1. **`// pub mod foo;`**（行内注释）
2. **`/* pub mod foo; */`**（块注释）
3. **`String::from("pub mod foo;")`**（字符串字面量）

#### 护栏 1：剥注释（preprocess）

```python
import re

_LINE_COMMENT = re.compile(r'//[^\n]*')
_BLOCK_COMMENT = re.compile(r'/\*.*?\*/', re.DOTALL)

def strip_comments(src: str) -> str:
    src = _BLOCK_COMMENT.sub('', src)
    src = _LINE_COMMENT.sub('', src)
    return src
```

#### 护栏 2：保守+宽容匹配（regex pattern）

```python
# 匹配 pub mod <name>; 但 name 必须是合法 Rust 标识符（小写+下划线+数字）
# 且整个 `pub mod` 段不在引号里（手工保证 — 实际 mod.rs 不会有这种"在字符串里写 pub mod"的情况）
_REGISTRY_RE = re.compile(
    r'^\s*pub\s+mod\s+([a-z][a-z0-9_]*)\s*;',
    re.MULTILINE,
)

def parse_mod_rs_registry(mod_rs_path: str) -> Set[str]:
    src = Path(mod_rs_path).read_text(encoding='utf-8')
    src = strip_comments(src)
    return {m.group(1) for m in _REGISTRY_RE.finditer(src)}
```

为什么这样足够？
- `pub mod` 关键字在 Rust 里**唯一**用途就是模块声明，不会与 `let pub_mod_x = ...` 冲突（`pub` 是关键字，不允许作变量名前缀）
- `name` 限定小写起头 + `[a-z0-9_]*`，避免误吃 `pub module_xxx;`（如果以后真有这种，把 pattern 放宽）
- 注释已剥，剩下的就是真声明

#### 护栏 3：白名单 + 烟雾测试

```python
# tests/test_audit_llm_common.py
def test_parse_mod_rs_registry_against_ground_truth():
    """白名单：当前 mod.rs 应该有 39 个 plugin，名字固定。"""
    expected = {
        "shell_session_plugin", "vcs_git_plugin", "yaml", "json",
        "web_log", "node_error", "python_traceback", ...
        # 维护一个白名单，每次新加 plugin 需手动 update
    }
    got = parse_mod_rs_registry("src/plugins/mod.rs")
    missing = expected - got
    extra = got - expected
    assert not missing, f"mod.rs lost plugins: {missing}"
    # extra 不强制 fail（新增 plugin 是好事），只 warn
    if extra:
        print(f"[smoke] new plugin(s) detected: {extra}")
```

CI 跑 `pytest -k parse_mod_rs` 自动告警"白名单缺新 plugin"。维护成本：每次新增 plugin 把名字加进白名单，5 秒操作。

### 13.3 何时**应该**切到 tree-sitter

下列场景出现时重评：
- `src/plugins/mod.rs` 改成 `pub mod xxx { ... }` 内联声明（目前没有）
- 新增 `pub(crate) mod` / `pub(self) mod` 等变体
- 引入 `build.rs` 生成的 mod.rs（动态注入）
- 跨多文件联合注册（`pub use xxx::yyy as zzz;`）

任意一条触发 → 把 §13 升级为 v3 并切换 parser。
