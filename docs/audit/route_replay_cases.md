# Route Misclassification Replay Cases

- generated_at: 2026-06-24T17:55:11.422689
- source_audit_version: all_20260624-175500
- tokenslim_explain_binary: target\debug\tokenslim.exe

Use this file when a compact/original mirror looks suspicious: replay the input through `tokenslim explain-plugin`, inspect `fallback_decision`, `retry_plugin`, `recommendation_*`, and capability evidence, then decide whether the issue is a real route/detector mismatch or an expected generic fallback.

## Active Audit Replay Templates

### android_gradle_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/android_gradle_plugin/android_gradle_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### ansi_cleaner_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/ansi_cleaner_plugin/ansi_cleaner_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### ansible_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/ansible_plugin/ansible_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### artifact_summary_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/artifact_summary_plugin/artifact_summary_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### bazel_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/bazel_plugin/bazel_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### ci_log_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/ci_log_plugin/ci_log_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### cloudformation_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/cloudformation_plugin/cloudformation_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### db_log_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/db_log_plugin/db_log_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### dotnet_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/dotnet_plugin/dotnet_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### gcc_log_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/gcc_log_plugin/gcc_log_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### generic_text_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/generic_text_plugin/generic_text_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### git_diff_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/git_diff_plugin/git_diff_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### helm_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/helm_plugin/helm_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### java_stack_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/java_stack_plugin/java_stack_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### json_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/json_plugin/json_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### kubernetes_docker_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/kubernetes_docker_plugin/kubernetes_docker_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### markdown_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/markdown_plugin/markdown_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### maven_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/maven_plugin/maven_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### ndjson_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/ndjson_plugin/ndjson_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

### node_error_plugin

- reason: plugin_failed_or_needs_manual_case_lookup
- action: inspect `docs/audit/node_error_plugin/node_error_plugin.latest.json` and export a focused case with `audit_case_metrics.py` before replay.

## Smoke Explainability Baseline

### command: az pipelines runs show

- explain_json_parsed: True
- selected_plugin: ci_log
- fallback_decision: stable_route
- retry_plugin: none
- recommendation_primary: ci_log
- recommendation_confidence: high
- recommendation_action: accept
- recommendation_reason: route_match:arg_prefix/pattern:az pipelines/intent:none/priority:160

#### machine_readable_summary

```json
{"selected_plugin": "ci_log", "fallback_decision": "stable_route", "retry_plugin": "none", "recommendation": {"primary": "ci_log", "confidence": "high", "action": "accept", "reason": "route_match:arg_prefix/pattern:az pipelines/intent:none/priority:160"}}
```

#### explain_output_json

```json
{
    "alternatives": [
        {
            "capability": {
                "audit": "0",
                "description": "",
                "frozen": "0",
                "route": "vcs",
                "samples": "0",
                "showcase": "0",
                "status": "orchestrator",
                "tags": ""
            },
            "fallback": "false",
            "group": "vcs",
            "intent": "other",
            "key": "alternative_1",
            "matched_by": "keyword",
            "pattern": "az",
            "plugin": "vcs",
            "primary": "vcs",
            "priority": "100",
            "rank": 1,
            "raw": "vcs|group=vcs|matched_by=keyword|pattern=az|intent=other|priority=100|fallback=false"
        }
    ],
    "contract_ok": true,
    "contract_version": "explain.v1",
    "fallback_decision": "stable_route",
    "fields": {
        "alternative_1": "vcs|group=vcs|matched_by=keyword|pattern=az|intent=other|priority=100|fallback=false",
        "alternative_1_capability": "description:|tags:|route:vcs|samples:0|showcase:0|audit:0|frozen:0|status:orchestrator",
        "alternatives": "1",
        "candidate_plugin_chain": "android_gradle, ansible, ansi_cleaner, artifact_summary, bazel, cloud_log, cloudformation, ci_log, db_log, dotnet, gcc_log, helm, java_stack, json, kubernetes_docker, ndjson, markdown, maven, node_error, nodejs, noise_filter, generic_text, php_ruby, protobuf, pulumi, pytest, python_traceback, rust_go, shell_session, smart_code, smart_path, spring_boot, sql, static_rule, syslog, terraform, unity_unreal, web_log, xcode_log, webpack_vite, xml_html, template_driven, yaml",
        "command": "az pipelines runs show",
        "confidence_gap": "60",
        "confidence_gap_source": "route_priority",
        "fallback_decision": "stable_route",
        "fallback_threshold": "0.150",
        "input_kind": "command",
        "output_format": "json",
        "recommendation_action": "accept",
        "recommendation_alternative_1": "vcs",
        "recommendation_alternative_2": "none",
        "recommendation_confidence": "high",
        "recommendation_primary": "ci_log",
        "recommendation_reason": "route_match:arg_prefix/pattern:az pipelines/intent:none/priority:160",
        "replay_case_template": "available_with:--explain-replay-out <path>",
        "retry_plugin": "none",
        "route_group": "build",
        "run_route_view": "available_with:tokenslim run --explain-route az pipelines runs show",
        "selected_capability": "description:CI/CD shell wrapper semantic compaction for GitHub Actions, GitLab CI, Jenkins, Azure Pipelines, CircleCI, Buildkite, local act, TeamCity, Travis CI, and organization-specific banner logs|tags:|route:build|samples:88|showcase:44|audit:0|frozen:0|status:missing_audit",
        "selected_declared_patterns": "::(group/endgroup/error/warning):: ; (section_start:/section_end:/Running with gitlab-runner) ; (\\[Pipeline\\]/##\\[(section/error/warning/command)\\]/buildkite-agent/circleci) ; (##teamcity\\[/travis_(fold/time):/\\[(ACME-CI/ci)\\]/### Step:)",
        "selected_plugin": "ci_log",
        "top_score_gap": "60",
        "why": "command_tool:az matched_by:arg_prefix pattern:az pipelines intent:none priority:160 fallback:false"
    },
    "input_kind": "command",
    "kind": "plugin_selection",
    "missing_required_fields": [],
    "recommendation": {
        "action": "accept",
        "alternative_1": "vcs",
        "alternative_2": "none",
        "confidence": "high",
        "confidence_gap": "60",
        "confidence_gap_source": "route_priority",
        "primary": "ci_log",
        "reason": "route_match:arg_prefix/pattern:az pipelines/intent:none/priority:160"
    },
    "required_fields": [
        "input_kind",
        "selected_plugin",
        "fallback_decision",
        "retry_plugin",
        "recommendation_primary",
        "recommendation_confidence",
        "recommendation_action",
        "recommendation_reason",
        "confidence_gap",
        "confidence_gap_source",
        "alternatives"
    ],
    "retry_plugin": "none",
    "selected": {
        "capability": {
            "audit": "0",
            "description": "CI/CD shell wrapper semantic compaction for GitHub Actions, GitLab CI, Jenkins, Azure Pipelines, CircleCI, Buildkite, local act, TeamCity, Travis CI, and organization-specific banner logs",
            "frozen": "0",
            "route": "build",
            "samples": "88",
            "showcase": "44",
            "status": "missing_audit",
            "tags": ""
        },
        "declared_patterns": "::(group/endgroup/error/warning):: ; (section_start:/section_end:/Running with gitlab-runner) ; (\\[Pipeline\\]/##\\[(section/error/warning/command)\\]/buildkite-agent/circleci) ; (##teamcity\\[/travis_(fold/time):/\\[(ACME-CI/ci)\\]/### Step:)",
        "plugin": "ci_log",
        "why": {
            "command_tool": "az matched_by:arg_prefix pattern:az pipelines intent:none priority:160 fallback:false"
        }
    },
    "selected_plugin": "ci_log"
}
```

### log sample: web_log case_016

- explain_json_parsed: True
- selected_plugin: web_log
- fallback_decision: stable_detector
- retry_plugin: none
- recommendation_primary: web_log
- recommendation_confidence: medium
- recommendation_action: accept
- recommendation_reason: detector_stable/selected_score:1.000/top_score_gap:0.100/threshold:0.150

#### machine_readable_summary

```json
{"selected_plugin": "web_log", "fallback_decision": "stable_detector", "retry_plugin": "none", "recommendation": {"primary": "web_log", "confidence": "medium", "action": "accept", "reason": "detector_stable/selected_score:1.000/top_score_gap:0.100/threshold:0.150"}}
```

#### explain_output_json

```json
{
    "alternatives": [
        {
            "capability": {
                "audit": "0",
                "description": "\u667a\u80fd\u8def\u5f84\u8131\u6c34",
                "frozen": "0",
                "route": "none",
                "samples": "24",
                "showcase": "12",
                "status": "missing_audit",
                "tags": ""
            },
            "key": "alternative_1",
            "plugin": "smart_path",
            "primary": "smart_path",
            "priority": "250",
            "rank": 1,
            "raw": "smart_path|score=0.900|priority=250",
            "score": "0.900"
        },
        {
            "capability": {
                "audit": "0",
                "description": "\u901a\u7528\u6587\u672c\u8131\u6c34",
                "frozen": "0",
                "route": "generic",
                "samples": "24",
                "showcase": "12",
                "status": "missing_audit",
                "tags": ""
            },
            "key": "alternative_2",
            "plugin": "generic_text",
            "primary": "generic_text",
            "priority": "160",
            "rank": 2,
            "raw": "generic_text|score=0.110|priority=160",
            "score": "0.110"
        }
    ],
    "contract_ok": true,
    "contract_version": "explain.v1",
    "fallback_decision": "stable_detector",
    "fields": {
        "alternative_1": "smart_path|score=0.900|priority=250",
        "alternative_1_capability": "description:\u667a\u80fd\u8def\u5f84\u8131\u6c34|tags:|route:none|samples:24|showcase:12|audit:0|frozen:0|status:missing_audit",
        "alternative_2": "generic_text|score=0.110|priority=160",
        "alternative_2_capability": "description:\u901a\u7528\u6587\u672c\u8131\u6c34|tags:|route:generic|samples:24|showcase:12|audit:0|frozen:0|status:missing_audit",
        "alternatives": "2",
        "byte_count": "599",
        "confidence_gap": "0.100",
        "confidence_gap_source": "detector_score",
        "fallback_decision": "stable_detector",
        "fallback_note": "nearest_candidate_non_retryable:smart_path",
        "fallback_threshold": "0.150",
        "input_kind": "log",
        "line_count": "6",
        "recommendation_action": "accept",
        "recommendation_alternative_1": "smart_path",
        "recommendation_alternative_2": "generic_text",
        "recommendation_confidence": "medium",
        "recommendation_primary": "web_log",
        "recommendation_reason": "detector_stable/selected_score:1.000/top_score_gap:0.100/threshold:0.150",
        "replay_case_template": "available_with:--explain-replay-out <path>",
        "retry_plugin": "none",
        "retry_score_gap": "1.000",
        "selected_capability": "description:Web access log v3 semantic aggregation for Nginx, Apache, ingress, Uvicorn, Envoy/Istio, CloudFront/IIS W3C, Cloudflare, native ALB, and cloud-wrapped CSV/JSON/table/plain access logs with dictionaries, routine folding, noise diagnostics, scan/burst spotlight, anomalies, and slow request signals|tags:|route:none|samples:96|showcase:48|audit:0|frozen:0|status:missing_audit",
        "selected_declared_patterns": "^\\d+\\.\\d+\\.\\d+\\.\\d+ ; INFO:\\s+\\d+\\.\\d+\\.\\d+\\.\\d+:\\d+\\s+-\\s+\\\"(GET/POST/PUT/PATCH/DELETE) ; \\\"(request_method/ClientRequestMethod/httpRequest/message)\\\" ; ^#Fields:\\s+.*cs-method.*sc-status ; ^\\[\\d{4}-\\d{2}-\\d{2}T[^\\]]+\\]\\s+\\\"(GET/POST/PUT/PATCH/DELETE)",
        "selected_plugin": "web_log",
        "top_score_gap": "0.100",
        "why": "content_detector_score:1.000|plugin_priority:170|candidate_rank:1"
    },
    "input_kind": "log",
    "kind": "plugin_selection",
    "missing_required_fields": [],
    "recommendation": {
        "action": "accept",
        "alternative_1": "smart_path",
        "alternative_2": "generic_text",
        "confidence": "medium",
        "confidence_gap": "0.100",
        "confidence_gap_source": "detector_score",
        "primary": "web_log",
        "reason": "detector_stable/selected_score:1.000/top_score_gap:0.100/threshold:0.150"
    },
    "required_fields": [
        "input_kind",
        "selected_plugin",
        "fallback_decision",
        "retry_plugin",
        "recommendation_primary",
        "recommendation_confidence",
        "recommendation_action",
        "recommendation_reason",
        "confidence_gap",
        "confidence_gap_source",
        "alternatives"
    ],
    "retry_plugin": "none",
    "selected": {
        "capability": {
            "audit": "0",
            "description": "Web access log v3 semantic aggregation for Nginx, Apache, ingress, Uvicorn, Envoy/Istio, CloudFront/IIS W3C, Cloudflare, native ALB, and cloud-wrapped CSV/JSON/table/plain access logs with dictionaries, routine folding, noise diagnostics, scan/burst spotlight, anomalies, and slow request signals",
            "frozen": "0",
            "route": "none",
            "samples": "96",
            "showcase": "48",
            "status": "missing_audit",
            "tags": ""
        },
        "declared_patterns": "^\\d+\\.\\d+\\.\\d+\\.\\d+ ; INFO:\\s+\\d+\\.\\d+\\.\\d+\\.\\d+:\\d+\\s+-\\s+\\\"(GET/POST/PUT/PATCH/DELETE) ; \\\"(request_method/ClientRequestMethod/httpRequest/message)\\\" ; ^#Fields:\\s+.*cs-method.*sc-status ; ^\\[\\d{4}-\\d{2}-\\d{2}T[^\\]]+\\]\\s+\\\"(GET/POST/PUT/PATCH/DELETE)",
        "plugin": "web_log",
        "why": {
            "candidate_rank": "1",
            "content_detector_score": "1.000",
            "plugin_priority": "170"
        }
    },
    "selected_plugin": "web_log"
}
```
