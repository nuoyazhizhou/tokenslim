# Sample Case Quality Report - vcs_git_plugin

- generated_at: 2026-06-24T17:49:20.087199
- version: 20260624-174915
- physical_samples: 88
- registered_in_showcase: 88
- duplicate_groups: 3
- llm_invoked: 0 / 88

## Status Distribution

| status | count | human-readable meaning |
| --- | ---: | --- |
| valid | 85 | case 与 showcase title 一致，无 lint/LLM 阻断，可直接进 case_fixtures；含 dispatch_chain_confirmed 时表示二段路由已确认 |
| duplicate | 3 | 与同 plugin 另一 case 内容重复 |

## Realism Audit Summary

- llm_invoked: 0（默认 lint-only 模式，未调 LLM 真实性仲裁）
- 提示：`--llm-audit` 启用 LLM 仲裁；无 API key 时加 `--allow-llm-missing`
  降级到 lint-only；硬要求时用 `--require-llm-audit`。

## Command Family Coverage

- target_families: 0
- covered: 0
- missing: 0

### Covered families
- None

### Missing families (target_high_frequency ∖ current_coverage)
- None

## Duplicate Groups

- primary: `case_247_git_status_porcelain`  →  duplicates: `case_248_git_status_short`
- primary: `case_249_git_status_long`  →  duplicates: `case_250_git_status_ignored`
- primary: `case_258_git_diff_head`  →  duplicates: `case_260_git_diff_word_diff`

## Recommended Missing Cases

All target families are already covered.
## Per-Case Detail

| case_id | status | registered | title_consistent | first_token | mojibake | secrets | routing_tool | dispatch_chain | sidecar_mismatch |
| --- | --- | :-: | :-: | --- | :-: | :-: | --- | --- | --- |
| case_081_merge_conflict | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_082_rebase_interactive | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_083_log_graph | valid | Y | ? | `-` | N | N | `-` |`-` |keep:提交哈希与提交信息（如 abc1234 Merge branch 'feature compress:图形字符（如 * |\ |
| case_084_reflog_long | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_128_git_remote | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_129_git_branch | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_130_git_tag_list | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_131_git_stash_list | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_163_git_blame | valid | Y | ? | `-` | N | N | `-` |`-` |keep:提交哈希 (8b3c2d1e,时间戳 (2026-04-03 14:32:11) |
| case_164_git_rebase_i | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_165_git_worktree | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_166_git_reflog | valid | Y | ? | `-` | N | N | `-` |`-` |compress:无（条目数未超20,无需折叠） |
| case_167_git_grep | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_168_git_log_oneline | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_169_git_diff_stat | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_170_git_shortlog | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_237_git_log_oneline_short | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_238_git_log_graph | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_239_git_bisect_bad | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_23_git_status | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_240_git_bisect_good | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_241_git_clean_fd | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_246_git_status_branch | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_247_git_status_porcelain | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_248_git_status_short | duplicate | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_249_git_status_long | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_24_git_diff | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_250_git_status_ignored | duplicate | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_251_git_status_untracked_all | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_252_git_log_oneline_n20 | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_253_git_log_stat | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_254_git_log_patch | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_255_git_log_graph | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_256_git_log_all | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_257_git_diff_cached | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_258_git_diff_head | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_259_git_diff_stat | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_25_git_log | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_260_git_diff_word_diff | duplicate | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_261_git_diff_branches | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_26_git_show | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_283_git_diff_name_only | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_284_git_diff_name_status | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_285_git_stash_show | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_286_git_stash_pop | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_287_git_branch_v | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_288_git_branch_d | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_289_git_branch_D | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_290_git_add_p | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_291_git_reset_hard | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_292_git_reset_soft | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_293_git_reset_mixed | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_294_git_checkout_b | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_295_git_cherry_pick_continue | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_296_git_cherry_pick_abort | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_297_git_rebase_continue | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_298_git_rebase_abort | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_299_git_rebase_skip | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_300_git_merge_abort | valid | Y | ? | `-` | N | N | `-` |`-` |keep:无输出 compress:无输出 |
| case_30_git_checkout | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_31_git_checkout_file | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_323_git_log_parentheses | valid | Y | ? | `-` | N | N | `-` |`-` |compress:无明显的压缩目标,但提交信息中的括号内容可能被保留或压缩 |
| case_324_git_log_oneline_full_hash_collision | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_325_git_reflog_with_hash_collision | valid | Y | ? | `-` | N | N | `-` |`-` |compress:无（reflog 条目少于 20 条,全部保留） |
| case_326_git_log_oneline_heavy_collision | valid | Y | ? | `-` | N | N | `-` |`-` |keep:提交哈希（前10位）和提交信息 compress:重复的'f'字符（哈希后30位） |
| case_327_git_blame_hash_collision | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_328_git_show_multi_commit_collision | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_329_git_help | valid | Y | ? | `-` | N | N | `-` |`-` |compress:无（帮助信息通常完整保留） |
| case_32_git_stash | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_330_git_status_unmerged | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_33_git_fetch | valid | Y | ? | `-` | N | N | `-` |`-` |keep:分支更新行（如 main -> origin compress:进度行（如 Enumerating objects: 25 |
| case_34_git_push | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_44_git_merge | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_45_git_rebase | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_46_git_reset | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_47_git_cherry_pick | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_48_git_revert | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_49_git_pull | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_50_git_add | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_51_git_rm | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_52_git_restore | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_53_git_switch | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_54_git_tag | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_55_git_bisect | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_56_git_clean | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_57_git_submodule | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_tree_diff_name_only | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |
| case_tree_status | valid | Y | ? | `-` | N | N | `-` |`-` |✓ |

## Hard-Block List (must fix before `audit_case_metrics.py`)

| case_id | status | explanation | fix_hint |
| --- | --- | --- | --- |
| case_248_git_status_short | duplicate | 与 `case_247_git_status_porcelain` 内容重复，应合并或删除次要副本 | 删除本 case，或在 showcase 中合并到 `case_247_git_status_porcelain` |
| case_250_git_status_ignored | duplicate | 与 `case_249_git_status_long` 内容重复，应合并或删除次要副本 | 删除本 case，或在 showcase 中合并到 `case_249_git_status_long` |
| case_260_git_diff_word_diff | duplicate | 与 `case_258_git_diff_head` 内容重复，应合并或删除次要副本 | 删除本 case，或在 showcase 中合并到 `case_258_git_diff_head` |

## Sidecar Field Mismatch (expected_keep / expected_compress not in case content)

**9 case(s)** with mismatched sidecar fields:

| case_id | keep_mismatch | compress_mismatch |
| --- | --- | --- |
| case_083_log_graph | `提交哈希与提交信息（如 abc1234 Merge branch 'feature` | `图形字符（如 * |\` |
| case_163_git_blame | `提交哈希 (8b3c2d1e`, `时间戳 (2026-04-03 14:32:11)` | - |
| case_166_git_reflog | - | `无（条目数未超20`, `无需折叠）` |
| case_300_git_merge_abort | `无输出` | `无输出` |
| case_323_git_log_parentheses | - | `无明显的压缩目标`, `但提交信息中的括号内容可能被保留或压缩` |
| case_325_git_reflog_with_hash_collision | - | `无（reflog 条目少于 20 条`, `全部保留）` |
| case_326_git_log_oneline_heavy_collision | `提交哈希（前10位）和提交信息` | `重复的'f'字符（哈希后30位）` |
| case_329_git_help | - | `无（帮助信息通常完整保留）` |
| case_33_git_fetch | `分支更新行（如 main -> origin` | `进度行（如 Enumerating objects: 25` |

## Sidecar Semantic Accuracy (LLM judgment)

None — no LLM-audited sidecar inaccuracies found ✓

## Output Artifacts

- `docs\audit\vcs_git_plugin\sample_quality\case_quality_report.json`
- `docs\audit\vcs_git_plugin\sample_quality\case_quality_report.md`
- `docs\audit\vcs_git_plugin\sample_quality\case_quality_latest.json`
- `docs\audit\vcs_git_plugin\sample_quality\command_family_coverage.json`
- `docs\audit\vcs_git_plugin\sample_quality\cases/<case_id>/quality.json`
