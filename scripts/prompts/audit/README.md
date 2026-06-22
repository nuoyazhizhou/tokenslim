# scripts/prompts/audit/ — LLM 审计提示词目录（fix #29）

## 这是什么

三个 LLM 审计脚本用的提示词模板，**全部以 .md 文件存在磁盘上**，由
`audit_llm_common.load_prompt_template()` 加载。**磁盘文件优先级高于嵌入 fallback**。

这样做的目的：
- 改提示词不需要改 Python 代码（不需要发版）
- 提示词作者和代码作者可以并行工作
- 提示词走 git diff / git blame / code review，普通文档工具就能处理
- 不再让"哪个脚本用了哪个 prompt"靠 grep 内联常量

## 目录约定

```
scripts/prompts/audit/
  case_quality/            # 给 audit_sample_case_quality.py 用
    _base.md               # 必有：通用部分（STRUCTURE_RULES + KB 注入段）
    _footer.md             # 必有：JSON schema 收尾
    shell.md               # 必有：shell 类型的 REALISM_RULES
    access_log.md          # 可选：access_log 类型的 REALISM_RULES
    data_struct.md         # 可选：data_struct 类型的 REALISM_RULES
    vcs.md                 # 可选：vcs 类型的 REALISM_RULES
    build.md               # 可选：build 类型的 REALISM_RULES
    error_trace.md         # 可选：error_trace 类型的 REALISM_RULES
    default.md             # 必有：未知 type 兜底（不能删）

  case_metrics/            # 给 audit_case_metrics.py 用
    _base.md               # 必有：9 条 audit constitution + JSON schema
    _footer.md             # 必有：DECISION DISCIPLINE
    {type}.md              # 可选：type-specific 规则
```

## 模板变量

`_base.md` 和 type-specific 文件里都可以放 `{{ key }}` 占位符，由
`build_case_quality_prompt()` / `build_case_metrics_prompt()` 在运行时替换。

可用的占位符（按 audit_kind 分）：

### case_quality

| placeholder | 来源 |
|-------------|------|
| `{{ project.display_name }}` | tokenslim_kb/project.yaml |
| `{{ project.mission }}` | tokenslim_kb/project.yaml |
| `{{ project.tagline }}` | tokenslim_kb/project.yaml |
| `{{ plugin.name }}` | 调用方传 |
| `{{ plugin.type }}` | 调用方传 |
| `{{ plugin.narrative }}` | docs/audit/plugin_capability_index.json |
| `{{ scenario.scenario }}` | samples/<plugin>/case_NNN_xxx.scenario.yaml |
| `{{ scenario.target_capability }}` | sidecar |
| `{{ scenario.expected_keep }}` | sidecar |
| `{{ scenario.expected_compress }}` | sidecar |

### case_metrics

| placeholder | 来源 |
|-------------|------|
| `{{ project.display_name }}` | tokenslim_kb/project.yaml |
| `{{ project.mission }}` | tokenslim_kb/project.yaml |
| `{{ project.tagline }}` | tokenslim_kb/project.yaml |
| `{{ plugin.narrative }}` | docs/audit/plugin_capability_index.json |
| `{{ plugin.type }}` | 调用方传 |
| `{{ tactical_rules }}` | docs/prompts/semantic_audit_profiles.md（自动按 plugin group 选） |

## 文件开头的 `#` 注释

`_base.md` / `<type>.md` 可以以 `#` 开头的注释行开头（解释"这块放什么"、
"这块不许改"），`load_prompt_template` 会自动剥离 — 不会让 LLM 看到。

**约定**：

- 中段的 `#` 行视作内容（不会被剥）
- 注释从文件开头连续到第一个非 `#` 起始行止
- 鼓励用注释头说明"宪法级别"约束（如 9 条 audit constitution）

## 修改提示词

普通修改（如改 R1 一条细则）：直接 PR。
宪法级别修改（9 条 audit constitution / R1-R7 shell 真实性规则）：
- 改对应的 .md 文件
- 同步改 audit_case_metrics.py / audit_sample_case_quality.py 的 gate test
- 改对应的 smoke 测试（`scripts/_smoke_types.py`）
- 在 PR 描述里写明"宪法级别修改"

## 工具

- `scripts/generate_case_sidecars.py` — 给 samples/ 下 case 批量创建空 sidecar 模板
- `scripts/audit_llm_common.py::build_*_prompt` — 拼装完整 system prompt

## 验证

跑 `python scripts/audit_sample_case_quality.py --plugin <plugin>` 时，
会在 `out_dir` 写出 prompt snapshot（如 `llm_prompt_<case>.txt`），可用来对比
改动前后的实际发送给 LLM 的内容。
