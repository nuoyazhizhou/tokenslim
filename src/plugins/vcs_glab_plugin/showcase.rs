#[cfg(test)]
mod tests {
    use super::super::methods::*;
    #[test]
    fn generate_showcase_report() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let dir = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("vcs_glab_plugin");
        let cases: &[&str] = &[
            "case_95_glab_mr_list",
            "case_108_glab_mr_view",
            "case_109_glab_mr_create",
            "case_110_glab_issue_list",
            "case_111_glab_issue_view",
            "case_159_glab_mr_create",
            "case_206_glab_issue_create",
        ];
        let mut out = String::new();
        out.push_str(&"=".repeat(80));
        out.push_str("\n  VCS Glab AI Compact Showcase - Detailed Case-by-Case Report\n");
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
            let compacted = compact_glab_log_for_ai(&raw);
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
            out.push_str(&format!("\nCase {} - Glab ({})\n", case_name, fnm));
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
            .join("vcs_glab_compact_showcase_report.txt");
        std::fs::write(&op, &out).unwrap();
        eprintln!("\n[Glab Showcase] {}", op.display());
    }
}
