# TokenSlim Semantic Audit Profiles

本文档只供 `scripts/audit_case_metrics.py` 的 LLM 语义门禁读取。

设计原则：
- 只描述“语义保真”判断标准，不包含接管口令、人工执行流程、历史 needs_fix 处方或具体命令。
- `CLAUDE.md` / Compression Protocol V1 是最高优先级；本文件只是按插件族群补充“哪些字段不能丢”。
- Profile 内容必须短且稳定，作为 system prompt 的 cacheable prefix 使用。
- 动态 case 文本、case_id、字节数只能放在 user prompt 末尾，不能放入本文件。

## default

- Preserve the first recognizable command, diagnostic header, protocol signature, or log type signature as the semantic anchor.
- Preserve decision-critical identifiers: file paths, line/column numbers, test names, rule IDs, status codes, package/module names, request paths, resource names, and object IDs.
- Preserve all error/fatal/panic/exception/failure/conflict signals and at least one representative message for each distinct anomaly class.
- Aggregation is allowed only when counts, key dimensions, and representative anomalous samples remain visible.
- Empty original input may compact to empty only when the case is explicitly an empty-input case.

## vcs-common

- Preserve command anchor, repository operation intent, changed paths, statuses, authors/owners, revisions/commit IDs, branches, tags, conflicts, and commit messages.
- Native status symbols are acceptable; do not fail merely because a plugin keeps native VCS notation instead of TokenSlim labels.
- Diff compaction must preserve file boundaries, hunk intent, binary/truncation notes, and enough changed context for LLM review.
- Push/pull/fetch/merge/rebase/sync operations must preserve direction, target/source, result, and conflict/failure state.

## vcs-dvcs

- Applies to Git, Hg, Bzr, Darcs, and Fossil.
- Log/timeline/history output must retain revision/check-in identity, author, date/order, branch/bookmark/tag context, and message summary.
- Status output must retain each changed file's state and path.
- Stash/shelve/worktree/bookmark/branch operations must preserve object identity and active/current markers.

## vcs-centralized

- Applies to SVN, P4, and CVS.
- Revision/change numbers, depot/repository paths, workspace/client names, changelist IDs, and lock/edit/opened state are decision-critical.
- Update/commit/submit/sync output must retain action per file and final conflict/error state.
- Annotate/blame output must retain minimum useful ownership/revision/date information per code line or compact group.

## vcs-cloud-cli

- Applies to GitHub CLI, GitLab CLI, Azure DevOps, Bitbucket, Gerrit, and Android repo CLI.
- Preserve PR/MR/issue/change/run IDs, titles/subjects, state, author/owner, branch/project/repository, CI conclusion, and reviewer/vote labels.
- Table spacing may be removed, but column meaning must remain reconstructable.
- API/query pagination or row-count summaries may be compacted, but result count and empty/error states must remain unambiguous.

## build-compiler

- Applies to build tools, compilers, package managers, IaC tools, and mobile build logs.
- Preserve failing target/task/module, diagnostic code, severity, file:line:column, package/artifact name, command/tool name, and failed dependency/resource.
- Progress/download/cache noise may be removed when it does not explain a failure.
- Successful clean/no-op builds may be summarized, but must not look like failure or hide warning/error counts.

## runtime-trace

- Applies to stack traces, runtime exceptions, test failures, language errors, and code diagnostics.
- Preserve exception class, top-level message, root stack frame, relevant file:line locations, caused-by chain, and repeated distinct failures.
- Stack frames may be deduplicated or truncated only with an explicit marker and only after the root cause remains visible.
- Do not hide language-specific error names such as TypeError, SyntaxError, NullPointerException, panic, traceback, or fatal.

## structured-log

- Applies to web/cloud/database/syslog/container/CI logs and repeated operational events.
- Preserve provider/service/stream identity, timestamp/order, level, request path, method/status, duration/latency, host/pod/container, and error payloads.
- Routine health checks, static assets, and repeated 2xx/success events may be aggregated.
- Minority anomalies such as 4xx/5xx bursts, slow requests, auth failures, crash loops, and database errors must be isolated with counts and representative samples.

## data-format

- Applies to JSON/YAML/XML/NDJSON/Markdown/SQL/Protobuf/artifact summaries and diff-like structured payloads.
- Preserve schema shape, keys, IDs, rule/test names, severity, location, failing assertion, and relationship between nested fields.
- Formatting whitespace may be removed, but key-value association and nesting meaning must remain recoverable.
- SARIF/JUnit/test artifacts must preserve failing/error/skipped tests, rule IDs, tool names, file locations, and summary counts.

## utility-text

- Applies to generic text, ANSI cleaner, noise filter, smart path, static rule, template-driven, and similar utility plugins.
- Preserve the first meaningful content line, all warnings/errors, and any explicit clean/empty/no-op state.
- Removing ANSI/control noise is valid; removing the only semantic content is not.
- If the plugin is expected to be pass-through for a short sample, semantic equality is more important than inventing structure.

## shell-command

- Applies to Windows cmd.exe, PowerShell, POSIX sh/bash/zsh/fish, Git Bash, WSL, and CI shell transcripts.
- Preserve the first executed command as the absolute anchor, plus shell flavor, working directory, environment assignments, flags, pipes, redirections, quoting, and script names when visible.
- Preserve stdout/stderr distinction, exit code, signal/timeout/cancel state, and shell-native failure classes such as command-not-found, permission/access denied, parser error, parameter binding error, execution policy, no-such-file, and pipe failure.
- Repeated successful file operations or progress lines may be aggregated, but failures must remain isolated with counts and representative paths/messages.
- Empty successful output must remain unambiguous, for example command anchor plus clean/exit-zero state; non-zero exit without output must still preserve failure state.
