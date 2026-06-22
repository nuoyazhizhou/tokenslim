#[cfg(test)]
mod tests {
    use super::super::methods::*;
    #[test]
    fn generate_showcase_report() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let dir = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("vcs_gh_plugin");
        let cases: &[&str] = &[
            "case_92_gh_pr_list",
            "case_93_gh_issue_list",
            "case_98_gh_pr_view",
            "case_99_gh_run_list",
            "case_106_gh_api",
            "case_107_gh_auth",
            "case_155_gh_pr_create",
            "case_156_gh_pr_merge",
            "case_157_gh_issue_create",
            "case_158_gh_issue_view",
            "case_162_gh_run_view",
            "case_197_gh_run_view",
            "case_198_gh_repo_list",
            "case_199_gh_repo_view",
            "case_200_gh_gist_list",
            "case_201_gh_gist_view",
            "case_202_gh_actions_list",
            "case_203_gh_actions_view",
            "case_204_gh_secret_list",
            "case_205_gh_deploy_list",
        ];
        let mut out = String::new();
        out.push_str(&"=".repeat(80));
        out.push_str("\n  VCS GH AI Compact Showcase - Detailed Case-by-Case Report\n");
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
            let compacted = compact_gh_log_for_ai(&raw);
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
            out.push_str(&format!("\nCase {} - GH ({})\n", case_name, fnm));
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
            .join("vcs_gh_compact_showcase_report.txt");
        std::fs::write(&op, &out).unwrap();
        eprintln!("\n[GH Showcase] {}", op.display());
    }
}
