#[cfg(test)]
mod tests {
    use crate::plugins::vcs_hg_plugin::methods::*;
    use crate::plugins::vcs_hg_plugin::parser::*;

    #[test]
    fn generate_vcs_hg_showcase_report() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let samples_dir = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("vcs_hg_plugin");

        let cases: Vec<(&str, &str, &str)> = vec![
            // Core commands (named parsers)
            ("case_08", "hg_status", "status"),
            ("case_09", "hg_diff", "diff"),
            ("case_10", "hg_log", "log"),
            ("case_11", "hg_heads", "log"),
            ("case_12", "hg_outgoing", "log"),
            ("case_13", "hg_incoming", "log"),
            ("case_14", "hg_parents", "log"),
            ("case_58", "hg_clone", "log"),
            ("case_59", "hg_pull", "log"),
            ("case_60", "hg_push", "log"),
            ("case_61", "hg_update", "status"),
            ("case_62", "hg_commit", "log"),
            ("case_63", "hg_branches", "log"),
            ("case_64", "hg_merge", "log"),
            ("case_65", "hg_rollback", "log"),
            ("case_66", "hg_backout", "log"),
            ("case_67", "hg_shelve", "log"),
            ("case_68", "hg_phase", "log"),
            ("case_69", "hg_bookmarks", "log"),
            ("case_82", "hg_tag", "log"),
            // Generic parser cases
            ("case_132", "hg_copy", "log"),
            ("case_133", "hg_move", "log"),
            ("case_134", "hg_purge", "log"),
            ("case_135", "hg_archive", "log"),
            ("case_136", "hg_verify", "log"),
            ("case_171", "hg_identify", "other"),
            ("case_172", "hg_paths", "other"),
            ("case_173", "hg_config", "other"),
            ("case_174", "hg_summarize", "other"),
            ("case_175", "hg_transplant", "log"),
            // Extended status variants
            ("case_232", "hg_status_S", "status"),
            ("case_233", "hg_status_c", "status"),
            ("case_274", "hg_st", "status"),
            ("case_275", "hg_status_quiet", "status"),
            ("case_276", "hg_status_untracked", "status"),
            ("case_277", "hg_status_path", "status"),
            // Extended log variants
            ("case_278", "hg_log_limit", "log"),
            ("case_279", "hg_log_verbose", "log"),
            ("case_280", "hg_log_style_compact", "log"),
            ("case_281", "hg_log_tip", "log"),
            // Extended diff variants
            ("case_301", "hg_diff_git", "diff"),
            ("case_302", "hg_diff_stat", "diff"),
            ("case_303", "hg_log_follow", "log"),
            ("case_304", "hg_log_patch", "log"),
            ("case_305", "hg_update_C", "status"),
            ("case_306", "hg_commit_amend", "log"),
            // Phase 2 P2 增强功能 - 新增 3 个 case
            ("case_085", "shelve_list", "shelve"),
            ("case_086", "graft", "graft"),
            ("case_087", "histedit", "histedit"),
        ];

        let mut all_output = String::new();
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n  VCS Hg AI Compact Showcase\n");
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n\n");

        for (case_id, file_base, profile) in &cases {
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

            let compacted = route_hg_command(&raw, file_base, profile);

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
            all_output.push_str(&format!("\nCase {} - Hg ({})\n", case_id, file_name));
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
                .join("vcs_hg_compact_showcase_report.txt"),
            &all_output,
        )
        .unwrap();
    }

    fn route_hg_command(raw: &str, file_base: &str, profile: &str) -> String {
        match profile {
            "status" => {
                if file_base.contains("update") {
                    process_parser(&HgUpdateParser, raw)
                } else {
                    compact_hg_status_for_ai(raw)
                }
            }
            "diff" => compact_hg_diff_for_ai(raw),
            "log" => {
                if file_base.contains("clone") {
                    process_parser(&HgCloneParser, raw)
                } else if file_base.contains("pull") {
                    process_parser(&HgPullParser, raw)
                } else if file_base.contains("push") {
                    process_parser(&HgPushParser, raw)
                } else if file_base.contains("commit") {
                    process_parser(&HgCommitParser, raw)
                } else if file_base.contains("branches") {
                    process_parser(&HgBranchesParser, raw)
                } else if file_base.contains("merge") {
                    process_parser(&HgMergeParser, raw)
                } else if file_base.contains("rollback") {
                    process_parser(&HgRollbackParser, raw)
                } else if file_base.contains("backout") {
                    process_parser(&HgBackoutParser, raw)
                } else if file_base.contains("shelve") {
                    process_parser(&HgShelveParser, raw)
                } else if file_base.contains("phase") {
                    process_parser(&HgPhaseParser, raw)
                } else if file_base.contains("bookmarks") {
                    process_parser(&HgBookmarksParser, raw)
                } else if file_base.contains("tag") {
                    process_parser(&HgTagParser, raw)
                } else if file_base.contains("heads") {
                    compact_hg_heads_for_ai(raw)
                } else if file_base.contains("outgoing") {
                    compact_hg_outgoing_for_ai(raw)
                } else if file_base.contains("incoming") {
                    compact_hg_incoming_for_ai(raw)
                } else if file_base.contains("parents") {
                    compact_hg_parents_for_ai(raw)
                } else if file_base.contains("copy")
                    || file_base.contains("move")
                    || file_base.contains("purge")
                    || file_base.contains("archive")
                    || file_base.contains("verify")
                    || file_base.contains("transplant")
                {
                    // HG-9: "other" 命令路由到 compact_hg_other_for_ai
                    compact_hg_other_for_ai(raw)
                } else {
                    compact_hg_log_for_ai(raw)
                }
            }
            "shelve" => compact_hg_shelve_enhanced(raw),
            "graft" => compact_hg_graft_enhanced(raw),
            "histedit" => compact_hg_histedit_enhanced(raw),
            _ => compact_hg_other_for_ai(raw),
        }
    }
}
