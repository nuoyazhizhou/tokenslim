#[cfg(test)]
mod tests {
    use super::super::methods::*;
    #[test]
    fn generate_showcase_report() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let dir = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("vcs_az_plugin");
        let cases: &[&str] = &[
            "case_96_az_repos_show",
            "case_112_az_repos_list",
            "case_160_az_repos_create",
            "case_161_az_repos_delete",
            "case_209_az_repos_create_no_url",
            "case_210_az_repos_delete_confirm",
            "case_211_az_repos_generic_error",
            "case_212_az_repos_update_kv",
            "case_221_az_repos_show_ansi_error",
        ];
        let mut out = String::new();
        out.push_str(&"=".repeat(80));
        out.push_str("\n  VCS AZ AI Compact Showcase - Detailed Case-by-Case Report\n");
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
            let compacted = compact_az_log_for_ai(&raw);
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
            out.push_str(&format!("\nCase {} - AZ ({})\n", case_name, fnm));
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
            .join("vcs_az_compact_showcase_report.txt");
        std::fs::write(&op, &out).unwrap();
        eprintln!("\n[AZ Showcase] {}", op.display());
    }
}
