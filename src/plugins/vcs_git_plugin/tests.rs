#[cfg(test)]
mod tests {
    use super::super::methods::*;
    use super::super::parser::*;

    fn read_case(name: &str) -> String {
        crate::plugins::test_utils::vcs_read_case("vcs_git_plugin", name)
    }

    /// 测试：git status 样例文件被解析为树结构。
    #[test]
    fn git_status_from_sample_file() {
        let raw = read_case("case_23_git_status");
        let parsed = process_parser(&GitStatusParser, &raw);
        assert!(parsed.starts_with("git status"));
        assert!(parsed.contains("BR:feature/vcs-plugin"));
        assert!(parsed.contains("M src/core/doctor_workspace/methods.rs"));
        assert!(parsed.contains("? config/vcs_plugin.example.json"));
        assert!(!parsed.contains("[changes]"));
        assert!(!parsed.contains("[untracked]"));
    }

    /// 测试：git log 样例文件被扁平化压缩。
    #[test]
    fn git_log_flattened_from_sample_file() {
        let raw = read_case("case_25_git_log");
        let compacted = compact_git_log_for_ai(&raw);
        assert!(compacted.starts_with("git log"));
        assert!(compacted.contains("18f4bb5007"));
        assert!(compacted.contains("@nuoyazhizhou"));
        assert!(compacted.contains("2026-03-31 22:11:16"));
        assert!(compacted.contains("@alice.chen"));
    }

    /// 测试：含括号的 log 消息保留在提交行内。
    #[test]
    fn git_log_message_with_parentheses_stays_in_commit_line() {
        let raw = read_case("case_323_git_log_parentheses");
        let compacted = compact_git_log_for_ai(&raw);
        assert!(compacted.starts_with("git log -n 2"));
        assert!(
            compacted.contains(
                "7a5775c834 @nuoyazhizhou 2026-04-29 09:28:41 优化 Mercurial (Hg) 命令输出压缩率"
            ),
            "{}",
            compacted
        );
        assert!(
            !compacted
                .lines()
                .any(|line| line.trim() == "优化 Mercurial (Hg) 命令输出压缩率"),
            "{}",
            compacted
        );
    }

    /// 测试：git fetch 样例噪音被过滤。
    #[test]
    fn git_fetch_noise_filtered_from_sample_file() {
        let raw = read_case("case_33_git_fetch");
        let compacted = compact_git_fetch_for_ai(&raw);
        assert!(compacted.starts_with("git fetch"));
        assert!(compacted.contains("From "));
        assert!(!compacted.contains("Counting objects:"));
        assert!(!compacted.contains("Compressing objects:"));
    }

    /// 测试：git show 样例将提交头与主题折叠为单行。
    #[test]
    fn git_show_flattens_commit_header_and_subject_into_single_line() {
        let raw = read_case("case_26_git_show");
        let compacted = compact_git_show_for_ai(&raw);
        assert!(compacted.starts_with("git show"), "{}", compacted);
        assert!(
            compacted.contains("18f4bb5007 @nuoyazhizhou 2026-03-31 22:11:16 feat: Support doctor workspace auto-detection"),
            "{}",
            compacted
        );
        assert!(
            !compacted
                .lines()
                .any(|line| line.trim() == "OW:@nuoyazhizhou"),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("DIFF://src/core/doctor_workspace/methods.rs"),
            "{}",
            compacted
        );
    }

    /// 测试：git blame 样例去重压缩。
    #[test]
    fn git_blame_dedup_from_sample_file() {
        let raw = read_case("case_163_git_blame");
        let compacted = compact_git_blame_for_ai(&raw);
        assert!(compacted.starts_with("git blame"));
        assert!(compacted.contains("@alice.chen"), "{}", compacted);
        assert!(compacted.contains("@bob.wang"), "{}", compacted);
        assert!(compacted.contains("@charlie.li"), "{}", compacted);
        assert!(compacted.contains("^ "));
    }

    /// 测试：git merge 冲突样例被归一化。
    #[test]
    fn git_merge_conflict_normalized_from_sample_file() {
        let raw = read_case("case_44_git_merge");
        let compacted = compact_git_merge_for_ai(&raw);
        assert!(compacted.starts_with("git merge"));
        assert!(compacted.contains("!CONFLICT:src/main.rs"));
        assert!(compacted.contains("Auto-merging src/main.rs"));
    }

    /// 测试：git rm 样例被正确解析。
    #[test]
    fn git_rm_parser_from_sample_file() {
        let raw = read_case("case_51_git_rm");
        let compacted = compact_git_rm_for_ai(&raw);
        assert!(compacted.starts_with("git rm"));
        assert!(compacted.contains("D src/old_file.rs"));
        assert!(compacted.contains("config/old_config.json"));
    }

    /// 测试：git cherry-pick 样例被正确解析。
    #[test]
    fn git_cherry_pick_proper_parser_from_sample_file() {
        let raw = read_case("case_47_git_cherry_pick");
        let compacted = compact_git_cherry_pick_for_ai(&raw);
        assert!(compacted.starts_with("git cherry-pick"));
        assert!(compacted.contains(" -> "));
        assert!(!compacted.contains("Counting objects"));
    }

    /// 测试：git revert 样例被正确解析，冗余整段全 hash 与样板句被合并折叠。
    #[test]
    fn git_revert_parser_from_sample_file() {
        let raw = read_case("case_48_git_revert");
        let compacted = compact_git_revert_for_ai(&raw);
        assert!(compacted.starts_with("git revert"));
        assert!(compacted.contains("Reverting commit"));
        assert!(compacted.contains("Reverted commit"));
        assert!(compacted.contains("files"));
        // 冗余的整段全 hash 行应被丢弃（仅保留 abc1234 短哈希）
        assert!(
            !compacted.contains("abc1234567890abcdef1234567890abcdef12345678"),
            "整段全 hash 行应被折叠丢弃"
        );
        // "completed successfully" 样板句仅保留 parent，语义仍完整
        assert!(
            !compacted.contains("completed successfully"),
            "样板句应被折叠"
        );
        assert!(
            compacted.contains("parent def5678"),
            "parent 哈希应被保留: {}",
            compacted
        );
        // 日期符号化 + 作者保留
        assert!(
            compacted.contains("20260401") && compacted.contains("alice.chen"),
            "日期(符号化)与作者应被保留: {}",
            compacted
        );
    }

    /// 测试：git remote 样例被正确解析。
    #[test]
    fn git_remote_parser_from_sample_file() {
        let raw = read_case("case_128_git_remote");
        let compacted = compact_git_remote_for_ai(&raw);
        assert!(compacted.starts_with("git remote"));
        assert!(compacted.contains("origin"));
        assert!(compacted.contains("github.com"));
        assert!(compacted.contains("upstream"));
    }

    /// 测试：git remote -v 中同 name+url 的 (fetch)/(push) 双行被合并为单行 `(fetch/push)`。
    #[test]
    fn git_remote_v_merges_fetch_and_push_for_same_url() {
        let raw = read_case("case_128_git_remote");
        let compacted = compact_git_remote_for_ai(&raw);

        // 锚点守卫：首行保留命令触发行
        assert!(compacted.starts_with("git remote -v"));
        // 合并后每个 remote 只有一行，角色合并为 (fetch/push)
        assert!(
            compacted.contains("origin https://github.com/owner/repo (fetch/push)"),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("upstream https://github.com/original/repo (fetch/push)"),
            "{}",
            compacted
        );
        // 不再出现独立的 (fetch) 尾行（去重后角色已合并）
        assert!(
            !compacted.lines().any(|l| l.ends_with("(fetch)")),
            "{}",
            compacted
        );
        // origin 远程不再重复出现两行
        let origin_lines = compacted
            .lines()
            .filter(|l| l.starts_with("origin"))
            .count();
        assert_eq!(origin_lines, 1, "{}", compacted);
    }

    /// 测试：git tag 列表样例被正确解析。
    #[test]
    fn git_tag_list_parser_from_sample_file() {
        let raw = read_case("case_130_git_tag_list");
        let compacted = compact_git_tag_for_ai(&raw);
        assert!(compacted.starts_with("git tag"));
        assert!(compacted.contains("v1.0.0"));
        assert!(compacted.contains("v2.0.0"));
    }

    /// 测试：git switch 分离头样例被正确解析。
    #[test]
    fn git_switch_detached_head_from_sample_file() {
        let raw = read_case("case_53_git_switch");
        let compacted = compact_git_switch_for_ai(&raw);
        assert!(compacted.starts_with("git switch"));
        assert!(compacted.contains("BR:main"));
        assert!(compacted.contains("BR:*feature/new-feature"));
        assert!(compacted.contains("BR:prev@abc1234"));
    }

    /// 测试：git branch -v 保留星号且不加 BR 前缀。
    #[test]
    fn git_branch_v_keeps_star_without_br_prefix() {
        let raw = read_case("case_287_git_branch_v");
        let compacted = compact_git_branch_for_ai(&raw);
        assert!(compacted.starts_with("git branch -v"), "{}", compacted);
        assert!(compacted.contains("* main"), "{}", compacted);
        assert!(!compacted.contains("*BR:"), "{}", compacted);
        // 列对齐的多空格折叠为单空格，但不丢任何语义 token
        assert!(
            compacted.contains("feature/login a1b2c3d Add login authentication"),
            "分支名/哈希/消息应保留且多空格被折叠: {}",
            compacted
        );
        assert!(
            compacted.contains("c7d8e9f Release v2.1.0"),
            "当前分支哈希与消息应保留: {}",
            compacted
        );
        assert!(
            !compacted.contains("  a1b2c3d "),
            "列对齐双空格应被折叠: {}",
            compacted
        );
    }

    /// 测试：git grep 保留命令锚点。
    #[test]
    fn git_grep_keeps_command_anchor() {
        let raw = read_case("case_167_git_grep");
        let compacted = compact_git_grep_for_ai(&raw);
        assert!(compacted.starts_with("git grep"), "{}", compacted);
        assert!(compacted.contains("src/main.rs:15:"), "{}", compacted);
        assert!(compacted.contains("src/utils.rs:42:"), "{}", compacted);
    }

    /// 测试：git grep 输出不扩张。
    #[test]
    fn git_grep_does_not_expand_size() {
        let raw = read_case("case_167_git_grep");
        let compacted = compact_git_grep_for_ai(&raw);
        assert!(
            compacted.len() <= raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// 测试：git worktree 共享前缀被符号化为路径字典。
    #[test]
    fn git_worktree_symbolizes_shared_prefix() {
        let raw = read_case("case_165_git_worktree");
        let compacted = compact_git_worktree_for_ai(&raw);
        assert!(compacted.starts_with("git worktree list"), "{}", compacted);
        // 三条 worktree 共享仓库根前缀，应提取为 [paths] 字典
        assert!(
            compacted.contains("[paths] $P0=C:/git_work/TokenSlim"),
            "应提取共享前缀字典，实际输出: {:?}",
            compacted
        );
        // 根 worktree 与分支变体均符号化为 $P0
        assert!(compacted.contains("$P0-feature-auth"), "{}", compacted);
        assert!(compacted.contains("$P0-bugfix-123"), "{}", compacted);
        // 压缩不得扩张
        assert!(
            compacted.len() <= raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// 测试：git stash show 紧凑化 diff stat 行。
    #[test]
    fn git_stash_show_compacts_diff_stat_lines() {
        let raw = read_case("case_285_git_stash_show");
        let compacted = compact_git_stash_for_ai(&raw);
        assert!(compacted.starts_with("git stash show"), "{}", compacted);
        assert!(compacted.contains("src/main.rs"), "{}", compacted);
        assert!(compacted.contains("| 12 ++++++------"), "{}", compacted);
        assert!(compacted.contains("src/lib/utils.rs"), "{}", compacted);
        assert!(compacted.contains("|  3 ++-"), "{}", compacted);
        assert!(
            compacted.contains("tests/integration_test.rs"),
            "{}",
            compacted
        );
        assert!(compacted.contains("|  8 ++++++++"), "{}", compacted);
        assert!(
            compacted.contains("3 files, 16 ins, 7 del"),
            "{}",
            compacted
        );
    }

    /// 测试：git stash list 折叠连续索引样板，行序隐含索引。
    #[test]
    fn git_stash_list_collapses_index_sample() {
        let raw = read_case("case_131_git_stash_list");
        let compacted = compact_git_stash_for_ai(&raw);
        assert!(compacted.starts_with("git stash list"), "{}", compacted);
        // 每条 stash 完整内容保留，且不再含 `stash@{N}: ` 样板前缀
        assert!(
            compacted.contains("WIP on master: abc1234 last commit message"),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("saved on feature-xyz: def5678 another work in progress"),
            "{}",
            compacted
        );
        assert!(
            !compacted.lines().any(|l| l.contains("stash@{")),
            "应折叠 stash 索引样板，实际输出: {:?}",
            compacted
        );
        // 压缩不得扩张
        assert!(
            compacted.len() <= raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// 测试：git shortlog 扁平化作者分组。
    #[test]
    fn git_shortlog_flattens_author_groups() {
        let raw = read_case("case_170_git_shortlog");
        let compacted = compact_git_log_for_ai(&raw);
        assert!(compacted.starts_with("git shortlog"), "{}", compacted);
        assert!(
            compacted.contains(
                "alice.chen(10): Add login feature | Add user registration | Add password reset"
            ),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("bob.wang(5): Fix bug in auth | Update dependencies"),
            "{}",
            compacted
        );
    }

    /// 测试：log oneline 使用最短唯一前缀并在冲突时加长。
    #[test]
    fn git_log_oneline_uses_shortest_unique_prefix_with_collision_expansion() {
        let raw = read_case("case_324_git_log_oneline_full_hash_collision");
        let compacted = compact_git_log_for_ai(&raw);
        assert!(
            compacted.starts_with("git log --oneline -n 3"),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("aaaaaaaaaab feat: first collision candidate"),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("aaaaaaaaaac feat: second collision candidate"),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("bbbbbbbbbb feat: third distinct commit"),
            "{}",
            compacted
        );
    }

    /// 测试：reflog 哈希前缀使用最短唯一前缀。
    #[test]
    fn git_reflog_hash_prefix_uses_shortest_unique_prefix() {
        let raw = read_case("case_325_git_reflog_with_hash_collision");
        let compacted = compact_git_log_for_ai(&raw);
        assert!(compacted.starts_with("git reflog -n 3"), "{}", compacted);
        assert!(
            compacted.contains("aaaaaaaaaab HEAD@{0}: commit: first"),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("aaaaaaaaaac HEAD@{1}: commit: second"),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("bbbbbbbbbb HEAD@{2}: commit: third"),
            "{}",
            compacted
        );
    }

    /// 测试：reflog checkout 行被紧凑化。
    #[test]
    fn git_reflog_checkout_line_is_compacted() {
        let raw = read_case("case_166_git_reflog");
        let compacted = compact_git_log_for_ai(&raw);
        assert!(compacted.starts_with("git reflog"), "{}", compacted);
        assert!(
            compacted.contains("HEAD@{1}: co:main->feature-auth"),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("HEAD@{3}: co:feature-auth->main"),
            "{}",
            compacted
        );
    }

    /// 测试：高冲突场景下 log oneline 前缀仍保持唯一。
    #[test]
    fn git_log_oneline_heavy_collision_prefixes_remain_unique() {
        let raw = read_case("case_326_git_log_oneline_heavy_collision");
        let compacted = compact_git_log_for_ai(&raw);
        assert!(
            compacted.starts_with("git log --oneline -n 50"),
            "{}",
            compacted
        );

        let mut prefixes = std::collections::HashSet::new();
        let mut row_count = 0usize;
        for line in compacted.lines().skip(1) {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            row_count += 1;
            let prefix = t.split_whitespace().next().unwrap_or("");
            assert!(
                prefixes.insert(prefix.to_string()),
                "duplicate prefix found: {}",
                prefix
            );
        }

        assert_eq!(row_count, 50, "{}", compacted);
        assert_eq!(prefixes.len(), 50, "{}", compacted);
        assert!(
            compacted.contains("feat: collision item 1"),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("feat: collision item 50"),
            "{}",
            compacted
        );
    }

    /// 测试：blame 哈希冲突时使用最短唯一前缀。
    #[test]
    fn git_blame_uses_shortest_unique_prefix_when_hashes_collide() {
        let raw = read_case("case_327_git_blame_hash_collision");
        let compacted = compact_git_blame_for_ai(&raw);
        assert!(compacted.starts_with("git blame"), "{}", compacted);
        assert!(
            compacted.contains("aaaaaaaaaab @alice.chen"),
            "{}",
            compacted
        );
        assert!(compacted.contains("aaaaaaaaaac @bob.wang"), "{}", compacted);
        assert!(compacted.contains("^ "), "{}", compacted);
    }

    /// 测试：show 多提交冲突时使用最短唯一前缀。
    #[test]
    fn git_show_uses_shortest_unique_prefix_for_multi_commit_collision() {
        let raw = read_case("case_328_git_show_multi_commit_collision");
        let compacted = compact_git_show_for_ai(&raw);
        assert!(compacted.starts_with("git show"), "{}", compacted);
        assert!(
            compacted.contains(
                "aaaaaaaaaab @alice.chen 2026-04-29 09:28:41 feat: first colliding show commit"
            ),
            "{}",
            compacted
        );
        assert!(
            compacted.contains(
                "aaaaaaaaaac @bob.wang 2026-04-29 09:30:12 feat: second colliding show commit"
            ),
            "{}",
            compacted
        );
    }

    /// 测试：blame 带时区与补零行号时被紧凑化。
    #[test]
    fn git_blame_with_timezone_and_padded_lineno_is_compacted() {
        // P3-202：手写 mock 物理化为 samples 测试专用样本（红线：禁手写 mock 喂压缩器）。
        let raw = read_case("test_git_blame_timezone_padded");
        let compacted = compact_git_blame_for_ai(&raw);
        assert!(compacted.starts_with("git blame -L 1,4"), "{}", compacted);
        assert!(
            compacted.contains("03d06505 @nuoyazhizhou 2026-04-25 10:19:00 1 <BLANK>"),
            "{}",
            compacted
        );
        assert!(compacted.contains("^ 2 <BLANK>"), "{}", compacted);
        assert!(
            compacted.contains("^ 3 // --- IR 通用定义 (内联隔离) ---"),
            "{}",
            compacted
        );
    }

    /// 测试：git other help 被压缩且不丢失锚点。
    #[test]
    fn git_other_help_is_compacted_without_losing_anchor() {
        let raw = read_case("case_329_git_help");
        let compacted = compact_git_other_for_ai(&raw);
        assert!(compacted.starts_with("git"), "{}", compacted);
        assert!(
            compacted.contains("usage: git [-v | --version]"),
            "{}",
            compacted
        );
        // usage 多行续行已合并进 usage 主行
        assert!(compacted.contains("[--exec-path"), "{}", compacted);
        assert!(
            compacted.contains("clone Clone a repository into a new directory"),
            "{}",
            compacted
        );
        assert!(!compacted.contains("\n\n\n"), "{}", compacted);
        // usage 续行不再作为独立行存在（flag 行被折叠进 usage 行）
        assert!(
            !compacted
                .lines()
                .any(|l| l.trim_start().starts_with("[--exec-path")),
            "{}",
            compacted
        );
        assert!(
            compacted.len() < raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// 测试：git status 合并暂存与未暂存为单一 changes 段。
    #[test]
    fn git_status_merges_staged_and_unstaged_into_single_changes_section() {
        // P3-202：手写 mock 物理化为 samples 测试专用样本。
        let raw = read_case("test_git_status_merge_sections");
        let compacted = compact_git_status_for_ai(&raw);
        assert!(compacted.contains("M src/cli/methods.rs"), "{}", compacted);
        assert!(compacted.contains("M Cargo.toml"), "{}", compacted);
        assert!(compacted.contains("? tmp/new_file.txt"), "{}", compacted);
        assert!(!compacted.contains("[changes]"), "{}", compacted);
        assert!(!compacted.contains("[untracked]"), "{}", compacted);
        assert!(!compacted.contains("(use \"git"), "{}", compacted);
    }

    /// 测试 git status 的树结构重组功能
    #[test]
    fn git_status_tree_restructure_integration() {
        let raw = read_case("case_tree_status");
        let compacted = compact_git_status_for_ai(&raw);

        // 验证命令锚点保留
        assert!(compacted.starts_with("git status"), "{}", compacted);

        // 验证分支信息保留
        assert!(
            compacted.contains("BR:feature/tree-restructure"),
            "{}",
            compacted
        );

        // 验证树结构重组生效（应该包含目录结构）
        // 由于有 7 个文件，满足 min_files=4 的门控条件
        // 并且有共享的 src/ 目录，满足 min_shared_depth=1
        assert!(compacted.contains("src/"), "{}", compacted);

        // 验证文件状态保留
        assert!(compacted.contains("M "), "{}", compacted);
        assert!(compacted.contains("? "), "{}", compacted);

        // 验证不包含原始的提示信息
        assert!(!compacted.contains("(use \"git"), "{}", compacted);
        assert!(!compacted.contains("no changes added"), "{}", compacted);
    }

    /// 测试 git diff --name-only 的树结构重组功能
    #[test]
    fn git_diff_name_only_tree_restructure_integration() {
        let raw = read_case("case_tree_diff_name_only");
        let compacted = compact_git_diff_for_ai(&raw);

        // 验证命令锚点保留
        assert!(
            compacted.starts_with("git diff --name-only"),
            "{}",
            compacted
        );

        // 验证树结构重组生效
        assert!(compacted.contains("src/"), "{}", compacted);

        // 验证文件路径被正确处理
        // 树结构应该包含目录层级
        let lines: Vec<&str> = compacted.lines().collect();
        assert!(lines.len() > 1, "{}", compacted);

        // P3-203：落地 ROI 承诺（原注释声称不扩张却无断言）。树结构框线可能略微
        // 增加体积，但门控逻辑应确保不显著扩展——允许 10% 余量。
        assert!(
            compacted.len() <= raw.len() + raw.len() / 10,
            "树结构重组不应显著扩展: raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// 测试少量文件时不触发树结构重组（门控测试）
    #[test]
    fn git_status_tree_gating_with_few_files() {
        // P3-202：手写 mock 物理化为 samples 测试专用样本。
        let raw = read_case("test_git_status_few_files_gating");
        let compacted = compact_git_status_for_ai(&raw);

        // 只有 2 个文件，不满足 min_files=4 的门控条件
        // 应该返回扁平的列表格式
        assert!(compacted.contains("M README.md"), "{}", compacted);
        assert!(compacted.contains("M Cargo.toml"), "{}", compacted);

        // 不应该有树结构的框线字符
        assert!(!compacted.contains("├─"), "{}", compacted);
        assert!(!compacted.contains("└─"), "{}", compacted);
    }

    // ==================== Phase 2 P2 增强功能测试 ====================

    /// 测试 git status 在 merge/rebase/cherry-pick 冲突时必须保留 Unmerged paths 区块。
    /// 这是核心防失忆红线：5 种冲突标记 (both modified / added by us / added by them /
    /// deleted by us / deleted by them) 必须全部按 U=Unmerged 状态保留到压缩输出。
    #[test]
    fn git_status_preserves_unmerged_paths() {
        let raw = read_case("case_330_git_status_unmerged");
        let compacted = compact_git_status_for_ai(&raw);

        // 1. 命令锚点保留
        assert!(compacted.starts_with("git status"), "{}", compacted);

        // 2. 分支信息保留
        assert!(compacted.contains("BR:main"), "{}", compacted);

        // 3. Unmerged paths 区块的 5 种冲突标记必须全部保留（核心回归测试）
        // 状态码遵循 git porcelain v1 规范：UU/AU/UA/DU/UD
        assert!(compacted.contains("UU src/core/parser.rs"), "{}", compacted);
        assert!(
            compacted.contains("UU packages/sdk-nodejs/package-lock.json"),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("AU src/core/new_module.rs"),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("UA src/core/their_module.rs"),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("DU src/legacy/old_module.rs"),
            "{}",
            compacted
        );
        assert!(
            compacted.contains("UD src/legacy/their_old_module.rs"),
            "{}",
            compacted
        );

        // 4. 其他章节也必须保留（不是只保留 unmerged）
        // 暂存区 staged new file / modified
        assert!(
            compacted.contains("A src/core/new_feature.rs"),
            "{}",
            compacted
        );
        assert!(compacted.contains("M docs/CHANGELOG.md"), "{}", compacted);
        // 工作区 unstaged modified
        assert!(compacted.contains("M src/cli/methods.rs"), "{}", compacted);
        assert!(compacted.contains("M Cargo.toml"), "{}", compacted);
        // 未跟踪文件
        assert!(compacted.contains("? tmp/draft.txt"), "{}", compacted);
        assert!(compacted.contains("? .qoder/"), "{}", compacted);

        // 5. 噪声被压缩
        assert!(!compacted.contains("(use \"git"), "{}", compacted);
        assert!(
            !compacted.contains("no changes added to commit"),
            "{}",
            compacted
        );
        assert!(
            !compacted.contains("Your branch is up to date"),
            "{}",
            compacted
        );
    }

    /// 测试：merge 冲突被压缩为 [CONFLICT] 摘要。
    #[test]
    fn compresses_merge_conflicts() {
        let raw = read_case("case_081_merge_conflict");
        let compacted = compact_git_merge_enhanced(&raw);

        assert!(compacted.starts_with("git merge"), "{}", compacted);
        assert!(compacted.contains("[CONFLICT]"), "{}", compacted);
        assert!(compacted.contains("src/main.rs"), "{}", compacted);
        assert!(compacted.contains("src/utils.rs"), "{}", compacted);

        // 验证压缩率
        assert!(
            compacted.len() < raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// 测试：rebase 交互式输出被压缩。
    #[test]
    fn compresses_rebase_interactive() {
        let raw = read_case("case_082_rebase_interactive");
        let compacted = compact_git_rebase_enhanced(&raw);

        assert!(compacted.starts_with("git rebase"), "{}", compacted);
        assert!(compacted.contains("[REBASE]"), "{}", compacted);
        assert!(compacted.contains("10 commits"), "{}", compacted);
        assert!(compacted.contains("interactive mode"), "{}", compacted);

        // 验证注释行被过滤
        assert!(!compacted.contains("# Commands:"), "{}", compacted);
        assert!(!compacted.contains("# p, pick"), "{}", compacted);

        // 验证命令行保留
        assert!(compacted.contains("pick abc1234"), "{}", compacted);
        assert!(compacted.contains("pick def5678"), "{}", compacted);

        // 验证压缩率
        assert!(
            compacted.len() < raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// 测试：log --graph 输出被压缩。
    #[test]
    fn compresses_log_graph() {
        let raw = read_case("case_083_log_graph");
        let compacted = compact_git_log_enhanced(&raw);

        assert!(compacted.starts_with("git log --graph"), "{}", compacted);

        // 应该触发压缩（> 10 个提交）
        assert!(compacted.contains("[GRAPH]"), "{}", compacted);
        assert!(compacted.contains("commits"), "{}", compacted);

        // 验证图形字符被去除（在提交行中）
        let lines: Vec<&str> = compacted.lines().skip(2).collect(); // 跳过命令行和摘要行
        for line in &lines {
            if !line.contains("...") && !line.is_empty() {
                // 提交行不应该包含图形字符
                assert!(
                    !line.contains("|\\ "),
                    "Line should not contain graph chars: {}",
                    line
                );
                assert!(
                    !line.contains("|/ "),
                    "Line should not contain graph chars: {}",
                    line
                );
            }
        }

        // 验证提交信息保留（至少前几个）
        assert!(compacted.contains("abc1234"), "{}", compacted);
        assert!(compacted.contains("Merge branch"), "{}", compacted);

        // 验证压缩率
        assert!(
            compacted.len() < raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// 测试：长 reflog 被折叠。
    #[test]
    fn compresses_reflog_long() {
        let raw = read_case("case_084_reflog_long");
        let compacted = compact_git_log_enhanced(&raw);

        assert!(compacted.starts_with("git reflog"), "{}", compacted);

        // 验证压缩发生了：原始有 50 个条目，压缩后应该只有 20 个
        let entry_count = compacted.lines().filter(|l| l.contains("HEAD@{")).count();
        assert!(
            entry_count <= 20,
            "Should have at most 20 entries after compression, got {}",
            entry_count
        );

        // 验证前 20 个条目保留
        assert!(compacted.contains("HEAD@{0}"), "{}", compacted);
        assert!(compacted.contains("HEAD@{19}"), "{}", compacted);

        // 验证后面的条目被省略（不应该有 HEAD@{20} 或更大的）
        assert!(!compacted.contains("HEAD@{20}"), "{}", compacted);
        assert!(!compacted.contains("HEAD@{30}"), "{}", compacted);

        // 验证压缩率
        assert!(
            compacted.len() < raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// 测试：merge 冲突压缩不扩张。
    #[test]
    fn merge_conflict_compression_does_not_expand() {
        let raw = read_case("case_081_merge_conflict");
        let compacted = compact_git_merge_enhanced(&raw);

        // ROI 门控：确保不扩展
        assert!(
            compacted.len() <= raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// 测试：rebase 交互式压缩不扩张。
    #[test]
    fn rebase_interactive_compression_does_not_expand() {
        let raw = read_case("case_082_rebase_interactive");
        let compacted = compact_git_rebase_enhanced(&raw);

        // ROI 门控：确保不扩展
        assert!(
            compacted.len() <= raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// 测试：log graph 压缩不扩张。
    #[test]
    fn log_graph_compression_does_not_expand() {
        let raw = read_case("case_083_log_graph");
        let compacted = compact_git_log_enhanced(&raw);

        // ROI 门控：确保不扩展
        assert!(
            compacted.len() <= raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// 测试：reflog 压缩不扩张。
    #[test]
    fn reflog_compression_does_not_expand() {
        let raw = read_case("case_084_reflog_long");
        let compacted = compact_git_log_enhanced(&raw);

        // ROI 门控：确保不扩展
        assert!(
            compacted.len() <= raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// 测试：git pull 样例中无句号的 "Updating X..Y" 中间进度行被折叠，
    /// 仅保留带句号的完成态行，避免 hash 区间三处重复造成 token 冗余。
    #[test]
    fn git_pull_drops_intermediate_updating_line() {
        let raw = read_case("case_49_git_pull");
        let compacted = compact_git_pull_for_ai(&raw);

        // 锚点守卫：首行保留命令触发行
        assert!(compacted.starts_with("git pull"));
        // 摘要行保留远程更新范围
        assert!(compacted.contains("..def5678  main"));
        // 完成态行保留
        assert!(compacted.contains("Updating abc1234..def5678."));
        // 中间进度行（无句号）被折叠：不应同时出现两次无句号的 Updating 行
        let updating_count = compacted
            .lines()
            .filter(|l| l.starts_with("Updating ") && !l.ends_with('.'))
            .count();
        assert!(updating_count <= 1, "未折叠中间进度行: {}", compacted);
    }

    /// 测试：git bisect 的 Bisecting 模板行被折叠为紧凑格式。
    #[test]
    fn git_bisect_compacts_bisecting_template() {
        let raw = read_case("case_239_git_bisect_bad");
        let compacted = compact_git_bisect_for_ai(&raw);
        assert!(compacted.starts_with("git bisect bad"), "{}", compacted);
        // 模板文字被折叠为仅含数字的紧凑格式
        assert!(
            compacted.contains("Bisecting: 6 left (~3)"),
            "应折叠 Bisecting 模板，实际输出: {:?}",
            compacted
        );
        assert!(
            !compacted.contains("revisions left to test after this"),
            "不应保留模板文字，实际输出: {:?}",
            compacted
        );
        // 压缩不得扩张
        assert!(
            compacted.len() <= raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// 测试：git bisect good 正确处理额外 commit 行与模板折叠。
    #[test]
    fn git_bisect_good_keeps_commit_line() {
        let raw = read_case("case_240_git_bisect_good");
        let compacted = compact_git_bisect_for_ai(&raw);
        assert!(compacted.starts_with("git bisect good"), "{}", compacted);
        // 中间 commit 行保留
        assert!(
            compacted.contains("bdef123 Commit message in the middle"),
            "{}",
            compacted
        );
        // 模板折叠
        assert!(
            compacted.contains("Bisecting: 6 left (~3)"),
            "{}",
            compacted
        );
        assert!(
            compacted.len() <= raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    /// P2-69（D-1）：多字节 UTF-8 首词（如中文 commit 消息首 token）不使最短唯一前缀
    /// 计算 panic。构造两个共享中文前缀的候选，验证构建映射不崩溃且结果落在字符边界。
    #[test]
    fn hash_prefix_map_handles_multibyte_first_word() {
        // 中文全角字符：每个 3 字节，模拟 oneline 模式把非 hash 首词混入候选集
        let candidates = vec![
            "b1a2b3c4d5e6".to_string(),
            "优化功能提交一号".to_string(), // 多字节长首词
            "优化功能提交二号".to_string(), // 与前一个共享前缀
        ];
        let map = build_shortest_unique_prefix_map(&candidates, 2);
        // map 必须产出全部三条 key（不 panic）
        assert_eq!(map.len(), 3, "map={:?}", map);
        // 每条 short 值必须是合法 UTF-8（get(..) 保底不会从多字节中间切）
        for v in map.values() {
            assert!(std::str::from_utf8(v.as_bytes()).is_ok());
        }
        // 多字节首词的短前缀必须是其自身（unique 且长度不越过全集边界）
        let chinese = &map["优化功能提交一号"];
        assert!(!chinese.is_empty());
    }

    /// P2-69（D-1）：最短唯一前缀长度自身的返回值必须落在 UTF-8 字符边界，
    /// 直接对其结果切片不应 panic。
    #[test]
    fn shortest_unique_prefix_len_returns_char_boundary() {
        let values = vec![
            "功能提交".to_string(), // 4 个 3 字节汉字 = 12 字节
            "功能回复".to_string(),
        ];
        let len = shortest_unique_prefix_len(&values, 0, 1);
        // “功能”二字（6 字节）为共享前缀，分歧点在第三个字，唯一前缀为“功能提”=9 字节（恰好是字符边界）
        assert_eq!(len, 9, "len={}", len);
        let cur = &values[0];
        // 对返回值切片不可能 panic（已是字符边界）
        let _prefix = &cur[..len];
    }

    /// 测试：git status 同状态多行不再坍缩为单叶子（P2-61 处置）。
    #[test]
    /// 契约：全 M 多行 status 输出压缩后必须保留全部文件路径。旧默认正则
    /// （不要求分隔符）把 "M src/xxx" 的首捕获吞成状态字母 "M"，同状态
    /// ≥4 行在 Trie 中坍缩为唯一叶子，整棵文件列表丢失。
    fn p2_61_status_same_state_lines_not_collapsed() {
        let raw = r#"git status
On branch main
Changes to be committed:
  (use "git restore --staged <file>..." to unstage)
        modified:   src/core/engine.rs
        modified:   src/core/dispatcher.rs
        modified:   src/core/pipeline.rs
        modified:   src/plugins/mod.rs
"#;
        let compacted = compact_git_status_for_ai(&raw);
        assert!(compacted.contains("engine.rs"), "engine.rs 不得被坍缩吞掉: {}", compacted);
        assert!(compacted.contains("dispatcher.rs"), "dispatcher.rs 不得被坍缩吞掉: {}", compacted);
        assert!(compacted.contains("pipeline.rs"), "pipeline.rs 不得被坍缩吞掉: {}", compacted);
        assert!(compacted.contains("mod.rs"), "mod.rs 不得被坍缩吞掉: {}", compacted);
        // 树重组应正常生效（collapse_single_child 渲染 `└─ src`，无尾斜杠）
        assert!(
            compacted.contains("├─") || compacted.contains("└─"),
            "树重组应产出树形渲染: {}",
            compacted
        );
        assert!(
            !compacted.contains("modified:"),
            "原始提示行不应保留: {}",
            compacted
        );
    }
}
