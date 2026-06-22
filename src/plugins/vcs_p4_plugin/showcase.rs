#[cfg(test)]
mod tests {
    use super::super::methods::*;

    #[test]
    fn generate_vcs_p4_showcase_report() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let dir = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("vcs_p4_plugin");
        let cases: &[(&str, &str, &str)] = &[
            ("case_15", "p4_opened", "status"),
            ("case_16", "p4_describe", "diff"),
            ("case_17", "p4_changes", "log"),
            ("case_18", "p4_fstat", "other"),
            ("case_19", "p4_where", "other"),
            ("case_20", "p4_info", "other"),
            ("case_21", "p4_labels", "log"),
            ("case_22", "p4_dirs", "other"),
            ("case_83", "p4_sync", "status"),
            ("case_84", "p4_submit", "log"),
            ("case_85", "p4_shelve", "log"),
            ("case_86", "p4_unshelve", "log"),
            ("case_87", "p4_resolve", "status"),
            ("case_88", "p4_revert", "status"),
            ("case_89", "p4_edit", "status"),
            ("case_90", "p4_add", "status"),
            ("case_91", "p4_delete", "status"),
            ("case_142", "p4_move", "status"),
            ("case_143", "p4_copy", "status"),
            ("case_144", "p4_integrate", "status"),
            ("case_145", "p4_branches", "other"),
            ("case_179", "p4_branch", "other"),
            ("case_180", "p4_label", "other"),
            ("case_181", "p4_users", "other"),
            ("case_182", "p4_workspaces", "other"),
            ("case_183", "p4_client", "other"),
            ("case_184", "p4_files", "other"),
            ("case_185", "p4_filelog", "log"),
            ("case_186", "p4_print", "other"),
            ("case_211", "p4_tag", "other"),
            ("case_212", "p4_passwd", "other"),
            ("case_213", "p4_protect", "other"),
            ("case_214", "p4_triggers", "other"),
            ("case_215", "p4_depot", "other"),
            ("case_216", "p4_diff2", "other"),
            ("case_234", "p4_opened_long", "status"),
            ("case_235", "p4_describe_short", "diff"),
            ("case_236", "p4_changes_max", "log"),
            ("case_307", "p4_diff", "other"),
            ("case_308", "p4_changes_l", "log"),
            ("case_309", "p4_describe_S", "diff"),
            ("case_310", "p4_sync_n", "status"),
            ("case_311", "p4_diff_dc", "other"),
            ("case_312", "p4_fstat_T", "other"),
        ];
        let mut out = String::new();
        out.push_str(&"=".repeat(80));
        out.push_str("\n  VCS P4 AI Compact Showcase - Detailed Case-by-Case Report\n");
        out.push_str(&"=".repeat(80));
        out.push_str("\n\n");
        for (id, fb, prof) in cases {
            let fnm = format!("{}_{}.log", id, fb);
            let fp = dir.join(&fnm);
            if !fp.exists() {
                continue;
            }
            let raw = std::fs::read_to_string(&fp).unwrap_or_default();
            let ol = raw.lines().count();
            let ob = raw.len();
            let compacted_raw = match *prof {
                "status" => compact_p4_status_for_ai(&raw),
                "diff" => compact_p4_describe_for_ai(&raw),
                "log" => compact_p4_log_family_for_ai(&raw),
                "other" => compact_p4_other_for_ai(&raw),
                _ => raw.clone(),
            };
            // 法则 P4-1: 对压缩结果中的 depot 路径执行字典压缩
            let compacted = compress_depot_paths(&compacted_raw);
            let cl = if compacted.is_empty() {
                0
            } else {
                compacted.lines().count()
            };
            let cb = compacted.len();
            let ratio = if ob > 0 {
                (1.0 - cb as f64 / ob as f64) * 100.0
            } else {
                0.0
            };
            out.push_str(&"-".repeat(80));
            out.push_str(&format!("\nCase {} - P4 {} ({})\n", id, fb, fnm));
            out.push_str(&"-".repeat(80));
            out.push_str(&format!("\nOriginal: {} lines, {} bytes  |  Compact: {} lines, {} bytes  |  Compression: {:.1}%\n", ol, ob, cl, cb, ratio));
            out.push_str(&format!(
                "AI Profile: {}  |  Path tokens: {}\n",
                prof,
                compacted.matches("$P").count()
            ));
            out.push_str("-- Case text --\n");
            out.push_str(&"-".repeat(80));
            out.push_str("\n");
            out.push_str(&raw);
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("-- Compact Output (full) --\n");
            out.push_str(&"-".repeat(80));
            out.push_str("\n");
            out.push_str(&compacted);
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }
        let op = std::path::Path::new(manifest_dir)
            .join("target")
            .join("vcs_p4_compact_showcase_report.txt");
        std::fs::write(&op, &out).unwrap();
        eprintln!("\n[P4 Showcase] {}", op.display());
    }
}
