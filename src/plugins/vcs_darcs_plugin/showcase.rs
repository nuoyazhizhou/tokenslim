#[cfg(test)]
mod tests {
    use super::super::methods::*;
    #[test]
    fn generate_vcs_darcs_showcase_report() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let dir = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("vcs_darcs_plugin");
        let cases: &[(&str, &str, &str)] = &[
            ("case_35", "darcs_log", "log"),
            ("case_42", "darcs_status", "status"),
            ("case_43", "darcs_diff", "diff"),
            ("case_105", "darcs_record", "log"),
            ("case_154", "darcs_amend", "log"),
            ("case_209", "darcs_whatsnew", "status"),
            ("case_210", "darcs_obliterate", "log"),
            ("case_282", "darcs_rebase", "log"),
            ("case_321", "darcs_log_summary", "log"),
            ("case_322", "darcs_whatsnew_s", "status"),
        ];
        let mut out = String::new();
        out.push_str(&"=".repeat(80));
        out.push_str("\n  VCS Darcs AI Compact Showcase - Detailed Case-by-Case Report\n");
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
                "status" => compact_darcs_status_for_ai(&raw),
                "diff" => compact_darcs_diff_for_ai(&raw),
                "log" => compact_darcs_log_family_for_ai(&raw),
                "other" => compact_darcs_other_for_ai(&raw),
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
            out.push_str(&format!("\nCase {} - Darcs {} ({})\n", id, fb, fnm));
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
            .join("vcs_darcs_compact_showcase_report.txt");
        std::fs::write(&op, &out).unwrap();
        eprintln!("\n[Darcs Showcase] {}", op.display());
    }
}
