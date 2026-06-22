#[cfg(test)]
mod tests {
    use super::super::methods::*;
    #[test]
    fn generate_showcase_report() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let dir = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("vcs_bitbucket_plugin");
        let cases: &[&str] = &[
            "case_113_bitbucket_pr_list",
            "case_114_bitbucket_pr_view",
            "case_207_bitbucket_pr_create",
            "case_208_bitbucket_issue_list",
            "case_213_bitbucket_pr_list_spacing",
            "case_214_bitbucket_pr_view_multiline_desc",
            "case_215_bitbucket_issue_list_resolved",
            "case_216_bitbucket_generic_alert",
            "case_222_bitbucket_pr_list_relative_time",
        ];
        let mut out = String::new();
        out.push_str(&"=".repeat(80));
        out.push_str("\n  VCS Bitbucket AI Compact Showcase - Detailed Case-by-Case Report\n");
        out.push_str(&"=".repeat(80));
        out.push_str("\n\n");
        for case_name in cases {
            let fnm = format!("{}.log", case_name);
            let fp = dir.join(&fnm);
            if !fp.exists() {
                continue;
            }
            let raw = std::fs::read_to_string(&fp).unwrap_or_default();
            let ol = raw.lines().count();
            let ob = raw.len();
            let compacted = compact_bitbucket_log_for_ai(&raw);
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
            out.push_str(&format!("\nCase {} - Bitbucket ({})\n", case_name, fnm));
            out.push_str(&"-".repeat(80));
            out.push_str(&format!("\nOriginal: {} lines, {} bytes  |  Compact: {} lines, {} bytes  |  Compression: {:.1}%\n", ol, ob, cl, cb, ratio));
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
            .join("vcs_bitbucket_compact_showcase_report.txt");
        std::fs::write(&op, &out).unwrap();
        eprintln!("\n[Bitbucket Showcase] {}", op.display());
    }
}
