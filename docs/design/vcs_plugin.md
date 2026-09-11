# VCS Plugin 设计说明

## 目标

新增统一 `vcs_plugin`，用同一套语义处理多种版本管理工具输出，减少“按命令硬编码分支”带来的维护成本。

当前覆盖的 VCS 与 `workspace` 诊断能力对齐：

- git
- svn
- hg (Mercurial)
- p4 (Perforce)
- cvs
- bzr (Bazaar)
- fossil
- darcs

## 设计原则

1. **代码内核保证语义正确性**：检测、路径识别、回退策略在代码内。
2. **配置控制压缩策略**：空白压缩、路径字典化等策略通过 `VcsConfig` 控制。
3. **并存迁移**：与 `git_diff_plugin` 并存，`git_diff_plugin` 保留为 patch 场景兜底。

## 常用命令白名单

每种 VCS 维护 10+ 条常用命令，用于检测与语义信号增强：

- git: status/diff/log/show/branch/checkout/switch/merge/rebase/reset/stash/fetch/pull/push/remote
- svn: status/diff/log/info/add/delete/move/copy/commit/update/checkout/revert
- hg: status/diff/log/summary/add/remove/rename/commit/update/branch/pull/push
- p4: opened/changes/describe/diff/submit/sync/edit/add/delete/revert/integrate/resolve
- cvs: status/diff/log/add/remove/commit/update/checkout/tag/annotate
- bzr: status/diff/log/add/remove/commit/update/branch/pull/push
- fossil: status/diff/timeline/changes/add/rm/commit/update/sync/checkout
- darcs: whatsnew/diff/changes/record/pull/push/rebase/add/remove/revert

## 配置驱动（无需重新编译）

`vcs_plugin` 启动时会尝试加载外部配置并覆盖默认规则。

查找顺序：

1. `config/vcs_plugin.json` 或 `config/vcs_plugin.toml`（仓库级）
2. `.tokenslim/vcs_plugin.json` 或 `.tokenslim/vcs_plugin.toml`（本地级，覆盖仓库级）
3. 环境变量 `TOKENSLIM_VCS_CONFIG` 指向的文件（最高优先级）

同一层如果同时存在 json/toml，会按先 json 后 toml 取第一份可解析配置。

支持字段：

- `dictionaryize_paths`
- `compact_leading_ws`
- `collapse_blank_lines`
- `max_blank_lines`
- `command_whitelists`（按 VCS 工具覆盖命令白名单）
- `signatures`（按 VCS 工具覆盖语义签名）
- `replace_command_whitelists`（先清空默认命令白名单再应用覆盖）
- `replace_signatures`（先清空默认签名再应用覆盖）

示例（JSON）：

```json
{
  "compact_leading_ws": false,
  "command_whitelists": {
    "git": ["status", "diff", "log", "rev-parse"]
  },
  "signatures": {
    "git": ["on branch", "custom git signature"]
  }
}
```

## 从日志自动生成配置

仓库内提供离线脚本：

`python scripts/generate_vcs_config.py --input <log1> --input <log2> --output config/vcs_plugin.json`

默认输出 `config/vcs_plugin.json`，可配合 `--replace` 生成严格覆盖策略。

推荐做法：

- 样本日志放在 `samples/vcs/`（仓库内已提供 status/diff/log/changes/timeline/describe 多命令示例）。
- 团队可先复制 `config/vcs_plugin.example.json` 为 `config/vcs_plugin.json` 再微调。
- 或直接基于样本生成：

`python scripts/generate_vcs_config.py --input samples/vcs/git_status.log --input samples/vcs/git_diff.log --input samples/vcs/git_log.log --input samples/vcs/svn_status.log --input samples/vcs/svn_diff.log --input samples/vcs/hg_status.log --input samples/vcs/hg_diff.log --input samples/vcs/p4_opened.log --input samples/vcs/p4_describe.log --input samples/vcs/cvs_update.log --input samples/vcs/cvs_log.log --input samples/vcs/bzr_status.log --input samples/vcs/bzr_log.log --input samples/vcs/fossil_status.log --input samples/vcs/fossil_timeline.log --input samples/vcs/darcs_whatsnew.log --input samples/vcs/darcs_changes.log --output config/vcs_plugin.json`

## 与 git_diff_plugin 的关系

- `vcs_plugin` 优先级高于 `git_diff_plugin`，用于通用 VCS 输出。
- 对标准 `diff --git` patch 文本，`vcs_plugin` 会降低 detect 分值，优先让 `git_diff_plugin` 处理。
- dispatcher fallback 设置为 `git_diff`，当无匹配时保留兜底。

## `$P` 路径 token 边界契约（必须遵守）

为避免不同模块对 `$P` token 解释不一致，路径字典相关逻辑统一走共享 helper：

- `src/core/path_optimizer/token_boundary.rs`

### 统一规则

当识别到 `$P<digits>` 后，**仅当后一个字符不是** `[A-Za-z0-9_-]` 时，才视为有效 token 边界。

换句话说：

- `"$P1/foo.rs"` ✅ 视为 token 引用
- `"$P1-notes/readme.md"` ❌ 不视为 token 引用（属于字面路径片段）
- `"$P1abc"` ❌ 不视为 token 引用

### 为什么这样设计

1. 防止把真实路径中的字面片段（例如 `docs/$P1-notes/...`）误替换成字典路径，造成语义漂移。
2. 保证 `vcs_plugin` 与 `path_optimizer` 在 footer 合并、dead-anchor 扁平化、token 计数上的行为一致。
3. 为后续 SVN/HG/P4 adapter 扩展提供稳定语义基线，避免每个 adapter 各自实现边界判断。

### 实施约束

- `vcs_plugin` 与 `path_optimizer` **不得**再各自维护独立 token 边界实现。
- 新增 VCS adapter（Git/SVN/HG/P4/…）时，如涉及 `$P` token 匹配/替换，必须复用该共享 helper。
- 若将来调整 token 语义，只允许在 `token_boundary.rs` 单点修改，并补齐回归测试。

## Git 高频命令+参数 → IR 路径矩阵（当前实现）

下表记录当前 `vcs_plugin` 在 AI compact 场景下，Git 高频命令与参数形态如何分流到 IR Parser/RuleEngine，以及对应回归测试。

| 命令 | 高频参数/输出形态 | 走哪条 IR 路径 | 关键检测/分流函数 | 回归测试 |
|---|---|---|---|---|
| `git status` | 标准 status（`On branch`/`Changes not staged`/`Untracked files`） | `GitStatusParser -> VcsRuleEngine` | `is_git_status_block` / `is_git_status_fragment` | `git_status_parser_and_rule_engine_keep_sections`, `git_status_is_compact_in_ai_mode` |
| `git status -s` | short status（`M path` / `?? path`） | `GitStatusParser -> VcsRuleEngine` | `is_git_status_fragment`（`SHORT_STATUS_RE`） | `detect_git_short_status_fragment`, `git_status_is_compact_in_ai_mode` |
| `git log` | 标准 log（commit/author/date/subject） | `GitLogParser -> VcsRuleEngine` | `is_git_log_block` | `git_log_parser_and_rule_engine_keep_date_normalized`, `git_log_is_compact_in_ai_mode_with_author_date_and_subject` |
| `git log --name-only` | log + 文件路径列表 | `GitLogParser -> VcsRuleEngine` | `is_git_log_block` + `looks_like_vcs_path` | `git_log_name_only_paths_are_dictionaryized_in_ai_mode` |
| `git log --oneline --name-only` | oneline 头 + 文件路径列表 | `GitLogParser -> VcsRuleEngine` | `is_git_oneline_header` + `looks_like_vcs_path` | `git_log_oneline_name_only_keeps_headline` |
| `git diff` | patch/hunk（`diff --git`/`@@`/`+++`/`---`） | `GitDiffParser -> VcsRuleEngine` | `is_git_diff_block` | `git_diff_parser_and_rule_engine_keep_patch_structure` |
| `git diff --name-only` | 仅文件列表 | `GitDiffParser -> VcsRuleEngine` | `is_git_name_only_or_status_block`（被 `is_git_diff_block` 复用） | `git_diff_name_only_and_name_status_use_ir_in_ai_diff_profile` |
| `git diff --name-status` | 状态+路径（`M path`/`A path`） | `GitDiffParser -> VcsRuleEngine` | `looks_like_git_name_status_line`（被 `is_git_name_only_or_status_block` 复用） | `git_diff_name_only_and_name_status_use_ir_in_ai_diff_profile` |
| `git show` | commit + patch/stat 混合 | `GitShowParser -> VcsRuleEngine` | `is_git_show_block`（优先于 diff 分支） | `git_show_patch_uses_ir_compaction_in_ai_diff_profile`, `git_show_parser_and_rule_engine_keep_commit_and_stat` |
| `git show --name-only` | commit + 文件列表 | `GitShowParser -> VcsRuleEngine` | `is_git_show_block`（commit + 路径命中） | `git_show_name_only_routes_to_show_ir_in_diff_profile` |
| `git diff/show` | 带空格路径 header（`"a/foo bar.rs" "b/foo bar.rs"`） | `GitDiffParser/GitShowParser -> VcsRuleEngine` | `parse_diff_git_header`（quoted token 解析） | `git_diff_parser_handles_quoted_diff_header_paths_with_spaces` |

### patch 语义保真约束（Git diff/show）

- patch payload 行（`+`/`-`/` `，但不含 `+++`/`---`/`@@`/`diff --`）禁止空白折叠与路径改写。
- `VcsRuleEngine::normalize` 不再对 `VcsRecord::Patch` 做 `trim_end`。
- 目标：尽量保持 patch 语义，不因 AI 压缩破坏空白敏感内容。

对应回归：

- `compact_ws_preserves_non_python_diff_indentation_in_patch_payload`
- `compact_ws_preserves_python_diff_indentation`
- `git_diff_parser_keeps_blank_context_and_space_only_patch_lines`

## 非 Git 适配器扩展矩阵模板（SVN/HG/P4）

> 目的：后续将 SVN/HG/P4 的高频命令与参数统一纳入 `IR -> Parser -> RuleEngine` 流程时，保持与 Git 相同的可验证性。

### 填写规范

- 每条命令形态必须有明确“检测/分流函数”。
- 每行至少绑定 1 个回归测试函数名（先写 planned 名称，落地后替换为真实函数）。
- 涉及 patch/hunk 的命令，必须注明是否复用 Git 的 patch 保真约束。

### SVN（26 命令，13 已接入 IR）

| 命令 | 高频参数/输出形态 | 走哪条 IR 路径 | 关键检测/分流函数 | 回归测试 |
|---|---|---|---|---|
| `svn status` | `M path` / `A path` / `? path` | `SvnStatusParser -> VcsRuleEngine` | `is_svn_status_block` / `classify_tool` | `svn_status_parser_and_rule_engine_keep_file_status`, `svn_status_uses_generic_ai_compact_with_paths_dictionary` |
| `svn diff` | patch/hunk（`Index:`/`@@`/`+++`/`---`） | `SvnDiffParser -> VcsRuleEngine` | `is_svn_diff_block` | `svn_diff_parser_and_rule_engine_keep_patch_structure`, `svn_diff_uses_ir_compaction_in_ai_diff_profile` |
| `svn log` | revision/author/date/message（`r123 \| author \| date`） | `SvnLogParser -> VcsRuleEngine` | `is_svn_log_block` | `svn_log_parser_and_rule_engine_keep_revision_author_date` |
| `svn blame` | `rev author line` 逐行标注 | `SvnBlameParser -> VcsRuleEngine` | `is_svn_blame_block` | `svn_blame_parser_and_rule_engine_keep_revision_and_author` |
| `svn list` | 文件/目录列表（可选 size/date） | `SvnListParser -> VcsRuleEngine` | `is_svn_list_block` | `svn_list_parser_and_rule_engine_keep_paths` |
| `svn propget` / `svn proplist` | 属性名/值对 | `SvnPropParser -> VcsRuleEngine` | `is_svn_prop_block` | `svn_prop_parser_and_rule_engine_keep_properties` |
| `svn info` | key-value 元数据 | `SvnInfoParser -> VcsRuleEngine` | `is_svn_info_block` | `svn_info_parser_and_rule_engine_keep_metadata` |
| `svn blame/list/prop/info`（AI compact） | 紧凑输出 | 各自 `compact_svn_*_for_ai` | 各自 `is_svn_*_block` | `svn_blame_uses_ir_compaction_in_ai_other_profile` 等 4 个 |
| 其他 svn 命令 | 通用文本 | `ai-compact-generic` | `classify_tool` | 通过 `detect_all_doctor_vcs_signatures` 覆盖 |

**未接入 IR 的 SVN 命令**（通过 `ai-compact-generic` 回退）：`add`、`delete`、`move`、`copy`、`commit`、`update`、`checkout`、`revert`、`merge`、`switch`、`resolve`、`cleanup`、`mkdir`、`export`、`import`、`lock`、`unlock`

### HG / Mercurial（28 命令，11 已接入 IR）

| 命令 | 高频参数/输出形态 | 走哪条 IR 路径 | 关键检测/分流函数 | 回归测试 |
|---|---|---|---|---|
| `hg status` | `M path` / `? path` | `HgStatusParser -> VcsRuleEngine` | `is_hg_status_block` / `classify_tool` | `hg_status_and_diff_parsers_parse_common_lines`, `hg_status_uses_ir_compact_with_paths_dictionary` |
| `hg diff` | patch/hunk（`diff -r`/`@@`） | `HgDiffParser -> VcsRuleEngine` | `is_hg_diff_block` | `hg_status_and_diff_parsers_parse_common_lines` |
| `hg log` | changeset/user/date/summary | `HgLogParser -> VcsRuleEngine` | `is_hg_log_block` | `hg_log_parser_and_rule_engine_keep_metadata`, `hg_log_uses_ir_compaction_in_ai_log_profile` |
| `hg heads` | 各分支最新 changeset | `HgHeadsParser -> VcsRuleEngine` | `is_hg_heads_block` | `hg_heads_parser_and_rule_engine_keep_changesets` |
| `hg outgoing` | 未推送 changeset 列表 | `HgOutgoingParser -> VcsRuleEngine` | `is_hg_outgoing_block` | `hg_outgoing_parser_and_rule_engine_keep_changesets` |
| `hg incoming` | 未拉取 changeset 列表 | `HgIncomingParser -> VcsRuleEngine` | `is_hg_incoming_block` | `hg_incoming_parser_and_rule_engine_keep_changesets` |
| `hg parents` | 当前工作目录父 changeset | `HgParentsParser -> VcsRuleEngine` | `is_hg_parents_block` | `hg_parents_parser_and_rule_engine_keep_metadata` |
| `hg heads/outgoing/incoming/parents`（AI compact） | 紧凑 changeset | `compact_hg_log_for_ai`（复用 log 管线） | 各自 `is_hg_*_block` | `hg_heads_uses_ir_compaction_in_ai_log_profile` 等 4 个 |
| 其他 hg 命令 | 通用文本 | `ai-compact-generic` | `classify_tool` | 通过 `detect_all_doctor_vcs_signatures` 覆盖 |

**未接入 IR 的 HG 命令**（通过 `ai-compact-generic` 回退）：`summary`、`add`、`remove`、`rename`、`commit`、`update`、`branch`、`pull`、`push`、`annotate`、`graft`、`rebase`、`shelve`、`unshelve`、`revert`、`cat`、`backout`、`uncommit`、`forget`、`tip`、`tags`

### P4 / Perforce（26 命令，8 已接入 IR）

| 命令 | 高频参数/输出形态 | 走哪条 IR 路径 | 关键检测/分流函数 | 回归测试 |
|---|---|---|---|---|
| `p4 opened` | `... //depot/...#rev action` | `P4OpenedParser -> VcsRuleEngine` | `is_p4_opened_block` / `classify_tool` + `P4_PATH_RE` | `p4_opened_parser_and_rule_engine_keep_depot_paths`, `p4_opened_uses_ir_compact_with_paths_dictionary` |
| `p4 describe` | Change + affected files + diff | `P4DescribeParser -> VcsRuleEngine` | `is_p4_describe_block` | `p4_describe_parser_and_rule_engine_keep_files_and_diff` |
| `p4 changes` | change list summary（`Change N on ... by ...`） | `P4ChangesParser -> VcsRuleEngine` | `is_p4_changes_block` | `p4_changes_parser_and_rule_engine_keep_change_summary`, `p4_changes_uses_ir_compaction_in_ai_log_profile` |
| `p4 fstat` | `... depotFile`, `... headChange`, `... action` | `P4FstatParser -> VcsRuleEngine` | `is_p4_fstat_block` | `p4_fstat_parser_and_rule_engine_keep_depot_info` |
| `p4 where` | depot → client → local 路径映射 | `P4WhereParser -> VcsRuleEngine` | `is_p4_where_block` | `p4_where_parser_and_rule_engine_keep_mappings` |
| `p4 info` | server 元数据 key-value | `P4InfoParser -> VcsRuleEngine` | `is_p4_info_block` | `p4_info_parser_and_rule_engine_keep_server_info` |
| `p4 labels` | label 列表（name/date/owner） | `P4LabelsParser -> VcsRuleEngine` | `is_p4_labels_block` | `p4_labels_parser_and_rule_engine_keep_labels` |
| `p4 dirs` | depot 目录列表 | `P4DirsParser -> VcsRuleEngine` | `is_p4_dirs_block` | `p4_dirs_parser_and_rule_engine_keep_directories` |
| 其他 p4 命令 | 通用文本 | `ai-compact-generic` | `classify_tool` | 通过 `detect_all_doctor_vcs_signatures` 覆盖 |

**未接入 IR 的 P4 命令**（通过 `ai-compact-generic` 回退）：`diff`、`submit`、`sync`、`edit`、`add`、`delete`、`revert`、`integrate`、`resolve`、`reconcile`、`shelve`、`unshelve`、`files`、`client`、`print`、`have`、`label`、`users`

### 扩展完成定义（DoD）

1. 模板中的 `TODO` 被真实 parser/函数/测试名替换。
2. 每个 VCS 至少覆盖 status/diff/log（或等价高频命令）3 条。
3. 新增命令在 AI compact 下必须可回退（parser miss 时返回 raw）。
4. 文档矩阵与测试函数名保持一一对应，不允许“文档有、测试无”。

## 未来演进

1. 将命令白名单外置为配置文件（无需重新编译即可扩展）。
2. 增加 tool/command 级别语义统计并输出到 metadata。
3. 为 svn/hg/p4 补充更细粒度的结构化 token（类似 git hunk token）。
