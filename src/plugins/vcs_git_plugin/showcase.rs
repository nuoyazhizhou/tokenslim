#[cfg(test)]
mod tests {
    use crate::plugins::vcs_git_plugin::methods::*;

    #[test]
    fn generate_vcs_git_showcase_report() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let samples_dir = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("vcs_git_plugin");

        let cases = [
            ("case_23", "git_status", "status"),
            ("case_24", "git_diff", "diff"),
            ("case_25", "git_log", "log"),
            ("case_26", "git_show", "diff"),
            ("case_30", "git_checkout", "status"),
            ("case_31", "git_checkout_file", "status"),
            ("case_32", "git_stash", "status"),
            ("case_33", "git_fetch", "log"),
            ("case_34", "git_push", "log"),
            ("case_44", "git_merge", "log"),
            ("case_45", "git_rebase", "log"),
            ("case_46", "git_reset", "log"),
            ("case_47", "git_cherry_pick", "log"),
            ("case_48", "git_revert", "log"),
            ("case_49", "git_pull", "log"),
            ("case_50", "git_add", "status"),
            ("case_51", "git_rm", "status"),
            ("case_52", "git_restore", "status"),
            ("case_53", "git_switch", "status"),
            ("case_54", "git_tag", "log"),
            ("case_55", "git_bisect", "log"),
            ("case_56", "git_clean", "status"),
            ("case_57", "git_submodule", "log"),
            ("case_128", "git_remote", "log"),
            ("case_129", "git_branch", "log"),
            ("case_130", "git_tag_list", "log"),
            ("case_131", "git_stash_list", "log"),
            ("case_163", "git_blame", "other"),
            ("case_164", "git_rebase_i", "log"),
            ("case_165", "git_worktree", "other"),
            ("case_166", "git_reflog", "log"),
            ("case_167", "git_grep", "other"),
            ("case_168", "git_log_oneline", "log"),
            ("case_169", "git_diff_stat", "diff"),
            ("case_170", "git_shortlog", "log"),
            ("case_237", "git_log_oneline_short", "log"),
            ("case_238", "git_log_graph", "log"),
            ("case_239", "git_bisect_bad", "log"),
            ("case_240", "git_bisect_good", "log"),
            ("case_241", "git_clean_fd", "status"),
            ("case_246", "git_status_branch", "status"),
            ("case_247", "git_status_porcelain", "status"),
            ("case_248", "git_status_short", "status"),
            ("case_249", "git_status_long", "status"),
            ("case_250", "git_status_ignored", "status"),
            ("case_251", "git_status_untracked_all", "status"),
            ("case_252", "git_log_oneline_n20", "log"),
            ("case_253", "git_log_stat", "log"),
            ("case_254", "git_log_patch", "log"),
            ("case_255", "git_log_graph", "log"),
            ("case_256", "git_log_all", "log"),
            ("case_257", "git_diff_cached", "diff"),
            ("case_258", "git_diff_head", "diff"),
            ("case_259", "git_diff_stat", "diff"),
            ("case_260", "git_diff_word_diff", "diff"),
            ("case_261", "git_diff_branches", "diff"),
            ("case_283", "git_diff_name_only", "diff"),
            ("case_284", "git_diff_name_status", "diff"),
            ("case_285", "git_stash_show", "other"),
            ("case_286", "git_stash_pop", "other"),
            ("case_287", "git_branch_v", "log"),
            ("case_288", "git_branch_d", "log"),
            ("case_289", "git_branch_D", "log"),
            ("case_290", "git_add_p", "status"),
            ("case_291", "git_reset_hard", "log"),
            ("case_292", "git_reset_soft", "log"),
            ("case_293", "git_reset_mixed", "log"),
            ("case_294", "git_checkout_b", "status"),
            ("case_295", "git_cherry_pick_continue", "log"),
            ("case_296", "git_cherry_pick_abort", "log"),
            ("case_297", "git_rebase_continue", "log"),
            ("case_298", "git_rebase_abort", "log"),
            ("case_299", "git_rebase_skip", "log"),
            ("case_300", "git_merge_abort", "log"),
            ("case_323", "git_log_parentheses", "log"),
            ("case_324", "git_log_oneline_full_hash_collision", "log"),
            ("case_325", "git_reflog_with_hash_collision", "log"),
            ("case_326", "git_log_oneline_heavy_collision", "log"),
            ("case_327", "git_blame_hash_collision", "other"),
            ("case_328", "git_show_multi_commit_collision", "diff"),
            // Phase 2 P2 增强功能案例
            ("case_081", "merge_conflict", "log"),
            ("case_082", "rebase_interactive", "log"),
            ("case_083", "log_graph", "log"),
            ("case_084", "reflog_long", "log"),
            ("case_329", "git_help", "other"),
            ("case_tree", "diff_name_only", "diff"),
            ("case_tree", "status", "status"),
        ];

        let mut all_output = String::new();
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n  VCS Git AI Compact Showcase\n");
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n\n");

        for (case_id, file_base, _profile) in cases {
            let file_name = format!("{}_{}.log", case_id, file_base);
            let file_path = samples_dir.join(&file_name);

            if !file_path.exists() {
                all_output.push_str(&format!(
                    "[SKIP] {} - file not found: {}\n\n",
                    case_id, file_name
                ));
                continue;
            }

            let raw = std::fs::read_to_string(&file_path).unwrap_or_default();
            let original_lines = raw.lines().count();
            let original_bytes = raw.len();

            let command = file_base.to_string();
            let compacted = if command == "git_status" || command.contains("status_") {
                compact_git_status_for_ai(&raw)
            } else if command.contains("checkout") {
                compact_git_checkout_for_ai(&raw)
            } else if command.contains("diff") {
                compact_git_diff_for_ai(&raw)
            } else if command.contains("show") && !command.contains("stash_show") {
                compact_git_show_for_ai(&raw)
            } else if command.contains("add") {
                compact_git_add_for_ai(&raw)
            } else if command.contains("stash") {
                compact_git_stash_for_ai(&raw)
            } else if command.contains("reset") {
                compact_git_reset_for_ai(&raw)
            } else if command.contains("switch") {
                compact_git_switch_for_ai(&raw)
            } else if command.contains("merge") || command.contains("merge_conflict") {
                // 使用增强版本处理 merge conflict
                compact_git_merge_enhanced(&raw)
            } else if command.contains("restore") {
                compact_git_restore_for_ai(&raw)
            } else if command.contains("clean") {
                compact_git_clean_for_ai(&raw)
            } else if command.contains("rebase") || command.contains("rebase_interactive") {
                // 使用增强版本处理 rebase interactive
                compact_git_rebase_enhanced(&raw)
            } else if command.contains("log")
                || command.contains("log_graph")
                || command.contains("reflog")
            {
                // 使用增强版本处理 log/graph/reflog
                compact_git_log_enhanced(&raw)
            } else {
                compact_git_other_for_ai(&raw)
            };

            let compact_lines = if compacted.is_empty() {
                0
            } else {
                compacted.lines().count()
            };
            let compact_bytes = compacted.len();
            let compression_ratio = if original_bytes > 0 {
                (1.0 - compact_bytes as f64 / original_bytes as f64) * 100.0
            } else {
                0.0
            };

            all_output.push_str(&"-".repeat(80));
            all_output.push_str(&format!("\nCase {} - Git ({})\n", case_id, file_name));
            all_output.push_str(&"-".repeat(80));
            all_output.push_str(&format!(
                "\nOriginal: {} lines, {} bytes  |  Compact: {} lines, {} bytes  |  Compression: {:.1}%\n",
                original_lines, original_bytes, compact_lines, compact_bytes, compression_ratio
            ));

            all_output.push_str("-- Case text --\n");
            all_output.push_str(&"-".repeat(80));
            all_output.push_str("\n");
            all_output.push_str(&raw);
            if !all_output.ends_with('\n') {
                all_output.push('\n');
            }

            all_output.push_str("-- Compact Output (full) --\n");
            all_output.push_str(&"-".repeat(80));
            all_output.push_str("\n");
            all_output.push_str(&compacted);
            if !all_output.ends_with('\n') {
                all_output.push('\n');
            }
        }

        std::fs::write(
            std::path::Path::new(manifest_dir)
                .join("target")
                .join("vcs_git_compact_showcase_report.txt"),
            &all_output,
        )
        .unwrap();
    }
}
