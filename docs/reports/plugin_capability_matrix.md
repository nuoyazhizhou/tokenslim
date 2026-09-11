# Plugin Capability Matrix

- generated_at: 2026-09-11T14:37:02.839519
- plugins_total: 66
- audited_plugins: 60
- frozen_plugins: 60
- coverage_gaps: 2
- authoritative sources: config/plugins/*.json, samples/*, src/plugins/*/showcase.rs, docs/audit/*

| plugin | status | tags | route | samples | showcase | audit | frozen | auditing | warnings | description |
| ------ | ------ | ---- | ----- | ------: | -------: | ----: | -----: | -------: | -------- | ----------- |
| android_gradle | frozen | build_log | - | 22 | 22 | 22 | 22 | 0 | - | Android Gradle 构建日志脱水 |
| ansi_cleaner | frozen | general | - | 12 | 12 | 12 | 12 | 0 | - | ANSI escape sequence cleaner |
| ansible | frozen | infra | - | 12 | 12 | 12 | 12 | 0 | - | Ansible play/task output compaction |
| artifact_summary | frozen | build_log,data_format,test_log | - | 12 | 12 | 12 | 12 | 0 | - | Build artifact summary compaction for SARIF security/code scanning results and JUnit XML test reports |
| backfill | unknown | general | - | 0 | 0 | 0 | 0 | 0 | - |  |
| bazel | frozen | build_log,test_log | - | 12 | 12 | 12 | 12 | 0 | - | Bazel build/test output compaction |
| ci_log | frozen | ci_cd | build | 44 | 44 | 44 | 44 | 0 | - | CI/CD shell wrapper semantic compaction for GitHub Actions, GitLab CI, Jenkins, Azure Pipelines, CircleCI, Buildkite, local act, TeamCity, Travis CI, and organization-specific banner logs |
| cloud_log | frozen | cloud_log | - | 52 | 52 | 52 | 52 | 0 | - | 主流云厂商日志外壳剥离（AWS/GCP/Azure/阿里云/OCI/腾讯云/华为云/Cloudflare），将 message/textPayload/content 等内层日志还原给专用日志插件 |
| cloudformation | frozen | cloud_log,infra | - | 12 | 12 | 12 | 12 | 0 | - | AWS CloudFormation event compaction |
| db_log | frozen | database,stack_trace | - | 22 | 22 | 22 | 22 | 0 | - | Database log compaction for PostgreSQL, MySQL, MongoDB, and Redis slow/error/replication/runtime events |
| dotnet | frozen | general | - | 12 | 12 | 12 | 12 | 0 | - | .NET 构建日志脱水 |
| encoding_fallback | missing_audit | general | - | 11 | 0 | 0 | 0 | 0 | - | 编码兜底 |
| explain | meta | general | - | 1 | 0 | 0 | 0 | 0 | - | 解释插件 |
| gcc_log | frozen | build_log | - | 36 | 36 | 36 | 36 | 0 | - | GCC/Clang 编译日志脱水，含 nm/readelf/size/objdump 符号·节表·反汇编·重定位和 ar 归档成员/创建等 binutils 工具输出压缩。 |
| generic_text | frozen | general | generic | 15 | 15 | 15 | 15 | 0 | - | 通用文本脱水 |
| git_diff | frozen | vcs | - | 12 | 12 | 12 | 12 | 0 | - | Git diff 输出脱水 |
| helm | frozen | infra | - | 12 | 12 | 12 | 12 | 0 | - | Helm install/upgrade output compaction |
| java_stack | frozen | stack_trace | - | 16 | 16 | 16 | 16 | 0 | - | Java 堆栈跟踪脱水 |
| json | frozen | data_format | - | 12 | 12 | 12 | 12 | 0 | - | JSON structure compaction |
| kubernetes_docker | frozen | infra | - | 24 | 24 | 24 | 24 | 0 | - | Kubernetes/Docker 输出脱水 |
| ls_listing | frozen | cloud_log | - | 1 | 1 | 1 | 1 | 0 | - | columnar directory listing compaction (aws s3 ls / ls -l family) |
| markdown | frozen | general | - | 12 | 12 | 12 | 12 | 0 | - | Markdown 脱水 |
| maven | frozen | build_log | - | 15 | 15 | 15 | 15 | 0 | - | Maven 构建日志脱水 |
| minify_code | disabled_config | general | - | 0 | 0 | 0 | 0 | 0 | - | Deprecated config-only entry; smart_code_plugin owns code compaction |
| ndjson | frozen | data_format | - | 12 | 12 | 12 | 12 | 0 | - | NDJSON 脱水 |
| node_error | frozen | stack_trace | - | 12 | 12 | 12 | 12 | 0 | - | Node.js 错误堆栈脱水 |
| nodejs | frozen | stack_trace,test_log,web_log | - | 25 | 25 | 25 | 25 | 0 | - | Node.js 日志脱水，含 npm/yarn/pnpm install、tsc、eslint、webpack、jest、vitest 测试输出折叠。 |
| noise_filter | frozen | general | - | 12 | 12 | 12 | 12 | 0 | - | 噪声过滤 |
| php_ruby | frozen | general | - | 12 | 12 | 12 | 12 | 0 | - | PHP/Ruby 错误栈脱水 |
| privacy | source_only | general | - | 0 | 0 | 0 | 0 | 0 | - | 在其他压缩插件之前单向替换高置信度凭证，防止敏感值外发给 AI。 |
| protobuf | frozen | general | - | 12 | 12 | 12 | 12 | 0 | - | protoc/buf diagnostic compaction |
| pulumi | frozen | infra | - | 12 | 12 | 12 | 12 | 0 | - | Pulumi preview/up output compaction |
| pytest | frozen | test_log | - | 18 | 18 | 18 | 18 | 0 | - | pytest session and result compaction |
| python_traceback | frozen | stack_trace | - | 16 | 16 | 16 | 16 | 0 | - | Python 堆栈跟踪脱水 |
| rust_go | frozen | stack_trace | - | 20 | 20 | 20 | 20 | 0 | - | Rust/Go 日志脱水 |
| shell_session | frozen | general | - | 80 | 80 | 80 | 80 | 0 | - | Shell 会话脱水 |
| smart_code | frozen | general | - | 12 | 12 | 12 | 12 | 0 | - | Smart code detection and compaction |
| smart_path | frozen | general | - | 12 | 12 | 12 | 12 | 0 | - | 智能路径脱水 |
| spring_boot | frozen | general | - | 15 | 15 | 15 | 15 | 0 | - | Spring Boot 应用日志脱水 |
| sql | frozen | database | - | 12 | 12 | 12 | 12 | 0 | - | SQL 输出脱水 |
| static_rule | frozen | general | - | 12 | 12 | 12 | 12 | 0 | - | 静态规则脱水 |
| syslog | frozen | general | - | 12 | 12 | 12 | 12 | 0 | - | 系统日志脱水 |
| template_driven | frozen | general | - | 12 | 12 | 12 | 12 | 0 | - | 模板驱动输出脱水 |
| terraform | frozen | infra | - | 12 | 12 | 12 | 12 | 0 | - | Terraform plan/apply output compaction |
| toml_ini | frozen | general | - | 3 | 3 | 3 | 3 | 0 | - | TOML/INI 配置结构压缩 |
| unity_unreal | frozen | general | - | 15 | 15 | 15 | 15 | 0 | - | Unity/Unreal 构建日志脱水 |
| vcs | orchestrator | general | vcs | 0 | 0 | 0 | 0 | 0 | - |  |
| vcs_az | frozen | vcs | - | 9 | 9 | 9 | 9 | 0 | - | Azure DevOps CLI 输出脱水 |
| vcs_bitbucket | frozen | vcs | - | 9 | 9 | 9 | 9 | 0 | - | Bitbucket CLI 输出脱水 |
| vcs_bzr | frozen | vcs | - | 13 | 13 | 13 | 13 | 0 | - | Bazaar 命令输出脱水 |
| vcs_cvs | frozen | vcs | - | 14 | 14 | 14 | 14 | 0 | - | CVS 命令输出脱水 |
| vcs_darcs | frozen | vcs | - | 10 | 10 | 10 | 10 | 0 | - | Darcs 命令输出脱水 |
| vcs_fossil | frozen | vcs | - | 10 | 10 | 10 | 10 | 0 | - | Fossil 命令输出脱水 |
| vcs_gerrit | frozen | vcs | - | 9 | 9 | 9 | 9 | 0 | - | Gerrit 命令输出脱水 |
| vcs_gh | frozen | vcs | - | 20 | 20 | 20 | 20 | 0 | - | GitHub CLI 输出脱水 |
| vcs_git | frozen | vcs | - | 86 | 86 | 86 | 86 | 0 | - | Git 命令输出脱水 |
| vcs_glab | frozen | vcs | - | 7 | 7 | 7 | 7 | 0 | declared_without_case_evidence:gitlab | GitLab CLI 输出脱水 |
| vcs_hg | frozen | vcs | - | 48 | 48 | 48 | 48 | 0 | - | Mercurial 命令输出脱水 |
| vcs_p4 | frozen | vcs | - | 44 | 44 | 44 | 44 | 0 | - | Perforce 命令输出脱水 |
| vcs_repo | frozen | vcs | - | 11 | 11 | 11 | 11 | 0 | - | Android Repo 命令输出脱水 |
| vcs_svn | frozen | vcs | - | 54 | 54 | 54 | 54 | 0 | - | SVN 命令输出脱水 |
| web_log | frozen | web_log | - | 48 | 48 | 48 | 48 | 0 | - | Web access log v3 semantic aggregation for Nginx, Apache, ingress, Uvicorn, Envoy/Istio, CloudFront/IIS W3C, Cloudflare, native ALB, and cloud-wrapped CSV/JSON/table/plain access logs with dictionaries, routine folding, noise diagnostics, scan/burst spotlight, anomalies, and slow request signals |
| webpack_vite | frozen | web_log | - | 12 | 12 | 12 | 12 | 0 | - | Webpack/Vite 构建日志脱水 |
| xcode_log | frozen | general | - | 11 | 11 | 11 | 11 | 0 | - | Xcode 构建日志脱水 |
| xml_html | frozen | data_format | - | 12 | 12 | 12 | 12 | 0 | - | XML and HTML structure compaction |
| yaml | frozen | data_format | - | 14 | 14 | 14 | 14 | 0 | - | YAML structure compaction |