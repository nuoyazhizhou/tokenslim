#[cfg(test)]
mod tests {
    use super::super::methods::*;
    use crate::core::path_compressor::types::PathCompressor;

    /// 测试：遍历 p4 样例生成 showcase 对比报告并写入 target 目录。
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
            // 法则 P4-1: 对压缩结果中的 depot 路径执行字典压缩。
            // 本地构造 PathCompressor（与 compress_depot_paths 相同的 min_prefix_length / min_occurrences）
            // 得到与线上完全一致的 compact，同时保留同一实例用于提取 `$Pn` token -> 原 depot 路径 的词典，
            // 确保每个 case 的词典与写进报告的 compact 一一对应。
            let mut compressor = PathCompressor::new();
            compressor.set_min_prefix_length(10); // P4 路径前缀通常较短（如 //depot/main/ 14 chars）
            compressor.set_min_occurrences(2);
            let compacted = compressor.extract_and_compress_from_text(&compacted_raw);
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
            // 字典侧通道：从该 case 专属的 PathCompressor 提取 token->原文 映射并序列化为 JSON，
            // 仅保留 compact 中实际引用的 `$Pn` token；用 BTreeMap 保证序列化序稳定、报告可复现，
            // 不改变上方 compact 段内容（哈希不变）。
            let prefix_map = compressor.get_prefix_map();
            let mut dict_map: std::collections::BTreeMap<String, String> =
                std::collections::BTreeMap::new();
            for (token, path) in prefix_map {
                if compacted.contains(token) {
                    dict_map.insert(token.clone(), path.clone());
                }
            }
            let dict_json = if dict_map.is_empty() {
                String::new()
            } else {
                serde_json::to_string(&dict_map).unwrap_or_default()
            };
            if !dict_json.is_empty() {
                out.push_str("-- Dictionary (full) --\n");
                out.push_str(&"-".repeat(80));
                out.push('\n');
                out.push_str(&dict_json);
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
