#[cfg(test)]
mod tests {
    use super::super::methods::*;
    #[test]
    fn generate_vcs_bzr_showcase_report() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let dir = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("vcs_bzr_plugin");
        let cases: &[(&str, &str, &str)] = &[
            ("case_28", "bzr_diff", "diff"),
            ("case_38", "bzr_status", "status"),
            ("case_39", "bzr_log", "log"),
            ("case_103", "bzr_pull", "log"),
            ("case_148", "bzr_push", "log"),
            ("case_149", "bzr_merge", "log"),
            ("case_150", "bzr_resolve", "status"),
            ("case_151", "bzr_branch", "log"),
            ("case_191", "bzr_missing", "log"),
            ("case_192", "bzr_revert", "status"),
            ("case_193", "bzr_commit", "log"),
            ("case_318", "bzr_log_show_id", "log"),
            ("case_319", "bzr_status_short", "status"),
        ];
        let mut out = String::new();
        out.push_str(&"=".repeat(80));
        out.push_str("\n  VCS Bzr AI Compact Showcase - Detailed Case-by-Case Report\n");
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
                "status" => compact_bzr_status_for_ai(&raw),
                "diff" => compact_bzr_diff_for_ai(&raw),
                "log" => compact_bzr_log_family_for_ai(&raw),
                "other" => compact_bzr_other_for_ai(&raw),
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
            out.push_str(&format!("\nCase {} - Bzr {} ({})\n", id, fb, fnm));
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
            .join("vcs_bzr_compact_showcase_report.txt");
        std::fs::write(&op, &out).unwrap();
        eprintln!("\n[Bzr Showcase] {}", op.display());
    }
}
