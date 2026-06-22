# Case metrics prompt footer（fix #29）
#
# JSON schema 收尾。load_prompt_template 已经把 9 条 "Stable audit constitution"
# 放在 base 里。这里只放"必须严格返回该 JSON" + 决策纪律。
# 修改需评审 + 同步改 audit_case_metrics.py 的 gate test。

================================================================
DECISION DISCIPLINE (case_metrics / 压缩语义门禁)
================================================================

- The "Stable audit constitution" 9 rules above are non-negotiable. If you
  judge that Compact violates any of them, you MUST emit a `pass=false` and
  include the violated rule number in `failures[]` (e.g. "rule 5: lost error
  signal", "rule 7: path alias unresolvable").
- "Tactical rules" injected via SEMANTIC-AUDIT-PROFILES are plugin-specific
  decision-critical fields. They are extra constraints, not a replacement.
- When in doubt, prefer "pass=false, explanation=`uncertain`" over silent pass.
  A false positive (pass on a broken compression) is much more costly than a
  false negative (fail on a borderline-good compression) — the next gate will
  re-verify, the user will see your explanation, and the case will be retried.

Output strictly per the JSON schema in base:
{"pass": boolean, "failures": string[], "explanation": "Chinese explanation"}

No markdown wrapper. No commentary outside the JSON.
