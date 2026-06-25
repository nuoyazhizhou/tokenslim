# Plugin Capability Matrix

- generated_at: 2026-06-24T23:32:13.161732
- plugins_total: 63
- audited_plugins: 0
- frozen_plugins: 0
- coverage_gaps: 59
- authoritative sources: config/plugins/*.json, samples/*, src/plugins/*/showcase.rs, docs/audit/*

| plugin | status | tags | route | samples | showcase | audit | frozen | auditing | warnings | description |
| ------ | ------ | ---- | ----- | ------: | -------: | ----: | -----: | -------: | -------- | ----------- |
| android_gradle | missing_audit | build_log | - | 44 | 22 | 0 | 0 | 0 | - | Android Gradle 构建日志脱水 |
| ansi_cleaner | missing_audit | general | - | 24 | 12 | 0 | 0 | 0 | - | ANSI escape sequence cleaner |
| ansible | missing_audit | infra | - | 24 | 12 | 0 | 0 | 0 | - | Ansible play/task output compaction |
| artifact_summary | missing_audit | build_log,data_format,test_log | - | 24 | 12 | 0 | 0 | 0 | - | Build artifact summary compaction for SARIF security/code scanning results and JUnit XML test reports |
| bazel | missing_audit | build_log,test_log | - | 24 | 12 | 0 | 0 | 0 | - | Bazel build/test output compaction |
| ci_log | missing_audit | ci_cd | build | 88 | 44 | 0 | 0 | 0 | - | CI/CD shell wrapper semantic compaction for GitHub Actions, GitLab CI, Jenkins, Azure Pipelines, CircleCI, Buildkite, local act, TeamCity, Travis CI, and organization-specific banner logs |
| cloud_log | missing_audit | cloud_log | - | 104 | 52 | 0 | 0 | 0 | - | 主流云厂商日志外壳剥离（AWS/GCP/Azure/阿里云/OCI/腾讯云/华为云/Cloudflare），将 message/textPayload/content 等内层日志还原给专用日志插件 |
| cloudformation | missing_audit | cloud_log,infra | - | 24 | 12 | 0 | 0 | 0 | - | AWS CloudFormation event compaction |
| db_log | missing_audit | database,stack_trace | - | 44 | 22 | 0 | 0 | 0 | - | Database log compaction for PostgreSQL, MySQL, MongoDB, and Redis slow/error/replication/runtime events |
| dotnet | missing_audit | general | - | 24 | 12 | 0 | 0 | 0 | - | .NET 构建日志脱水 |
| encoding_fallback | missing_audit | general | - | 20 | 0 | 0 | 0 | 0 | - | 编码兜底 |
| explain | meta | general | - | 2 | 0 | 0 | 0 | 0 | - | 解释插件 |
| gcc_log | missing_audit | build_log | - | 48 | 24 | 0 | 0 | 0 | - | GCC/Clang 编译日志脱水 |
| generic_text | missing_audit | general | generic | 24 | 12 | 0 | 0 | 0 | - | 通用文本脱水 |
| git_diff | missing_audit | vcs | - | 24 | 12 | 0 | 0 | 0 | - | Git diff 输出脱水 |
| helm | missing_audit | infra | - | 24 | 12 | 0 | 0 | 0 | - | Helm install/upgrade output compaction |
| java_stack | missing_audit | stack_trace | - | 32 | 16 | 0 | 0 | 0 | - | Java 堆栈跟踪脱水 |
| json | missing_audit | data_format | - | 24 | 12 | 0 | 0 | 0 | - | JSON structure compaction |
| kubernetes_docker | missing_audit | infra | - | 48 | 24 | 0 | 0 | 0 | - | Kubernetes/Docker 输出脱水 |
| markdown | missing_audit | general | - | 24 | 12 | 0 | 0 | 0 | - | Markdown 脱水 |
| maven | missing_audit | build_log | - | 32 | 16 | 0 | 0 | 0 | - | Maven 构建日志脱水 |
| minify_code | disabled_config | general | - | 0 | 0 | 0 | 0 | 0 | - | Deprecated config-only entry; smart_code_plugin owns code compaction |
| ndjson | missing_audit | data_format | - | 24 | 12 | 0 | 0 | 0 | - | NDJSON 脱水 |
| node_error | missing_audit | stack_trace | - | 24 | 12 | 0 | 0 | 0 | - | Node.js 错误堆栈脱水 |
| nodejs | missing_audit | stack_trace | - | 46 | 23 | 0 | 0 | 0 | - | Node.js 日志脱水 |
| noise_filter | missing_audit | general | - | 24 | 12 | 0 | 0 | 0 | - | 噪声过滤 |
| npm | unknown | general | - | 0 | 0 | 0 | 0 | 0 | - |  |
| php_ruby | missing_audit | general | - | 24 | 12 | 0 | 0 | 0 | - | PHP/Ruby 错误栈脱水 |
| protobuf | missing_audit | general | - | 24 | 12 | 0 | 0 | 0 | - | protoc/buf diagnostic compaction |
| pulumi | missing_audit | infra | - | 24 | 12 | 0 | 0 | 0 | - | Pulumi preview/up output compaction |
| pytest | missing_audit | test_log | - | 36 | 18 | 0 | 0 | 0 | - | pytest session and result compaction |
| python_traceback | missing_audit | stack_trace | - | 32 | 16 | 0 | 0 | 0 | - | Python 堆栈跟踪脱水 |
| rust_go | missing_audit | stack_trace | - | 36 | 18 | 0 | 0 | 0 | - | Rust/Go 日志脱水 |
| shell_session | missing_audit | general | - | 160 | 80 | 0 | 0 | 0 | - | Shell 会话脱水 |
| smart_code | missing_audit | general | - | 24 | 12 | 0 | 0 | 0 | - | Smart code detection and compaction |
| smart_path | missing_audit | general | - | 24 | 12 | 0 | 0 | 0 | - | 智能路径脱水 |
| spring_boot | missing_audit | general | - | 30 | 15 | 0 | 0 | 0 | - | Spring Boot 应用日志脱水 |
| sql | missing_audit | database | - | 24 | 12 | 0 | 0 | 0 | - | SQL 输出脱水 |
| static_rule | missing_audit | general | - | 24 | 12 | 0 | 0 | 0 | - | 静态规则脱水 |
| syslog | missing_audit | general | - | 24 | 12 | 0 | 0 | 0 | - | 系统日志脱水 |
| template_driven | missing_audit | general | - | 24 | 12 | 0 | 0 | 0 | - | 模板驱动输出脱水 |
| terraform | missing_audit | infra | - | 24 | 12 | 0 | 0 | 0 | - | Terraform plan/apply output compaction |
| unity_unreal | missing_audit | general | - | 30 | 15 | 0 | 0 | 0 | - | Unity/Unreal 构建日志脱水 |
| vcs | orchestrator | general | vcs | 0 | 0 | 0 | 0 | 0 | - |  |
| vcs_az | missing_audit | vcs | - | 18 | 9 | 0 | 0 | 0 | - | Azure DevOps CLI 输出脱水 |
| vcs_bitbucket | missing_audit | vcs | - | 18 | 9 | 0 | 0 | 0 | - | Bitbucket CLI 输出脱水 |
| vcs_bzr | missing_audit | vcs | - | 26 | 13 | 0 | 0 | 0 | - | Bazaar 命令输出脱水 |
| vcs_cvs | missing_audit | vcs | - | 28 | 14 | 0 | 0 | 0 | - | CVS 命令输出脱水 |
| vcs_darcs | missing_audit | vcs | - | 20 | 10 | 0 | 0 | 0 | - | Darcs 命令输出脱水 |
| vcs_fossil | missing_audit | vcs | - | 20 | 10 | 0 | 0 | 0 | - | Fossil 命令输出脱水 |
| vcs_gerrit | missing_audit | vcs | - | 18 | 9 | 0 | 0 | 0 | - | Gerrit 命令输出脱水 |
| vcs_gh | missing_audit | vcs | - | 40 | 20 | 0 | 0 | 0 | - | GitHub CLI 输出脱水 |
| vcs_git | missing_audit | vcs | - | 176 | 88 | 0 | 0 | 0 | - | Git 命令输出脱水 |
| vcs_glab | missing_audit | vcs | - | 14 | 7 | 0 | 0 | 0 | declared_without_case_evidence:gitlab | GitLab CLI 输出脱水 |
| vcs_hg | missing_audit | vcs | - | 98 | 49 | 0 | 0 | 0 | - | Mercurial 命令输出脱水 |
| vcs_p4 | missing_audit | vcs | - | 88 | 44 | 0 | 0 | 0 | - | Perforce 命令输出脱水 |
| vcs_repo | missing_audit | vcs | - | 22 | 11 | 0 | 0 | 0 | - | Android Repo 命令输出脱水 |
| vcs_svn | missing_audit | vcs | - | 108 | 54 | 0 | 0 | 0 | - | SVN 命令输出脱水 |
| web_log | missing_audit | web_log | - | 96 | 48 | 0 | 0 | 0 | - | Web access log v3 semantic aggregation for Nginx, Apache, ingress, Uvicorn, Envoy/Istio, CloudFront/IIS W3C, Cloudflare, native ALB, and cloud-wrapped CSV/JSON/table/plain access logs with dictionaries, routine folding, noise diagnostics, scan/burst spotlight, anomalies, and slow request signals |
| webpack_vite | missing_audit | web_log | - | 24 | 12 | 0 | 0 | 0 | - | Webpack/Vite 构建日志脱水 |
| xcode_log | missing_audit | general | - | 24 | 12 | 0 | 0 | 0 | - | Xcode 构建日志脱水 |
| xml_html | missing_audit | data_format | - | 24 | 12 | 0 | 0 | 0 | - | XML and HTML structure compaction |
| yaml | missing_audit | data_format | - | 28 | 14 | 0 | 0 | 0 | - | YAML structure compaction |