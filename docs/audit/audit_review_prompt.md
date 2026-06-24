# LLM Audit Review Prompt

You are reviewing TokenSlim case audit results. First read CONTRIBUTING.md and the project's compression protocol (docs/development/PLUGIN_DEVELOPMENT.md), then review the generated audit artifacts listed below. Older *_tactical_prompt.md files are historical human handoff notes, not the active LLM semantic-gate contract.

## Audit Run

- version: all_20260624-175500
- generated_at: 2026-06-24T17:55:11.419825
- plugins: 58
- failed: 57
- total_cases: 132
- total_regressed: 1
- total_missing: 0
- total_frozen_changed: 4
- total_frozen_missing: 0
- total_semantic_gate_failed: 0
- total_state_frozen: 128

## Required Review

1. Check docs/audit/audit_health.md and docs/audit/audit_index.json for failed plugins, regressions, missing cases, frozen drift, and semantic gate failures.
2. For every failed or auditing case, compare docs/audit/<plugin>/cases/<case_id>/original.txt with compact.txt and summary.json.
3. Decide each reviewed case immediately: pass and freeze, needs optimization, or waived with reason.
4. If a case needs optimization, update the relevant task board before ending the turn.
5. Do not mark the task complete while any active P0/P1/P2 item remains stale in docs/tasks, docs/plans, or docs/reports.
6. Review docs/audit\route_replay_cases.md and docs/audit\route_replay_cases.json for route/detector explainability, fallback decisions, retry_plugin suggestions, recommendation fields, and replay templates for suspicious cases.

## Failed Plugins

- android_gradle_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- ansi_cleaner_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- ansible_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- artifact_summary_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- bazel_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- ci_log_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- cloudformation_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- db_log_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- dotnet_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- gcc_log_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- generic_text_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- git_diff_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- helm_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- java_stack_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- json_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- kubernetes_docker_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- markdown_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- maven_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- ndjson_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- node_error_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- nodejs_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- noise_filter_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- php_ruby_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- protobuf_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- pulumi_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- pytest_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- python_traceback_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- rust_go_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- shell_session_plugin: regressed=1, missing=0, frozen_changed=4, frozen_missing=0, semantic_gate_failed=0, auditing=4
- smart_code_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- smart_path_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- spring_boot_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- sql_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- static_rule_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- syslog_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- template_driven_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- terraform_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- unity_unreal_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- vcs_az_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- vcs_bitbucket_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- vcs_bzr_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- vcs_cvs_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- vcs_darcs_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- vcs_fossil_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- vcs_gerrit_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- vcs_gh_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- vcs_git_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- vcs_glab_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- vcs_hg_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- vcs_p4_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- vcs_repo_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- vcs_svn_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- web_log_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- webpack_vite_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- xcode_log_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- xml_html_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0
- yaml_plugin: regressed=0, missing=0, frozen_changed=0, frozen_missing=0, semantic_gate_failed=0, auditing=0

## Plugins With Auditing State

- shell_session_plugin: auditing=4, frozen=76, cases=80

## Useful Commands

~~~powershell
tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --version all_20260624-175500 --case-id <case_id> --require-semantic-gate
tokenslim run python scripts/audit_case_metrics.py --plugin <plugin> --version all_20260624-175500 --freeze-case <case_id> --require-semantic-gate
tokenslim run python scripts/audit_all_case_metrics.py --version all_20260624-175500 --require-semantic-gate --fail-on-regression --fail-on-frozen-change --fail-on-any-failure
tokenslim explain-plugin --format json --input docs/audit/<plugin>/cases/<case_id>/original.txt --explain-replay-out docs/audit/<plugin>/cases/<case_id>/route_replay.md
~~~
