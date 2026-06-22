# TokenSlim Plugins Registry

> TokenSlim ships **55+ plugins** that classify and compress structured text
> (VCS output, build logs, runtime traces, structured data formats, shell sessions, etc.).
>
> Each plugin is configured under [`config/plugins/`](./config/plugins/) (JSON).
> Plugins are dispatched in **priority order** (lower value = matched first; see
> [`src/core/plugin_config_loader/mod.rs`](./src/core/plugin_config_loader/mod.rs)).
>
> For detailed VCS command coverage, see
> [`docs/design/vcs_coverage_matrix.md`](./docs/design/vcs_coverage_matrix.md).
>
> For the per-plugin design source, see
> [`docs/development/PLUGIN_DEVELOPMENT.md`](./docs/development/PLUGIN_DEVELOPMENT.md).

---

## 路由优先级与列说明

| 列 | 含义 |
|---|---|
| **Priority** | 路由评分（值越小越优先；VCS 系列固定为 200，由 `vcs_plugin.route.json` 按子命令二次分发） |
| **Plugin** | 插件 ID（即 config 文件名去掉 `.json`） |
| **What it does** | 一句话功能描述（来自 `config/plugins/*.json` 的 `description` 字段，并与 `src/plugins/<name>_plugin/mod.rs` 顶部 `//!` 注释对齐） |
| **Commands / Coverage** | 该插件明确支持的输入命令或压缩目标（VCS 系列从 `vcs_coverage_matrix.md` 抄录） |
| **保留信号** | 压缩过程中必须保留的语义关键行（来自 mod.rs 顶部注释的 `## 保留信号` 段） |
| **压缩目标** | 折叠/聚合/字典化的可丢弃行（来自 mod.rs 顶部注释的 `## 压缩目标` 段） |
| **Detection signals** | 路由签名（config 中 `detect.rules[*].patterns`；VCS 走子命令分发，签名由 route 文件决定） |

匹配失败一律回退到 `generic_text_plugin`（priority 0，最低，作为兜底）。

---

## VCS plugins (15)

所有 VCS 插件 priority 固定为 200，由 [`config/plugins/vcs_plugin.route.json`](./config/plugins/vcs_plugin.route.json) 按子命令二次分发，避免 `git` 误触发 `vcs_gh`/`vcs_glab` 等。

| Priority | Plugin | What it does | Commands / Coverage | 保留信号 | 压缩目标 | Detection signals |
|---|---|---|---|---|---|---|
| 200 | `vcs_git_plugin` | Git 命令输出脱水：保留命令核心输出，折叠图形进度，压缩网络噪音和分页信息。 | `status` / `diff` / `log` / `show` / `branch` / `checkout` / `switch` / `merge` / `rebase` / `reset` / `stash` / `fetch` / `pull` / `push` / `remote` / `tag` / `cherry-pick` / `revert` / `blame` / `bisect` / `restore` / `clean` / `submodule` | 文件路径（diff/status/add等）、状态标记（M/A/D等）、提交哈希与提交信息（log）、reflog 条目（最多20条）、冲突标记（merge冲突）、分支切换信息（checkout reflog） | 进度行（fetch/push/pull 网络噪音）、图形字符（*\|/\ 图形输出）、超出的 reflog 条目（>20 折叠）、装饰（origin/->o/、tag: ->t:）、summary 词长压缩（files changed -> files 等）、分页提示行（--More-- 等） | route: `git ` 前缀 → `vcs_git` |
| 200 | `vcs_svn_plugin` | SVN 命令输出脱水：保留命令锚点和变更状态，压缩路径。 | `status` / `diff` / `log` / `info` / `add` / `delete` / `move` / `copy` / `commit` / `update` / `checkout` / `revert` / `merge` / `switch` / `resolve` / `cleanup` / `blame` | 原始命令锚点、文件变更状态（A/D/M/U/C）、commit 信息 | 长路径替换为 `$SVN` 令牌字典、重复行去重 | route: `svn ` 前缀 → `vcs_svn` |
| 200 | `vcs_hg_plugin` | Hg（Mercurial）命令输出脱水：保留命令首行、变更集、文件操作摘要，折叠重复 grafting 和注释行。 | `status` / `diff` / `log` / `summary` / `add` / `remove` / `rename` / `commit` / `update` / `branch` / `pull` / `push` / `annotate` / `graft` / `rebase` / `shelve` / `unshelve` / `revert` | 命令首行（如 hg update）、变更集 ID（grafting 行中提取）、文件操作摘要（如 `3 updated, 2 removed`）、分支名（含 `~` 表示 inactive）、日期（YYYY-MM-DD HH:MM）、histedit 命令（pick/edit/fold 等）、shelve 列表条目（--list 时） | 重复 grafting 行合并为一条（计数）、注释行（# 开头）与空行、分支 inactive 状态（替换为 `~`）、日期（压缩为紧凑格式）、跳过 merging 行（详细信息） | route: `hg ` 前缀 → `vcs_hg` |
| 200 | `vcs_p4_plugin` | P4（Perforce）输出脱水：保留 change 编号、文件操作与结果，折叠统计摘要，压缩长路径和日期。 | `opened` / `changes` / `describe` / `diff` / `submit` / `sync` / `edit` / `add` / `delete` / `revert` / `integrate` / `resolve` / `reconcile` / `shelve` / `unshelve` / `files` / `client` | change 编号（如 1234）、文件操作（opened/edit/add）、精简后的 depot 路径（公共前缀压缩）、命令结果（resolved/would-update）、差异统计摘要（文件数:增加-删除） | 长路径替换为公共前缀（`$VCS_P4`）、日期时间截断为 19 字符 YYYY-MM-DD HH:MM:SS、文件大小数字转人类可读（1234567 -> 1.2M）、diff 头行（---/+++）丢弃、叙事性噪音行丢弃、同步预览摘要压缩为"数 would-update" | route: `p4 ` 前缀 → `vcs_p4` |
| 200 | `vcs_cvs_plugin` | CVS 脱水：保留命令锚点和文件状态，折叠噪音和分隔线，压缩键值对。 | `status` / `diff` / `log` / `add` / `remove` / `commit` / `update` / `checkout` / `tag` / `annotate` / `edit` / `unedit` / `release` / `history` | cvs 开头的命令锚点行、状态码映射行（U:/M:/A:）、Index: 行（diff 文件索引）、错误/冲突行（conflict / error:）、No edits: 行（unedit 无编辑文件） | 空行、等号分隔线（≥8 全等号）、重复 cvs 命令锚点（只保留第一条）、CVS 噪音行（cvs server: 信息）、日志模板行（is_cvs_log_boilerplate）、键值对压缩（Working revision: → WR:）、状态长词压缩（Up-to-date → OK） | route: `cvs ` 前缀 → `vcs_cvs` |
| 200 | `vcs_bzr_plugin` | Bazaar 脱水：保留命令锚点和文件状态，折叠噪音和空行，压缩长路径状态。 | `status` / `diff` / `log` / `add` / `remove` / `commit` / `update` / `branch` / `pull` / `push` / `merge` / `resolve` / `missing` / `revert` | bzr 命令锚点行（bzr status / bzr log）、文件状态行（modified/added 等）、reverted 行（ST:R）、commit 行、pull/merge/push 行、警告/错误信息（map_bzr_alert） | 噪音行（进度条、统计信息等）、空行、重复 bzr 命令锚点（只保留第一个）、长路径状态文本（映射为短格式） | route: `bzr ` 前缀 → `vcs_bzr` |
| 200 | `vcs_fossil_plugin` | Fossil 脱水：保留命令锚点和状态变更摘要，折叠叙事废话，压缩元数据噪音。 | `status` / `diff` / `timeline` / `changes` / `add` / `rm` / `commit` / `update` / `sync` / `checkout` / `merge` / `stash` / `undo` / `tag` | fossil status/changes 等命令锚点行、映射后的状态码行（M/A/D/R/!）、Pull/Push 行（sync 命令）、警报行（map_fossil_alert）、非噪音非叙事的有效行（generic fallback） | 空行、元数据噪音行（Repository/Check-ins 等）、叙事废话行（Stash changes/Autosync 等）、非锚点的 fossil 命令前缀行（重复命令）、状态变化前的冗长描述行（语法覆盖） | route: `fossil ` 前缀 → `vcs_fossil` |
| 200 | `vcs_darcs_plugin` | Darcs 脱水：保留命令锚点与关键操作信息，折叠无关噪声和冗余行。 | `whatsnew` / `diff` / `changes` / `record` / `pull` / `push` / `rebase` / `add` / `remove` / `revert` / `tag` / `amend-record` / `obliterate` | darcs 命令锚点行（darcs log / darcs status）、补丁结构信息（hash/author/date/subject/files）、Old message: 和 New message: 行（amend 命令）、Rebasing from: 和 Rebasing to: 行（rebase 命令）、警报映射行 | 空行、噪声行（is_darcs_noise 过滤的无关行）、长输出中超出 cost_gate 阈值的冗余行、通用 fallback 中第一条之后的 darcs 命令 | route: `darcs ` 前缀 → `vcs_darcs` |
| 200 | `vcs_gh_plugin` | GitHub CLI 输出脱水：保留命令锚点和结构化数据行，折叠块内多行，压缩 URL 和键名。 | `gh pr list/view/merge/checkout/create`、`gh issue list/view/create/close`、`gh release list/view/create`、`gh run list/view/watch`、`gh repo view/clone/fork` | gh 命令本身（"gh pr list"）、PR/Issue 表格行（带序号和标题的行）、KV 对中的短符号键值（ST:success 等）、去符号后的状态行（含缩写 URL）、通用非空非分隔符非表头行（经空格压缩） | 空行、分隔线（"---" 行）、表头行（列标题行）、状态符号 ✓ ✗ ○（替换为空格后去除）、连续多个空格（压缩为单个空格）、URL 缩写（github.com/... → URL:...）、长键名替换为短符号（workflow → WF、status → ST）、run list 中块内多行合并为一行 | route: `gh ` 前缀 → `vcs_gh` |
| 200 | `vcs_glab_plugin` | GitLab CLI 输出脱水：保留命令及关键结果，折叠表格格式和噪声行，压缩长描述和冗余信息。 | `glab mr list/view/merge/create`、`glab issue list/view/create/close`、`glab ci status/view/lint/trigger`、`glab repo clone/fork/view` | glab 命令起始行、MR/Issue 列表行（精简行，如 `!123 Title [state] author date`）、MR/Issue 元数据（title/state/author/date 等）、创建结果（如 `A:!456`）、view 中的 Description 内容 | 空行、分隔符和表格标题（`---` / `---+` 等）、成功标记 ✓、URL 行（以 http 或 URL: 开头）、噪声行（is_glab_noise / is_glab_view_noise）、创建过程中的提示行（Creating merge request） | route: `glab ` 前缀 → `vcs_glab` |
| 200 | `vcs_az_plugin` | Azure DevOps CLI 输出脱水：保留命令锚点、关键 K-V 和项目信息，折叠空行、括号和噪音行，压缩 URL 为短形式。 | `az repos show/list`、`az pipelines run/list/show`、`az pr list/show/create/complete`、`az repos policy` | az repos show 等命令锚点行、警报行（map_az_alert）、K-V 行（key:value 映射为 BR/URL/SS/REPO/PRJ/ID 等符号）、列表输出中的 name/id/defaultBranch/project 字段、创建/删除结果中的 Repository created/deleted 行 | 空行、JSON 数组括号 `[]` 和对象花括号 `{}`（过滤）、重复的 "az ..." 锚点行（仅保留第一个）、噪音行（is_az_noise）、长 URL（remoteUrl/webUrl 缩写为短形式）、无冒号行视为项目名（标记为 PRJ:） | route: `az ` 前缀（限定 `az repos`/`az pipelines`/`az pr` 子命令）→ `vcs_az` |
| 200 | `vcs_bitbucket_plugin` | Bitbucket CLI 输出脱水：保留 PR/Issue 关键元数据，折叠表头分隔线和噪声信息，压缩为紧凑格式。 | `bitbucket pr list/view/create/merge`、`bitbucket pipeline list/run`、`bitbucket repo list/view` | 命令锚点（bitbucket pr list 等）、PR 列表数据行（#ID ST:STATE OW:@author title）、PR 视图元数据（状态/描述/分支等）、PR 创建结果（Created PR #...）、Issue 列表数据行（#ID ST:STATUS OW:@assignee title PRI:priority）、源代码分支映射（Source:feature-auth->main） | 空行、表头行（全大写缩写的标题行）、分隔线（全由 `-` 或 `=` 组成的长行）、噪声信息（Created/Updated/Participants/Comments 等）、URL 行（URL: 或 http 开头）、冗余标题行（如 "Pull request #..."） | route: `bitbucket ` 前缀 → `vcs_bitbucket` |
| 200 | `vcs_repo_plugin` | Android Repo 命令输出脱水：保留命令锚点和项目/状态/推送信息，折叠进度条、URL 等噪音，压缩 diff 文件路径和 hunk 头。 | `repo status/sync/init/upload/diff/forall/start/branches/manifest` | repo sync/status/upload 命令锚点（第一行）、project 行（项目路径与状态/哈希）、推送映射行（HEAD -> refs/...）、diff 文件路径（压缩为 D: 前缀）、hunk 头（压缩后格式） | 进度噪音行（Downloading... / Syncing: ... / Syncing done.）、SSH/HTTPS URL 行、重复的命令锚点（仅保留第一行）、URL 行（ssh:// / http:// / https://）、diff 行中 a/ b/ 路径前缀（压缩为 D:）、hunk 头中空格和上下文（压缩为 `@@-a,b->c,d@@`） | route: `repo ` 前缀 → `vcs_repo` |
| 200 | `vcs_gerrit_plugin` | Gerrit code review 输出脱水：保留命令锚点与核心变更摘要，折叠噪音与长 URL，压缩为符号化摘要。 | `git push` 触发 review 流程（Change-Id / Submit+ / CR+ / Code-Review+ / Verified+ / Workflow+ 等标签合并至命令锚点）、`gerrit query`、`gerrit ls-projects` | 命令锚点行（"gerrit query"）、change ID 行（压缩为 CHG @xxx）、关键 key-value 字段（project/branch/status 等，压缩为 PRJ/BR 等）、reviewer 列表（逗号分隔）、review 标签（如 Code-Review+2）、push ref 映射（压缩后保留源和目标分支名）、checkout 分支和状态（Switched to branch / up-to-date） | 噪音行（Counting objects / remote: 等）、长 URL（http 或 Push to ssh:// 开头）、进度条/传输中行（以 "..." 结尾且长度 < 40）、空行、重复的 URL、change ID 完整格式压缩为 CHG @+短id、key-value 字段键名压缩为短符号（project → PRJ）、refs/heads/ 前缀在 push 映射中移除 | route: `URL:gr:` / `CHG @` / `Submit+` / `CR+` 等语义锚点 → `vcs_gerrit` |
| 100 | `vcs_plugin` (route) | 旧版 VCS 派发中心（已割接，保留作 fallback）：保留版本控制命令的语义结构，折叠冗余空白和路径信息，压缩差异输出。 | Git/Hg/SVN/P4 兼容入口 | diff -- 开头行（差异头部）、@@ 开头行（差异块范围）、+++ / --- 开头行（文件路径）、状态指示符（M/A/D）、[paths] 路径字典、日期时间令牌（YYYYMMDD HH:MM） | 前导连续空白（非 Python 文件或 hash 范围）、内联对齐多空白（非表格或代码注释保护）、长绝对路径（替换为 [paths] 字典条目）、目录树结构（如 git checkout 输出缩进）、SVN 更新输出中的冗余行、差异输出中的重复文件路径信息 | route: 兜底分发到上述 14 个独立 VCS 插件 |

> **VCS plugin 共同点**：保留原始触发命令作为 IR 输出绝对第一行（压缩协议法则 0），路径走字典压缩（`$P` Token），用户名/邮箱提取 `@user` 短 Token。

---

## Build / CI plugins (16)

| Priority | Plugin | What it does | 保留信号 | 压缩目标 | Detection signals |
|---|---|---|---|---|---|
| 190 | `android_gradle_plugin` | Android/Gradle 构建日志脱水：保留错误 Task 和资源警告，折叠重复 Task 状态行，压缩构建路径和环境变量。 | `> Task ... FAILED` 行（构建失败的任务）、`warn: removing resource` 行（资源删除警告）、`WORKSPACE` 等环境变量行（Jenkins 关键变量） | 非 FAILED 的 Task 行（UP-TO-DATE 等）折叠为计数、连续 5 个以上相同包名的资源警告合并、构建路径替换为 `$GRADLE` 令牌字典 | `[GRADLE] tasks=`, `FAILURE`, `testDebugUnitTest`, `Run ./gradlew` |
| 91 | `ansible_plugin` | Ansible play/task 输出脱水：保留任务头和主机状态，折叠重复任务细节，压缩语法错误。 | TASK [名称] 行、RUNNING HANDLER [名称] 行、ok/changed/failed/unreachable/skipping: [主机] 行、PLAY RECAP 行、ERROR! 语法错误行 | 重复的任务输出折叠为单行摘要、主机列表合并为范围格式如 `host[1,2]`、详情中的 msg 字段提取并压缩、语法错误多行压缩为单行、空行和无关注释行删除 | `TASK [Gathering Facts]`, `PLAY RECAP`, `RECAP:` |
| 95 | `bazel_plugin` | Bazel build/test 输出脱水：保留错误行和关键构建摘要，折叠普通 INFO 日志，压缩目标列表。 | `error:` 行、BUILD 完成行（Build completed）、INFO: Analyzed 行、bazel version 摘要行、bazel query 目标行 | 普通 INFO 日志行（折叠丢弃）、冗长的目标列表（压缩为 `TARGETS[count]`）、重复行去重 | `INFO: Analyzed`, `INFO: Build completed`, `bazel query` |
| 174 | `ci_log_plugin` | CI/CD 日志脱水：保留步骤、错误、警告和状态信号，折叠详细日志行为统计摘要。 | `::error` / `::warning` / `::group::` / `::endgroup::` 行、section_start: / section_end: 行、`##[error]` 行、finished: failure 行、process completed with exit code 行 | 步骤内的非关键日志行（折叠为行数统计）、缓存操作行（压缩为缓存计数）、重试操作行（压缩为重试计数）、空行（完全丢弃） | `CI\|SUMMARY\|provider=`, `##[endgroup]`, `##teamcity[...]` |
| 90 | `cloud_log_plugin` | 云日志外壳剥离：保留命令提示和记录结构，折叠元数据字段，压缩长值和时间。 | command_lines（命令提示行，如 aws logs tail）、records 中的消息文本、passthrough 行、CSV 输出或访问摘要 | 长结构化字段值截断为 `<TRUNCATED>`（>360 字符）、示例字段压缩为摘要格式 `[pos:shop:price eta= rating=]`、时间字段精简为"日期 时间"、资源路径缩短为前两部分+后 8 字符、多余空白字符合并为单个空格 | `HikariPool timeout`, `ocid1.`, `aws logs tail`, `TimeGenerated` |
| 93 | `cloudformation_plugin` | CloudFormation 事件行脱水：保留失败/回滚事件，折叠重复状态，压缩冗长输出。 | 事件行（状态+资源 ID）、失败/回滚行（FAILED 或 ROLLBACK 状态）、错误信号行（keep_error_signal） | 空行或全分隔符行、锚行（首行非空内容）、重复状态事件行（按状态计数折叠） | `FAILED`, `EVENTS:`, `CREATE_FAILED`, `ResourceChange:` |
| 200 | `dotnet_plugin` | .NET 构建日志脱水：保留错误定位和堆栈帧方法名，折叠冗余参数。 | `at method(...) in file:line`（堆栈帧行）、`file(line,col): error code: message`（MSBuild 错误行）、含 `System.` / `Microsoft.` 的行（疑似 .NET 相关） | 堆栈帧参数列表（替换为 `(...)`） | `Microsoft.`, `System.` |
| 200 | `gcc_log_plugin` | GCC/Clang 编译日志脱水：保留错误、警告和关键信息，折叠重复警告，压缩长路径和宏定义。 | `error:` 行、`warning:` 行（相同警告类型的前 N 次）、`note:` 行、`undefined reference` 行、`Build files have been written to:` 等关键行、CMake Error 等构建错误行、`Error 1` 或 `Error 2` 结尾的行 | 长路径替换为 `$GCC` 令牌字典、宏定义（-D...）替换为令牌、相同 warning 超过阈值的后续行（折叠）、重复行去重（threshold=1） | `undefined reference`, `/usr/bin/ld:`, `The following tests FAILED:` |
| 94 | `helm_plugin` | Helm install/upgrade 输出脱水：保留关键字段和资源类型，折叠重复资源，压缩空格和丢弃非核心行。 | NAME/LAST DEPLOYED/NAMESPACE/STATUS/REVISION/TEST SUITE/Last Started/Last Completed/Phase 行、deployment/ / service/ / configmap/ / secret/ 资源行、`error:` 行 | 行内多余空格（compact_spaces）、重复的资源行（BTreeSet 去重）、非字段非资源非错误的普通输出行（丢弃） | `NAMESPACE:`, `STATUS:`, `rollback was a success`, `deployment/` |
| 200 | `maven_plugin` | Maven 构建日志脱水：保留错误/警告和构建结果，折叠下载进度行。 | `[ERROR]` / `[WARNING]` 行、BUILD SUCCESS / BUILD FAILURE、Tests run: 测试摘要 | `[INFO]` 下载进度行折叠、重复 `[INFO]` 行去重 | `[INFO] Building`, `COMPILATION ERROR`, `Results:`, `[JUNIT]` |
| 96 | `protobuf_plugin` | protoc/buf 诊断脱水：保留诊断信息，折叠重复警告，压缩文件路径。 | `error` 诊断行（保留错误位置和消息）、`warning` 诊断行（最多前 6 个）、错误计数和警告计数（汇总行）、is_error_line 匹配的行 | `.proto` 文件路径（替换为 `$PB` 令牌字典）、多余空白字符（compact_spaces）、重复的警告（>6 后丢弃）、非诊断且非错误信号的行（丢弃） | `PROTOC: 2 errors`, `Writing descriptor set`, `libprotoc` |
| 92 | `pulumi_plugin` | Pulumi 部署输出脱水：保留错误信号和资源操作摘要，折叠详细资源行为（操作类型+字典化 URN），压缩非关键行。 | 错误行（含 `error:` 的行）、Resources: 行（资源计数摘要行）、操作计数汇总（如 `A: 3 D: 1 M: 2`） | 详细资源行（+/-~ 模式，折叠为操作类型+字典短标记）、资源 URN 和类型（dict_engine.add_path_layered 令牌替换）、非资源非错误非摘要行（可能丢弃或压缩） | `current stack outputs`, `OPS: +2 ~1 -0`, `Resources:` |
| 97 | `pytest_plugin` | pytest 脱水：保留测试结果与摘要，折叠重复状态，压缩测试路径和 summary 标签。 | collected 行、测试结果行（如 `test.py::test_func PASSED`）、summary 行（如 `1 failed, 10 passed`）、测试 session 开始行 | 完整测试路径替换为 `$PY` 令牌字典、重复的测试结果状态合并计数、summary 中为零计数的状态丢弃、状态标签缩写（passed → P、failed → F）、多余空格压缩 | `Coverage failure`, `PYTEST:`, `JUNITXML:`, `reruns=2` |
| 160 | `rust_go_plugin` | Rust/Go 日志脱水：保留编译与测试关键信号，折叠重复编译行，压缩测试统计与路径。 | Compiling .. 行（折叠后保留统计行）、Finished .. 行、`test .. FAILED` 行、Warning/error/panic 行、`goroutine .. [..]:` 行、`running N tests` 行 | 重复的 Compiling 行折叠为一行计数并隐藏细节、多个测试结果汇总为统计行（仅保留失败详情）、长路径替换为字典令牌如 `$Pn`、Go 测试输出类似压缩 | `Compiling `, `panic`, `test_fail_divide_by_zero`, `=== RUN` |
| 90 | `terraform_plugin` | Terraform 脱水：保留资源变更行与错误信号，折叠路径为 `$TF` 令牌，压缩计划摘要与已知后计算行。 | 资源变更行（`# 路径 will be 动作` 格式）、错误信号行（含 error 等关键词）、计划摘要行（`Plan: X to add, Y to change, Z to destroy`）、已知后计算行（`(known after apply)`） | 长资源路径替换为 `$TF` 令牌字典、已知后计算行折叠为计数、计划摘要格式化为简洁摘要、ANSI 转义序列被清除 | `created`, `destroyed`, `terraform plan`, `(known after apply)` |
| 200 | `webpack_vite_plugin` | Webpack/Vite 构建日志脱水：保留错误和警告行，折叠资产列表，压缩噪音行。 | `ERROR in` 行、`Module parse failed` 行、`warning:` 行、`⚠️` 行 | 噪音行（is_noise_line 过滤的废话行）、长路径（可能通过字典替换） | `Cannot find module`, `compiled`, `ready in`, `Asset` |
| 180 | `xcode_log_plugin` | Xcode 构建日志脱水：保留编译链接命令骨架，折叠 `/dev/null` 探针噪音，压缩路径参数。 | CompileC 行、Linking 行、clang 行、Build succeeded/failed 行 | `/dev/null` 探针行（clang/libtool，批量折叠为 `$XC|PROBE|x`）、编译命令中的路径参数（替换为字典令牌 `$XC|C|`）、编译命令中的源文件路径（压缩为 `$XC|C|` 的一部分） | `CompileC `, `Linking `, `Using response file:`, `xcodebuild` |

---

## Runtime / trace / error plugins (7)

| Priority | Plugin | What it does | 保留信号 | 压缩目标 | Detection signals |
|---|---|---|---|---|---|
| 170 | `java_stack_plugin` | Java 异常堆栈脱水：保留关键异常类和帧，折叠重复和深层堆栈，压缩类名和路径。 | 异常行（Exception in thread / Exception / Error 等）、常见异常类名（java.lang 白名单）、Caused by 行、堆栈帧前 N 行（通过阈值） | 重复相同异常堆栈（超过阈值折叠为 `[DUPLICATE]`）、深层堆栈超出 N 的行（截断并添加摘要）、抑制异常（suppressed exceptions）压缩为简短形式、异常类名（非白名单）用字典编码为 `$JEX` 令牌、堆栈帧类名和方法用 `$JST` 令牌编码、Caused by 类名用 `$JCB` 令牌编码 | `Throwable`, `at `, `[DUPLICATE]`, `IllegalArgumentException` |
| 200 | `ndjson_plugin` | NDJSON 脱水：保留首尾行，折叠中间行，压缩测试摘要。 | 前 5 行、后 5 行 | 中间行（折叠为省略行） | `go test -json`, `2 passed`, `1 failed`, `truncated` |
| 170 | `node_error_plugin` | Node.js 错误脱水：保留异常类名与消息，折叠堆栈帧为紧凑令牌，压缩自定义类名和文件路径。 | Error / SyntaxError 等内置异常类名（白名单保留字面量）、以 Error 或 Exception 结尾的自定义类名、异常消息（msg）、堆栈帧中的函数名/行号/列号、`<anonymous>` 文件（保留字面量） | 非白名单自定义异常类名（字典化）、堆栈帧中的文件路径（字典化，除 `<anonymous>`）、缩进符（转换为数字编码） | `Error`, `Exception`, `$ND\|` |
| 180 | `nodejs_plugin` | Node.js 运行时日志脱水：保留错误和 npm 错误，折叠下载进度。 | Error / TypeError 行、`npm ERR!` 行、WARN 行 | npm 下载进度折叠、`node_modules` 路径压缩 | `[TSC]`, `problems`, `compiled`, `warnings=2` |
| 200 | `php_ruby_plugin` | PHP/Ruby 错误栈脱水：保留错误关键行，折叠 HTML 包裹。 | `Fatal error:` 行（PHP 致命错误）、`PHP Stack trace:` 行、`Uncaught Error:` 行（PHP 未捕获错误）、`ActionView::Template::Error`（Ruby 模板错误）、`.rb:` 行（Ruby 文件引用）、`rake aborted!`、HTML 错误页面标题（Whoops! / exception_title） | HTML 标签包裹（`<html>`/`<div>` 等） | `<title>Whoops!`, `Fatal error:`, `rake aborted!`, `PHP Stack trace:` |
| 160 | `python_traceback_plugin` | Python 异常脱水：保留异常类型和关键堆栈信息，折叠重复异常和深层堆栈，压缩路径和异常消息。 | `Traceback (most recent call last):` 头部、内置异常类名（Exception / ValueError 等）、错误消息行（如 `ValueError: invalid literal`）、文件路径和行号、异常链中的连接信息（直接原因、上下文等） | 重复的异常堆栈（超过阈值替换为 `[DUPLICATE]`）、深层堆栈帧（超过阈值截断为 `[...]`）、文件路径（替换为 `$PY|FL|` 令牌）、异常类型和消息（替换为 `$PY|EX|` 令牌）、链式异常计数摘要（多个异常合并为 `[CHAINED]` 计数） | `Traceback`, `line `, `[DUPLICATE]`, `Error` |

---

## Structured log plugins (8)

| Priority | Plugin | What it does | 保留信号 | 压缩目标 | Detection signals |
|---|---|---|---|---|---|
| 165 | `db_log_plugin` | 数据库日志脱水：保留进程 ID、持续时间、日志级别、查询标签等关键字段，折叠冗余信息，压缩为紧凑格式。 | pid（进程 ID）、duration（持续时间毫秒）、level（日志级别，如 LOG / ERROR）、query（压缩后的查询标签）、msg（日志消息主体） | 原始时间戳（被移除）、原始长查询（被 compact_query_label 截断或替换）、普通 duration（仅保留数字，标记 SLOW 或 DUR）、ROI 门控（若压缩后体积更大则回退原文） | `SLOW\|`, `MY\|`, `REDIS\|`, `aggregate=` |
| 200 | `kubernetes_docker_plugin` | Kubernetes/Docker 日志脱水：保留关键事件和结构，折叠容器 ID 和 Pod 元数据，压缩为令牌字典。 | K8S_POD 正则匹配的行（命名空间/Pod）、DOCKER_ID 正则匹配的容器 ID、Docker CI 输出（如 `Step X/Y`）、Kubernetes CI 输出（如 `kubectl` 命令）、JSON 对象含 `message` 或 `logGroup` | 容器 ID 替换为短令牌（`$D`）、Pod 名称/命名空间替换为令牌（`$P`/`$PK`）、JSON 结构解包（展开嵌套） | `failed`, `panic`, `"message"`, `$PK1/$P1` |
| 200 | `smart_path_plugin` | 智能路径脱水：保留路径上下文，折叠长路径为字典令牌，压缩重复路径。 | 非路径文本（路径之外的文本，原样保留） | 文件路径（替换为字典令牌） | （无 signature；走 LLM 路径） |
| 200 | `spring_boot_plugin` | Spring Boot 应用日志脱水：保留关键事件（下载、生命周期、启动），折叠 Maven 下载 URL 和 Spring 组件包名。 | `Downloaded from` 行、Maven 下载开始行、Spring Boot 行（启动标识）、Starting application 行、SPRING_LIFECYCLE_RE 匹配行 | Maven 下载 URL 替换为 `$SPRING` 令牌字典（路径折叠）、Spring 生命周期日志中的 logger 包名替换为令牌字典、Bean 初始化消息中的包名（如配置 `extract_beans_packages`）折叠为令牌 | `Downloading from`, `Spring Boot`, `Starting application` |
| 200 | `static_rule_plugin` | 静态规则驱动的通用结构化日志脱水：保留进入和保留模式匹配的行，折叠重复行和长块。 | enter 正则匹配的行（区块开始标记）、keep 正则匹配的行（重要行，检测分数 0.45）、未折叠的独立行（原始内容保留） | 连续重复的行（合并为带计数的 `{行}(x次数)`）、超过阈值长度的区块（折叠为 `$STATIC[...]` 占位符） | `[failed_tests]`, `failed_count=`, `SUMMARY failed=` |
| 130 | `syslog_plugin` | 系统日志脱水：保留时间戳和消息内容，折叠主机名和进程名为字典令牌，压缩重复标识。 | 时间戳（月 日 时:分:秒）、消息内容（msg）、PID（可选）、非 syslog 格式的行（原样保留） | 主机名（替换为 `$SYS` 格式中的字典令牌）、进程名（替换为 `$SYS` 格式中的字典令牌） | `$SYS\|` |
| 200 | `template_driven_plugin` | 模板驱动脱水：保留模板结构，压缩可变部分为字典令牌。 | 匹配模板模式的行（保留模板固定文本） | 捕获组中变量部分（压缩为 `$TEMPLA` 字典令牌） | （由模板配置触发） |
| 170 | `web_log_plugin` | Web 访问日志脱水：保留异常/慢请求信号，折叠常规记录为紧凑摘要，压缩 IP/UA/路径等冗余字段。 | `$W\|SUMMARY` 行（健康摘要）、异常行（如 4xx/5xx 错误）、慢请求行（slow lines）、原始错误日志行（error_log_pattern 匹配的） | IP 地址替换为字典令牌、URI 路径替换为分层字典令牌、User-Agent 替换为字典令牌、时间戳压缩为紧凑格式（YYYY-MM-DD HH:MM:SS）、流路径（如 `/stream/xxx`）折叠为前 8 字符、URL 编码解码（`%20` 等替换）、多个连续空格压缩为一个、重复的详细记录聚合为标准摘要 | `GET /health`, `err_rate=`, `4xx=`, `$W\|ROUTINE\|` |

---

## Structured data format plugins (10)

| Priority | Plugin | What it does | 保留信号 | 压缩目标 | Detection signals |
|---|---|---|---|---|---|
| 200 | `ansi_cleaner_plugin` | ANSI 脱水：保留文本内容，移除 ANSI 控制码，压缩进度条覆盖，丢弃空行。 | 非空文本行（保留有效内容）、进度条最后有效状态（保留最后一次更新） | ANSI 转义序列（如 `\x1b[31m`）、回车符 `\r` 导致的进度条历史记录（仅保留最后一行）、空白行（剥离后空行丢弃） | （由调用方启用） |
| 58 | `artifact_summary_plugin` | 构建产物摘要脱水：保留测试失败/错误和 SARIF 关键信号，折叠冗余的测试用例细节和 SARIF 结果条目。 | 测试套件名称（JUnit 套件标识）、测试用例名称（含类名）、测试状态（失败/错误/跳过）、SARIF 规则 ID、严重级别、源位置、工具名称 | 原始 XML/JSON 文本（替换为紧凑摘要）、长字符串（字典引擎压缩）、通过状态的测试用例（可能丢弃）、冗余的 SARIF 结果条目（聚合为摘要） | `<testcase`, `<testsuite`, `SARIF\|RULES\|`, `sarif` |
| 200 | `git_diff_plugin` | Git diff 脱水：保留 diff header 和 hunk header，折叠非关键行，压缩文件路径。 | `diff --git` 行、`--- a/` 行、`+++ b/` 行、HUNK_HEADER 行、`index` 行 | 非 header 行（如上下文内容）、文件路径简化（通过 `token_prefix`） | `diff --git`, `+++ `, `@@ `, `index ` |
| 150 | `json_plugin` | JSON 脱水：保留 JSON 结构，折叠长字符串和路径，压缩为紧凑格式并添加前缀。 | `{}` 对象结构、`[]` 数组结构、数字/布尔/null 值、短字符串（≤max_string_val_len）、键名（未启用字典化时） | 长字符串（>max_string_val_len）替换为字典令牌、键名（启用 dictionaryize_keys 时）替换为字典宏、JSON 文本压缩为单行紧凑格式、短 JSON 通过 ROI 门控避免膨胀 | `$JSON\|{`, `{` |
| 200 | `markdown_plugin` | Markdown 脱水：保留标题、链接、图片、列表等核心结构，折叠注释以压缩体积。 | `#` 标题行、`[链接]` 或 `![图片]`、`-` / `*` / `1.` 列表项 | HTML/XML 注释（`<!-- comment -->`） | `# `, `* `, `- `, `](https://` |
| 200 | `noise_filter_plugin` | 噪声过滤：检测并替换二进制数据为简短标记，保留纯文本内容。 | 纯文本行（非二进制控制字符） | 二进制数据（替换为 `[BINARY_DATA: Size=..., MD5=...]` 标记） | （由调用方启用） |
| 100 | `smart_code_plugin` | Smart code 检测与脱水：保留内置异常和关键字标识符，折叠长自定义标识符为字典令牌，压缩连续空格为长度标记。 | SyntaxError（JavaScript / Python 内置异常）、`public` 关键字、`short_id`（长度 ≤8 的标识符） | 长自定义标识符（>8 且非关键字非保留异常）压缩为 `$PK` 令牌、连续空格压缩为 `$S|长度` 标记 | `public class`, `const `, `function `, `import ` |
| 200 | `sql_plugin` | SQL 脱水：保留 SQL 语句结构，折叠超长 INSERT VALUES 内容。 | SQL 关键字（SELECT / INSERT 等）所在行、INSERT 语句的前缀部分（不被截断）、短 SQL 语句（完整保留） | 长 INSERT VALUES（超过 max_insert_values_len 时截断为占位符） | `INSERT` |
| 150 | `xml_html_plugin` | XML/HTML 脱水：保留标签结构和文本内容，折叠标签间的空白字符，压缩冗余空格。 | XML/HTML 标签（如 `<div>`）、标签属性（如 `class="example"`）、文本内容（标签之间的文字） | 标签间的空白字符（换行、缩进等） | `<html>`, `<div`, `</tag>` |
| 150 | `yaml_plugin` | YAML 脱水：保留键值结构，折叠长序列，压缩键标识符。 | 有效 YAML 的映射键值对结构（键被字典化）、序列的前 max_seq_len 个元素、YAML 解析失败时的原始文本 | 长序列截断（超过 max_seq_len 部分替换为 `$SEQ-$n` 占位符）、映射键替换为字典宏、YAML 缩进与空白压缩为紧凑格式、深度超过 max_depth 时替换为 `...depth limit...` | `$YAML\|`, `:` |

---

## Shell / utility plugins (4)

| Priority | Plugin | What it does | 保留信号 | 压缩目标 | Detection signals |
|---|---|---|---|---|---|
| 0 | `generic_text_plugin` | 通用兜底：保留原始文本内容，折叠连续空白行，压缩 ANSI 控制序列和回车重绘。 | 非空白行文本内容（保留所有有效行） | ANSI 转义序列（移除）、连续空白行（折叠为单行）、回车重绘行（只保留最后一段）、制表符（可选替换为空格）、行尾空白（可选修剪） | （无 signature；由路由兜底触发） |
| 200 | `shell_session_plugin` | Shell 会话脱水：保留命令和输出，折叠提示符和进度信息。 | 命令与输出行（未明确丢弃的文本块） | shell 提示符（如 `$ # > PS ~$`）、ANSI 转义序列、多空格（折叠为单个空格）、环境变量赋值行（env var=value）、robocopy/curl/tar 进度行 | `shell session` 行首 token |
| 200 | `unity_unreal_plugin` | Unity/Unreal 构建日志脱水：保留 Unreal 和 Unity 的日志标签，折叠连续的资源加载噪音。 | LogUObject / LogHAL / LogLinker / FAndroidApp（Unreal 引擎日志标签）、Unloading / Building AssetBundle / Shader compilation（Unity 构建消息）、Loading .uasset/.prefab/.mat（通用资源加载信息，但可能被聚合） | 连续的 Loading Object 行（聚合为一条，计数量） | `LogLinker`, `Building AssetBundle`, `Shader compilation` |
| 200 | `shell_command` (family) | 通用 shell 命令输出脱水（Linux/PowerShell/cmd/bash/zsh/fish）：保留第一个执行的命令作为绝对锚点，加 shell 风格、工作目录、环境赋值、标志、管道、重定向、引号、脚本名等。 | 第一个执行的命令、shell 风格（cmd.exe / PowerShell / sh / bash / zsh / fish）、工作目录、环境赋值、标志、管道、重定向、引号、脚本名、stdout/stderr 区分、退出码、信号/超时/取消状态、shell 原生失败类（command-not-found / permission denied / parser error 等） | 重复成功的文件操作或进度行（可聚合）、流路径（折叠为前 8 字符）、空成功输出（命令锚点 + clean/exit-zero 状态必须明确）、非零退出无输出（仍需保留失败状态） | `shell_command`, `shell`, `cmd`, `powershell`, `bash`, `zsh`, `fish` |

> **Shell 家族说明**：以上 4 条合并到 `shell_command` 族实现，按子命令分发到对应子实现（`shell_command_powershell` / `shell_command_bash` / `shell_command_zsh` / `shell_command_fish` / `shell_command_cmd`）。配置见 `config/plugins/shell_command.*.json`（如有）或 `src/plugins/shell_command/`。

---

## 路由优先级说明

- **值越小越优先**（见 [`src/core/plugin_config_loader/mod.rs`](./src/core/plugin_config_loader/mod.rs) 注释：值越小优先级越高）。
- **同 priority 多匹配**：取第一个注册；建议把窄 signature 注册在宽 signature 之前。
- **VCS 系列**：所有 VCS 插件 priority 固定为 200，由 [`config/plugins/vcs_plugin.route.json`](./config/plugins/vcs_plugin.route.json) 按子命令二次分发。
- **匹配失败**：回退到 `generic_text_plugin`（priority 0，最低兜底）。
- **启用/禁用**：每个插件的 `enabled` 字段；关闭后不会进入匹配链。

---

## 如何新增插件

1. 写 parser + rule 实现：见 [`docs/development/PLUGIN_DEVELOPMENT.md`](./docs/development/PLUGIN_DEVELOPMENT.md)
2. 在 `config/plugins/` 新增 `<plugin_name>.json`（`name` / `description` / `priority` / `enabled` / `detect` / `compress`）
3. 准备 sample case：见 [`docs/development/PLUGIN_DEVELOPMENT.md` §3](./docs/development/PLUGIN_DEVELOPMENT.md)
4. 跑测试 + 审计：见 [`docs/development/TESTING.md`](./docs/development/TESTING.md)
5. 在本表追加一行（保持 **Priority / Plugin / What it does / Commands/Coverage / 保留信号 / 压缩目标 / Detection signals** 七列结构；VCS 插件必须填 Commands/Coverage 列）
