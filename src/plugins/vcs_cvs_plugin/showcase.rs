#[cfg(test)]
mod tests {
    use super::super::methods::*;
    #[test]
    fn generate_vcs_cvs_showcase_report() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let dir = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("vcs_cvs_plugin");
        let cases: &[(&str, &str, &str)] = &[
            ("case_27", "cvs_log", "log"),
            ("case_36", "cvs_status", "status"),
            ("case_37", "cvs_diff", "diff"),
            ("case_101", "cvs_update", "status"),
            ("case_102", "cvs_commit", "log"),
            ("case_146", "cvs_tag", "log"),
            ("case_147", "cvs_edit", "status"),
            ("case_187", "cvs_checkout", "log"),
            ("case_188", "cvs_annotate", "other"),
            ("case_189", "cvs_unedit", "log"),
            ("case_190", "cvs_history", "log"),
            ("case_313", "cvs_diff_c", "diff"),
            ("case_314", "cvs_update_d", "status"),
            ("case_315", "cvs_status_v", "status"),
        ];
        let mut out = String::new();
        out.push_str(&"=".repeat(80));
        out.push_str("\n  VCS CVS AI Compact Showcase - Detailed Case-by-Case Report\n");
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
            let compacted = match *prof {
                "status" => compact_cvs_status_for_ai(&raw),
                "diff" => compact_cvs_diff_for_ai(&raw),
                "log" => compact_cvs_log_family_for_ai(&raw),
                "other" => compact_cvs_other_for_ai(&raw),
                _ => raw.clone(),
            };
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
            out.push_str(&format!("\nCase {} - CVS {} ({})\n", id, fb, fnm));
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
            .join("vcs_cvs_compact_showcase_report.txt");
        std::fs::write(&op, &out).unwrap();
        eprintln!("\n[CVS Showcase] {}", op.display());
    }
}
