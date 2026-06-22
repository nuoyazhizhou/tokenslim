# Prompt Governance

## Active LLM Semantic Gate Prompt

- `semantic_audit_profiles.md` is the active source read by `scripts/audit_case_metrics.py`.
- It contains short, stable semantic-preservation profiles by plugin family.
- It must not contain per-case logs, dynamic case IDs, handoff slogans, implementation recipes, or historical needs-fix task lists.

## Historical / Human Handoff Prompts

- `*_tactical_prompt.md`
- `shell_command_tactical_prompt.md`
- `vcs_classical_prompts.md`
- `vcs_cloud_cli_prompt.md`
- `vcs_needs_fix_tactical_prompt.md`
- `non_vcs_classical_prompts.md`
- `vcs_prompts_delta.md`

These files are retained as historical context and human repair playbooks. They are not the active LLM semantic-gate contract because they include plugin-specific handoff wording, old command examples, and historical fix prescriptions that can bias semantic judging.

## Prompt Cache Rule

The LLM system prompt should keep this stable order:

1. `CLAUDE.md` / Compression Protocol V1 audit constitution summary.
2. `semantic_audit_profiles.md` selected static profile blocks.
3. Dynamic case metadata and Original/Compact text in the user message only.
