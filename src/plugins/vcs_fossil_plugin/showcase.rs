#[cfg(test)]
mod tests {
    use super::super::methods::*;
    #[test]
    fn generate_vcs_fossil_showcase_report() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let dir = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("vcs_fossil_plugin");
        let cases: &[(&str, &str, &str)] = &[
            ("case_29", "fossil_status", "status"),
            ("case_40", "fossil_diff", "diff"),
            ("case_41", "fossil_log", "log"),
            ("case_104", "fossil_timeline", "log"),
            ("case_152", "fossil_changes", "status"),
            ("case_153", "fossil_undo", "log"),
            ("case_194", "fossil_stash", "log"),
            ("case_195", "fossil_merge", "log"),
            ("case_196", "fossil_sync", "log"),
            ("case_320", "fossil_diff_brief", "diff"),
        ];
        let mut out = String::new();
        out.push_str(&"=".repeat(80));
        out.push_str("\n  VCS Fossil AI Compact Showcase - Detailed Case-by-Case Report\n");
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
                "status" => compact_fossil_status_for_ai(&raw),
                "diff" => compact_fossil_diff_for_ai(&raw),
                "log" => compact_fossil_log_family_for_ai(&raw),
                "other" => compact_fossil_other_for_ai(&raw),
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
            out.push_str(&format!("\nCase {} - Fossil {} ({})\n", id, fb, fnm));
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
            .join("vcs_fossil_compact_showcase_report.txt");
        std::fs::write(&op, &out).unwrap();
        eprintln!("\n[Fossil Showcase] {}", op.display());
    }
}
