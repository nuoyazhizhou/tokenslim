# Audit Health

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
- total_showcase_missing: 0
- total_state_frozen: 128
- capability_index_failed: False

| plugin | status | cases | regressed | missing | frozen_changed | frozen_missing | semantic_gate_failed | showcase_missing | frozen | auditing | exit |
| ------ | ------ | ----: | --------: | ------: | -------------: | -------------: | -------------------: | ---------------: | -----: | -------: | ---: |
| android_gradle_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| ansi_cleaner_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| ansible_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| artifact_summary_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| bazel_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| ci_log_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| cloud_log_plugin | pass | 52 | 0 | 0 | 0 | 0 | 0 | 0 | 52 | 0 | 0 |
| cloudformation_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| db_log_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| dotnet_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| gcc_log_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| generic_text_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| git_diff_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| helm_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| java_stack_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| json_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| kubernetes_docker_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| markdown_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| maven_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| ndjson_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| node_error_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| nodejs_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| noise_filter_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| php_ruby_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| protobuf_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| pulumi_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| pytest_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| python_traceback_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| rust_go_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| shell_session_plugin | fail | 80 | 1 | 0 | 4 | 0 | 0 | 0 | 76 | 4 | 0 |
| smart_code_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| smart_path_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| spring_boot_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| sql_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| static_rule_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| syslog_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| template_driven_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| terraform_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| unity_unreal_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| vcs_az_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| vcs_bitbucket_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| vcs_bzr_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| vcs_cvs_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| vcs_darcs_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| vcs_fossil_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| vcs_gerrit_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| vcs_gh_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| vcs_git_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| vcs_glab_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| vcs_hg_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| vcs_p4_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| vcs_repo_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| vcs_svn_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| web_log_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| webpack_vite_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| xcode_log_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| xml_html_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| yaml_plugin | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |

## Failed Plugins

### android_gradle_plugin

```text
Using plugin: android_gradle_plugin
ReportPath: target\android_gradle_plugin_compact_showcase_report.txt
OutDir: docs/audit\android_gradle_plugin
Track: android_gradle_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\android_gradle_plugin_compact_showcase_report.txt
```

### ansi_cleaner_plugin

```text
Using plugin: ansi_cleaner_plugin
ReportPath: target\ansi_cleaner_plugin_compact_showcase_report.txt
OutDir: docs/audit\ansi_cleaner_plugin
Track: ansi_cleaner_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\ansi_cleaner_plugin_compact_showcase_report.txt
```

### ansible_plugin

```text
Using plugin: ansible_plugin
ReportPath: target\ansible_plugin_compact_showcase_report.txt
OutDir: docs/audit\ansible_plugin
Track: ansible_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\ansible_plugin_compact_showcase_report.txt
```

### artifact_summary_plugin

```text
Using plugin: artifact_summary_plugin
ReportPath: target\artifact_summary_plugin_compact_showcase_report.txt
OutDir: docs/audit\artifact_summary_plugin
Track: artifact_summary_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\artifact_summary_plugin_compact_showcase_report.txt
```

### bazel_plugin

```text
Using plugin: bazel_plugin
ReportPath: target\bazel_plugin_compact_showcase_report.txt
OutDir: docs/audit\bazel_plugin
Track: bazel_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\bazel_plugin_compact_showcase_report.txt
```

### ci_log_plugin

```text
Using plugin: ci_log_plugin
ReportPath: target\ci_log_plugin_compact_showcase_report.txt
OutDir: docs/audit\ci_log_plugin
Track: ci_log_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\ci_log_plugin_compact_showcase_report.txt
```

### cloudformation_plugin

```text
Using plugin: cloudformation_plugin
ReportPath: target\cloudformation_plugin_compact_showcase_report.txt
OutDir: docs/audit\cloudformation_plugin
Track: cloudformation_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\cloudformation_plugin_compact_showcase_report.txt
```

### db_log_plugin

```text
Using plugin: db_log_plugin
ReportPath: target\db_log_plugin_compact_showcase_report.txt
OutDir: docs/audit\db_log_plugin
Track: db_log_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\db_log_plugin_compact_showcase_report.txt
```

### dotnet_plugin

```text
Using plugin: dotnet_plugin
ReportPath: target\dotnet_plugin_compact_showcase_report.txt
OutDir: docs/audit\dotnet_plugin
Track: dotnet_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\dotnet_plugin_compact_showcase_report.txt
```

### gcc_log_plugin

```text
Using plugin: gcc_log_plugin
ReportPath: target\gcc_log_plugin_compact_showcase_report.txt
OutDir: docs/audit\gcc_log_plugin
Track: gcc_log_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\gcc_log_plugin_compact_showcase_report.txt
```

### generic_text_plugin

```text
Using plugin: generic_text_plugin
ReportPath: target\generic_text_plugin_compact_showcase_report.txt
OutDir: docs/audit\generic_text_plugin
Track: generic_text_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\generic_text_plugin_compact_showcase_report.txt
```

### git_diff_plugin

```text
Using plugin: git_diff_plugin
ReportPath: target\git_diff_plugin_compact_showcase_report.txt
OutDir: docs/audit\git_diff_plugin
Track: git_diff_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\git_diff_plugin_compact_showcase_report.txt
```

### helm_plugin

```text
Using plugin: helm_plugin
ReportPath: target\helm_plugin_compact_showcase_report.txt
OutDir: docs/audit\helm_plugin
Track: helm_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\helm_plugin_compact_showcase_report.txt
```

### java_stack_plugin

```text
Using plugin: java_stack_plugin
ReportPath: target\java_stack_plugin_compact_showcase_report.txt
OutDir: docs/audit\java_stack_plugin
Track: java_stack_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\java_stack_plugin_compact_showcase_report.txt
```

### json_plugin

```text
Using plugin: json_plugin
ReportPath: target\json_plugin_compact_showcase_report.txt
OutDir: docs/audit\json_plugin
Track: json_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\json_plugin_compact_showcase_report.txt
```

### kubernetes_docker_plugin

```text
Using plugin: kubernetes_docker_plugin
ReportPath: target\kubernetes_docker_plugin_compact_showcase_report.txt
OutDir: docs/audit\kubernetes_docker_plugin
Track: kubernetes_docker_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\kubernetes_docker_plugin_compact_showcase_report.txt
```

### markdown_plugin

```text
Using plugin: markdown_plugin
ReportPath: target\markdown_plugin_compact_showcase_report.txt
OutDir: docs/audit\markdown_plugin
Track: markdown_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\markdown_plugin_compact_showcase_report.txt
```

### maven_plugin

```text
Using plugin: maven_plugin
ReportPath: target\maven_plugin_compact_showcase_report.txt
OutDir: docs/audit\maven_plugin
Track: maven_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\maven_plugin_compact_showcase_report.txt
```

### ndjson_plugin

```text
Using plugin: ndjson_plugin
ReportPath: target\ndjson_plugin_compact_showcase_report.txt
OutDir: docs/audit\ndjson_plugin
Track: ndjson_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\ndjson_plugin_compact_showcase_report.txt
```

### node_error_plugin

```text
Using plugin: node_error_plugin
ReportPath: target\node_error_plugin_compact_showcase_report.txt
OutDir: docs/audit\node_error_plugin
Track: node_error_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\node_error_plugin_compact_showcase_report.txt
```

### nodejs_plugin

```text
Using plugin: nodejs_plugin
ReportPath: target\nodejs_plugin_compact_showcase_report.txt
OutDir: docs/audit\nodejs_plugin
Track: nodejs_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\nodejs_plugin_compact_showcase_report.txt
```

### noise_filter_plugin

```text
Using plugin: noise_filter_plugin
ReportPath: target\noise_filter_plugin_compact_showcase_report.txt
OutDir: docs/audit\noise_filter_plugin
Track: noise_filter_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\noise_filter_plugin_compact_showcase_report.txt
```

### php_ruby_plugin

```text
Using plugin: php_ruby_plugin
ReportPath: target\php_ruby_plugin_compact_showcase_report.txt
OutDir: docs/audit\php_ruby_plugin
Track: php_ruby_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\php_ruby_plugin_compact_showcase_report.txt
```

### protobuf_plugin

```text
Using plugin: protobuf_plugin
ReportPath: target\protobuf_plugin_compact_showcase_report.txt
OutDir: docs/audit\protobuf_plugin
Track: protobuf_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\protobuf_plugin_compact_showcase_report.txt
```

### pulumi_plugin

```text
Using plugin: pulumi_plugin
ReportPath: target\pulumi_plugin_compact_showcase_report.txt
OutDir: docs/audit\pulumi_plugin
Track: pulumi_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\pulumi_plugin_compact_showcase_report.txt
```

### pytest_plugin

```text
Using plugin: pytest_plugin
ReportPath: target\pytest_plugin_compact_showcase_report.txt
OutDir: docs/audit\pytest_plugin
Track: pytest_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\pytest_plugin_compact_showcase_report.txt
```

### python_traceback_plugin

```text
Using plugin: python_traceback_plugin
ReportPath: target\python_traceback_plugin_compact_showcase_report.txt
OutDir: docs/audit\python_traceback_plugin
Track: python_traceback_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\python_traceback_plugin_compact_showcase_report.txt
```

### rust_go_plugin

```text
Using plugin: rust_go_plugin
ReportPath: target\rust_go_plugin_compact_showcase_report.txt
OutDir: docs/audit\rust_go_plugin
Track: rust_go_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\rust_go_plugin_compact_showcase_report.txt
```

### shell_session_plugin

```text
unchanged=77
new=0
missing=0
freeze_file=docs/audit\shell_session_plugin\frozen_cases.json
frozen_total=80
frozen_changed=4
frozen_missing=0
semantic_gate_failed=0
semantic_gate_passed=80
showcase_missing=0
state_file=docs/audit\shell_session_plugin\audit_state.json
state_todo=0
state_auditing=4
state_frozen=76
state_waived=0
WARN: frozen case content changed; please re-audit these case(s).
frozen_changed_case=case_005_zsh_glob_no_match frozen_hash=57e5bc87e4e914b8781cd7ccb97db4a095bc9c61c2ec4c1799266e82bd8763d9 current_hash=e66e73559484c46ef44d7bb29851347a5ff2920639d3be932b61e0cf6fe504bb
frozen_changed_case=case_038_bash_env_prefix frozen_hash=01248d7dbeb46da98d139ea86acc69bd7ac28aeecf4606b9522610060b50a71d current_hash=83a9075eec9875ed18c2911ebd45ca7d13a515f49857f45f0574f13adc7fef8c
frozen_changed_case=case_040_ps_copy_item frozen_hash=02365ccddc391c28dfc68950eb4d16454cc4099a5e5f86f5661e988fd3ce93cc current_hash=e52bfb93ce4e69f509e045106be39689dfd76c289ab9aaaf54f6bf9377e56d65
frozen_changed_case=case_043_cmd_copy frozen_hash=561f924059ad7f729c8d7c8a2a1f939939db2948dfbae76ce7eb348897e3db85 current_hash=5bbc7e0966ef12e04d9b6e867146fc410ddecc2c92eb7bddde00641222408f44
```

### smart_code_plugin

```text
Using plugin: smart_code_plugin
ReportPath: target\smart_code_plugin_compact_showcase_report.txt
OutDir: docs/audit\smart_code_plugin
Track: smart_code_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\smart_code_plugin_compact_showcase_report.txt
```

### smart_path_plugin

```text
Using plugin: smart_path_plugin
ReportPath: target\smart_path_plugin_compact_showcase_report.txt
OutDir: docs/audit\smart_path_plugin
Track: smart_path_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\smart_path_plugin_compact_showcase_report.txt
```

### spring_boot_plugin

```text
Using plugin: spring_boot_plugin
ReportPath: target\spring_boot_plugin_compact_showcase_report.txt
OutDir: docs/audit\spring_boot_plugin
Track: spring_boot_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\spring_boot_plugin_compact_showcase_report.txt
```

### sql_plugin

```text
Using plugin: sql_plugin
ReportPath: target\sql_plugin_compact_showcase_report.txt
OutDir: docs/audit\sql_plugin
Track: sql_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\sql_plugin_compact_showcase_report.txt
```

### static_rule_plugin

```text
Using plugin: static_rule_plugin
ReportPath: target\static_rule_plugin_compact_showcase_report.txt
OutDir: docs/audit\static_rule_plugin
Track: static_rule_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\static_rule_plugin_compact_showcase_report.txt
```

### syslog_plugin

```text
Using plugin: syslog_plugin
ReportPath: target\syslog_plugin_compact_showcase_report.txt
OutDir: docs/audit\syslog_plugin
Track: syslog_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\syslog_plugin_compact_showcase_report.txt
```

### template_driven_plugin

```text
Using plugin: template_driven_plugin
ReportPath: target\template_driven_plugin_compact_showcase_report.txt
OutDir: docs/audit\template_driven_plugin
Track: template_driven_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\template_driven_plugin_compact_showcase_report.txt
```

### terraform_plugin

```text
Using plugin: terraform_plugin
ReportPath: target\terraform_plugin_compact_showcase_report.txt
OutDir: docs/audit\terraform_plugin
Track: terraform_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\terraform_plugin_compact_showcase_report.txt
```

### unity_unreal_plugin

```text
Using plugin: unity_unreal_plugin
ReportPath: target\unity_unreal_plugin_compact_showcase_report.txt
OutDir: docs/audit\unity_unreal_plugin
Track: unity_unreal_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\unity_unreal_plugin_compact_showcase_report.txt
```

### vcs_az_plugin

```text
Using plugin: vcs_az_plugin
ReportPath: target\vcs_az_plugin_compact_showcase_report.txt
OutDir: docs/audit\vcs_az_plugin
Track: vcs_az_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\vcs_az_plugin_compact_showcase_report.txt
```

### vcs_bitbucket_plugin

```text
Using plugin: vcs_bitbucket_plugin
ReportPath: target\vcs_bitbucket_plugin_compact_showcase_report.txt
OutDir: docs/audit\vcs_bitbucket_plugin
Track: vcs_bitbucket_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\vcs_bitbucket_plugin_compact_showcase_report.txt
```

### vcs_bzr_plugin

```text
Using plugin: vcs_bzr_plugin
ReportPath: target\vcs_bzr_plugin_compact_showcase_report.txt
OutDir: docs/audit\vcs_bzr_plugin
Track: vcs_bzr_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\vcs_bzr_plugin_compact_showcase_report.txt
```

### vcs_cvs_plugin

```text
Using plugin: vcs_cvs_plugin
ReportPath: target\vcs_cvs_plugin_compact_showcase_report.txt
OutDir: docs/audit\vcs_cvs_plugin
Track: vcs_cvs_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\vcs_cvs_plugin_compact_showcase_report.txt
```

### vcs_darcs_plugin

```text
Using plugin: vcs_darcs_plugin
ReportPath: target\vcs_darcs_plugin_compact_showcase_report.txt
OutDir: docs/audit\vcs_darcs_plugin
Track: vcs_darcs_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\vcs_darcs_plugin_compact_showcase_report.txt
```

### vcs_fossil_plugin

```text
Using plugin: vcs_fossil_plugin
ReportPath: target\vcs_fossil_plugin_compact_showcase_report.txt
OutDir: docs/audit\vcs_fossil_plugin
Track: vcs_fossil_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\vcs_fossil_plugin_compact_showcase_report.txt
```

### vcs_gerrit_plugin

```text
Using plugin: vcs_gerrit_plugin
ReportPath: target\vcs_gerrit_plugin_compact_showcase_report.txt
OutDir: docs/audit\vcs_gerrit_plugin
Track: vcs_gerrit_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\vcs_gerrit_plugin_compact_showcase_report.txt
```

### vcs_gh_plugin

```text
Using plugin: vcs_gh_plugin
ReportPath: target\vcs_gh_plugin_compact_showcase_report.txt
OutDir: docs/audit\vcs_gh_plugin
Track: vcs_gh_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\vcs_gh_plugin_compact_showcase_report.txt
```

### vcs_git_plugin

```text
Using plugin: vcs_git_plugin
ReportPath: target\vcs_git_plugin_compact_showcase_report.txt
OutDir: docs/audit\vcs_git_plugin
Track: vcs_git_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\vcs_git_plugin_compact_showcase_report.txt
```

### vcs_glab_plugin

```text
Using plugin: vcs_glab_plugin
ReportPath: target\vcs_glab_plugin_compact_showcase_report.txt
OutDir: docs/audit\vcs_glab_plugin
Track: vcs_glab_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\vcs_glab_plugin_compact_showcase_report.txt
```

### vcs_hg_plugin

```text
Using plugin: vcs_hg_plugin
ReportPath: target\vcs_hg_plugin_compact_showcase_report.txt
OutDir: docs/audit\vcs_hg_plugin
Track: vcs_hg_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\vcs_hg_plugin_compact_showcase_report.txt
```

### vcs_p4_plugin

```text
Using plugin: vcs_p4_plugin
ReportPath: target\vcs_p4_plugin_compact_showcase_report.txt
OutDir: docs/audit\vcs_p4_plugin
Track: vcs_p4_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\vcs_p4_plugin_compact_showcase_report.txt
```

### vcs_repo_plugin

```text
Using plugin: vcs_repo_plugin
ReportPath: target\vcs_repo_plugin_compact_showcase_report.txt
OutDir: docs/audit\vcs_repo_plugin
Track: vcs_repo_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\vcs_repo_plugin_compact_showcase_report.txt
```

### vcs_svn_plugin

```text
Using plugin: vcs_svn_plugin
ReportPath: target\vcs_svn_plugin_compact_showcase_report.txt
OutDir: docs/audit\vcs_svn_plugin
Track: vcs_svn_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\vcs_svn_plugin_compact_showcase_report.txt
```

### web_log_plugin

```text
Using plugin: web_log_plugin
ReportPath: target\web_log_plugin_compact_showcase_report.txt
OutDir: docs/audit\web_log_plugin
Track: web_log_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\web_log_plugin_compact_showcase_report.txt
```

### webpack_vite_plugin

```text
Using plugin: webpack_vite_plugin
ReportPath: target\webpack_vite_plugin_compact_showcase_report.txt
OutDir: docs/audit\webpack_vite_plugin
Track: webpack_vite_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\webpack_vite_plugin_compact_showcase_report.txt
```

### xcode_log_plugin

```text
Using plugin: xcode_log_plugin
ReportPath: target\xcode_log_plugin_compact_showcase_report.txt
OutDir: docs/audit\xcode_log_plugin
Track: xcode_log_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\xcode_log_plugin_compact_showcase_report.txt
```

### xml_html_plugin

```text
Using plugin: xml_html_plugin
ReportPath: target\xml_html_plugin_compact_showcase_report.txt
OutDir: docs/audit\xml_html_plugin
Track: xml_html_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\xml_html_plugin_compact_showcase_report.txt
```

### yaml_plugin

```text
Using plugin: yaml_plugin
ReportPath: target\yaml_plugin_compact_showcase_report.txt
OutDir: docs/audit\yaml_plugin
Track: yaml_plugin
Traceback (most recent call last):
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 1293, in <module>
    main()
    ~~~~^^
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 843, in main
    rows = parse_report(report_path)
  File "C:\git_work\TokenSlim\scripts\audit_case_metrics.py", line 459, in parse_report
    raise FileNotFoundError(f"Report not found: {path}")
FileNotFoundError: Report not found: target\yaml_plugin_compact_showcase_report.txt
```
