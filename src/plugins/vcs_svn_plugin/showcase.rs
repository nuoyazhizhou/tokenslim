#[cfg(test)]
mod tests {
    use super::super::methods::*;
    #[test]
    fn generate_vcs_svn_showcase_report() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let dir = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("vcs_svn_plugin");
        let cases: &[(&str, &str, &str)] = &[
            ("case_01", "svn_status", "status"),
            ("case_02", "svn_diff", "diff"),
            ("case_03", "svn_log", "log"),
            ("case_04", "svn_blame", "other"),
            ("case_05", "svn_list", "other"),
            ("case_06", "svn_propget", "other"),
            ("case_07", "svn_info", "other"),
            ("case_70", "svn_update", "status"),
            ("case_71", "svn_commit", "log"),
            ("case_72", "svn_merge", "log"),
            ("case_73", "svn_switch", "status"),
            ("case_74", "svn_relocate", "log"),
            ("case_75", "svn_lock", "log"),
            ("case_76", "svn_unlock", "log"),
            ("case_77", "svn_revert", "status"),
            ("case_78", "svn_cleanup", "status"),
            ("case_79", "svn_resolve", "status"),
            ("case_80", "svn_export", "status"),
            ("case_81", "svn_import", "log"),
            ("case_137", "svn_add", "status"),
            ("case_138", "svn_delete", "status"),
            ("case_139", "svn_copy", "status"),
            ("case_140", "svn_move", "status"),
            ("case_141", "svn_mkdir", "status"),
            ("case_176", "svn_cat", "other"),
            ("case_177", "svn_proplist", "other"),
            ("case_178", "svn_propset", "log"),
            ("case_225", "svn_status_v", "status"),
            ("case_226", "svn_diff_c", "diff"),
            ("case_227", "svn_diff_git_diff", "diff"),
            ("case_228", "svn_info_xml", "other"),
            ("case_229", "svn_list_verbose", "other"),
            ("case_230", "svn_list_depth", "other"),
            ("case_242", "svn_revert_dir", "status"),
            ("case_243", "svn_revert_cwd", "status"),
            ("case_244", "svn_resolve_recursive", "status"),
            ("case_245", "svn_export_revision", "log"),
            ("case_262", "svn_st", "status"),
            ("case_263", "svn_status_quiet", "status"),
            ("case_264", "svn_status_no_ignore", "status"),
            ("case_265", "svn_status_path", "status"),
            ("case_266", "svn_status_update", "status"),
            ("case_267", "svn_log_limit", "log"),
            ("case_268", "svn_log_verbose", "log"),
            ("case_269", "svn_log_quiet", "log"),
            ("case_270", "svn_log_path", "log"),
            ("case_271", "svn_diff_revision", "diff"),
            ("case_272", "svn_diff_summarize", "diff"),
            ("case_273", "svn_diff_path", "diff"),
            ("case_316", "svn_info_show_item", "other"),
            ("case_317", "svn_update_r", "status"),
            // Phase 2 P2 增强功能 - 新增 3 个 case
            ("case_318", "svn_merge_conflict", "merge"),
            ("case_319", "svn_update_detailed", "update"),
            ("case_320", "svn_annotate_long", "annotate"),
        ];
        let mut out = String::new();
        out.push_str(&"=".repeat(80));
        out.push_str("\n  VCS SVN AI Compact Showcase - Detailed Case-by-Case Report\n");
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
                "status" => compact_svn_status_for_ai(&raw),
                "diff" => compact_svn_diff_for_ai(&raw),
                "log" => {
                    if fb.contains("commit") {
                        compact_svn_commit_for_ai(&raw)
                    } else {
                        compact_svn_log_for_ai(&raw)
                    }
                }
                "merge" => compact_svn_merge_enhanced(&raw),
                "update" => compact_svn_update_enhanced(&raw),
                "annotate" => compact_svn_annotate_enhanced(&raw),
                "other" => compact_svn_other_for_ai(&raw),
                _ => raw.clone(),
            };
            // 法则 A: 对压缩结果中的路径执行字典压缩
            let compacted = compress_svn_paths(&compacted_raw);
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
            out.push_str(&format!("\nCase {} - SVN {} ({})\n", id, fb, fnm));
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
            .join("vcs_svn_compact_showcase_report.txt");
        std::fs::write(&op, &out).unwrap();
        eprintln!("\n[SVN Showcase] {}", op.display());
    }
}
