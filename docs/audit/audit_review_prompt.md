# LLM Audit Review Prompt

You are reviewing TokenSlim case audit results. First read CONTRIBUTING.md and the project's compression protocol (docs/development/PLUGIN_DEVELOPMENT.md), then review the generated audit artifacts listed below. Older *_tactical_prompt.md files are historical human handoff notes, not the active LLM semantic-gate contract.

## Audit Run

- version: v20260911_r2
- generated_at: 2026-09-11T14:36:43.233254
- plugins: 60
- failed: 0
- total_cases: 1156
- total_regressed: 0
- total_missing: 0
- total_frozen_changed: 0
- total_frozen_missing: 0
- total_semantic_gate_failed: 0
- total_state_frozen: 1156

## Required Review

1. Check docs/audit/audit_health.md and docs/audit/audit_index.json for failed plugins, regressions, missing cases, frozen drift, and semantic gate failures.
2. For every failed or auditing case, compare docs/audit/<plugin>/cases/<case_id>/original.txt with compact.txt and summary.json.
3. Decide each reviewed case immediately: pass and freeze, needs optimization, or waived with reason.
4. If a case needs optimization, update the relevant task board before ending the turn.
5. Do not mark the task complete while any active P0/P1/P2 item remains stale in docs/tasks, docs/plans, or docs/reports.
6. Review docs/audit\route_replay_cases.md and docs/audit\route_replay_cases.json for route/detector explainability, fallback decisions, retry_plugin suggestions, recommendation fields, and replay templates for suspicious cases.

## Frozen Drift Hard Block

Frozen case drift (frozen_changed / frozen_missing) is a hard blocking condition, not a mild warning. If total_frozen_changed > 0 or total_frozen_missing > 0, you MUST NOT mark the audit pass and MUST resolve before continuing:

- None: no frozen case drift detected.

## Failed Plugins

- None

## Plugins With Auditing State

- None

## Useful Commands

~~~powershell
tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --version v20260911_r2 --case-id <case_id> --require-semantic-gate
tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --version v20260911_r2 --freeze-case <case_id> --require-semantic-gate
tokenslim run python scripts/audit_all_case_metrics.py --version v20260911_r2 --require-semantic-gate --fail-on-regression --fail-on-frozen-change --fail-on-any-failure
tokenslim explain-plugin --format json --input docs/audit/<plugin>/cases/<case_id>/original.txt --explain-replay-out docs/audit/<plugin>/cases/<case_id>/route_replay.md
~~~
