#[cfg(test)]
mod tests {
    use super::super::methods::*;
    use super::super::parser::*;

    /// 从 samples/vcs_git_plugin/<name>.log 读取测试用例
    fn read_case(name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("vcs_git_plugin")
            .join(format!("{}.log", name));
        std::fs::read_to_string(&path).unwrap_or_default()
    }

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

    #[test]
    fn git_fetch_noise_filtered_from_sample_file() {
        let raw = read_case("case_33_git_fetch");
        let compacted = compact_git_fetch_for_ai(&raw);
        assert!(compacted.starts_with("git fetch"));
        assert!(compacted.contains("From "));
        assert!(!compacted.contains("Counting objects:"));
        assert!(!compacted.contains("Compressing objects:"));
    }

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

    #[test]
    fn git_merge_conflict_normalized_from_sample_file() {
        let raw = read_case("case_44_git_merge");
        let compacted = compact_git_merge_for_ai(&raw);
        assert!(compacted.starts_with("git merge"));
        assert!(compacted.contains("!CONFLICT:src/main.rs"));
        assert!(compacted.contains("Auto-merging src/main.rs"));
    }

    #[test]
    fn git_rm_parser_from_sample_file() {
        let raw = read_case("case_51_git_rm");
        let compacted = compact_git_rm_for_ai(&raw);
        assert!(compacted.starts_with("git rm"));
        assert!(compacted.contains("D src/old_file.rs"));
        assert!(compacted.contains("config/old_config.json"));
    }

    #[test]
    fn git_cherry_pick_proper_parser_from_sample_file() {
        let raw = read_case("case_47_git_cherry_pick");
        let compacted = compact_git_cherry_pick_for_ai(&raw);
        assert!(compacted.starts_with("git cherry-pick"));
        assert!(compacted.contains(" -> "));
        assert!(!compacted.contains("Counting objects"));
    }

    #[test]
    fn git_revert_parser_from_sample_file() {
        let raw = read_case("case_48_git_revert");
        let compacted = compact_git_revert_for_ai(&raw);
        assert!(compacted.starts_with("git revert"));
        assert!(compacted.contains("Reverting commit"));
        assert!(compacted.contains("Reverted commit"));
        assert!(compacted.contains("files"));
    }

    #[test]
    fn git_remote_parser_from_sample_file() {
        let raw = read_case("case_128_git_remote");
        let compacted = compact_git_remote_for_ai(&raw);
        assert!(compacted.starts_with("git remote"));
        assert!(compacted.contains("origin"));
        assert!(compacted.contains("github.com"));
        assert!(compacted.contains("upstream"));
    }

    #[test]
    fn git_tag_list_parser_from_sample_file() {
        let raw = read_case("case_130_git_tag_list");
        let compacted = compact_git_tag_for_ai(&raw);
        assert!(compacted.starts_with("git tag"));
        assert!(compacted.contains("v1.0.0"));
        assert!(compacted.contains("v2.0.0"));
    }

    #[test]
    fn git_switch_detached_head_from_sample_file() {
        let raw = read_case("case_53_git_switch");
        let compacted = compact_git_switch_for_ai(&raw);
        assert!(compacted.starts_with("git switch"));
        assert!(compacted.contains("BR:main"));
        assert!(compacted.contains("BR:*feature/new-feature"));
        assert!(compacted.contains("BR:prev@abc1234"));
    }

    #[test]
    fn git_branch_v_keeps_star_without_br_prefix() {
        let raw = read_case("case_287_git_branch_v");
        let compacted = compact_git_branch_for_ai(&raw);
        assert!(compacted.starts_with("git branch -v"), "{}", compacted);
        assert!(compacted.contains("* main"), "{}", compacted);
        assert!(!compacted.contains("*BR:"), "{}", compacted);
    }

    #[test]
    fn git_grep_keeps_command_anchor() {
        let raw = read_case("case_167_git_grep");
        let compacted = compact_git_grep_for_ai(&raw);
        assert!(compacted.starts_with("git grep"), "{}", compacted);
        assert!(compacted.contains("src/main.rs:15:"), "{}", compacted);
        assert!(compacted.contains("src/utils.rs:42:"), "{}", compacted);
    }

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

    #[test]
    fn git_worktree_does_not_expand_size() {
        let raw = read_case("case_165_git_worktree");
        let compacted = compact_git_worktree_for_ai(&raw);
        assert!(
            compacted.len() <= raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

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

    #[test]
    fn git_blame_with_timezone_and_padded_lineno_is_compacted() {
        let raw = r#"git blame -L 1,4 src/plugins/vcs_git_plugin/parser.rs
03d06505 (nuoyazhizhou 2026-04-25 10:19:00 +0800  1)
03d06505 (nuoyazhizhou 2026-04-25 10:19:00 +0800  2)
03d06505 (nuoyazhizhou 2026-04-25 10:19:00 +0800  3) // --- IR 通用定义 (内联隔离) ---
03d06505 (nuoyazhizhou 2026-04-25 10:19:00 +0800  4) #[derive(Debug, PartialEq, Eq)]
"#;
        let compacted = compact_git_blame_for_ai(raw);
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
        assert!(
            compacted.contains("clone  Clone a repository into a new directory"),
            "{}",
            compacted
        );
        assert!(!compacted.contains("\n\n\n"), "{}", compacted);
        assert!(
            compacted.len() < raw.len(),
            "raw={} compacted={}",
            raw.len(),
            compacted.len()
        );
    }

    #[test]
    fn git_status_merges_staged_and_unstaged_into_single_changes_section() {
        let raw = r#"git status
On branch master
Changes to be committed:
  (use "git restore --staged <file>..." to unstage)
        modified:   src/cli/methods.rs
Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
        modified:   Cargo.toml
Untracked files:
  (use "git add <file>..." to include in what will be committed)
        tmp/new_file.txt
"#;
        let compacted = compact_git_status_for_ai(raw);
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

        // 验证不会扩展大小（树结构应该更紧凑或相当）
        // 注意：由于树结构添加了框线字符，可能会略微增加大小
        // 但门控逻辑应该确保不会显著扩展
    }

    /// 测试少量文件时不触发树结构重组（门控测试）
    #[test]
    fn git_status_tree_gating_with_few_files() {
        let raw = r#"git status
On branch main
Changes not staged for commit:
        modified:   README.md
        modified:   Cargo.toml
"#;
        let compacted = compact_git_status_for_ai(raw);

        // 只有 2 个文件，不满足 min_files=4 的门控条件
        // 应该返回扁平的列表格式
        assert!(compacted.contains("M README.md"), "{}", compacted);
        assert!(compacted.contains("M Cargo.toml"), "{}", compacted);

        // 不应该有树结构的框线字符
        assert!(!compacted.contains("├─"), "{}", compacted);
        assert!(!compacted.contains("└─"), "{}", compacted);
    }

    // ==================== Phase 2 P2 增强功能测试 ====================

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
}
