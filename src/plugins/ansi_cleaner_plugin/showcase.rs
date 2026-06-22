#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::{Slice, SliceType};
    use crate::plugins::ansi_cleaner_plugin::AnsiCleanerPlugin;
    use std::borrow::Cow;

    fn read_sample(file_name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("ansi_cleaner_plugin")
            .join(file_name);
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    fn compress_text(plugin: &AnsiCleanerPlugin, text: &str) -> String {
        let slice = Slice {
            id: 1,
            text: Cow::Borrowed(text),
            slice_type: SliceType::LogBlock,
            offset: 0,
            line_start: 1,
            line_end: text.lines().count().max(1),
            file_metadata: None,
            flags: Default::default(),
        };
        let mut dict = DictionaryEngine::new();
        let mut dedup = DedupEngine::new(DedupConfig::default());
        let arena = bumpalo::Bump::new();
        let result = plugin.compress(&slice, &mut dict, &mut dedup, &arena);
        result
            .tokens
            .iter()
            .filter_map(|t| match t {
                Token::Text(s) => Some(s.as_ref()),
                _ => None,
            })
            .collect::<String>()
    }

    #[test]
    fn generate_ansi_cleaner_showcase_report() {
        let plugin = AnsiCleanerPlugin::new();
        let cases = [
            ("case_001_ansi_colors", "ANSI 颜色代码"),
            ("case_002_ansi_bold", "ANSI 粗体格式"),
            ("case_003_ansi_cursor", "ANSI 光标控制"),
            ("case_004_ansi_mixed", "混合 ANSI 序列"),
            ("case_005_no_ansi", "无 ANSI 纯文本"),
            ("case_006_ansi_long", "长 ANSI 序列"),
            ("case_007_ansi_empty", "空输入"),
            ("case_008_ansi_single_line", "单行 ANSI"),
            ("case_009_ansi_escape_only", "纯转义序列"),
            ("case_010_ansi_terminal", "终端输出"),
            ("case_011_ansi_noise", "噪音夹杂"),
            ("case_012_ansi_no_compress", "无需压缩"),
        ];

        let mut all_output = String::new();
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n  ANSI Cleaner AI Compact Showcase\n");
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n\n");

        for (case_id, title) in cases {
            let file_name = format!("{}.log", case_id);
            let raw = read_sample(&file_name);

            let original_lines = raw.lines().count();
            let original_bytes = raw.len();
            let compacted = compress_text(&plugin, &raw);
            let compact_lines = if compacted.is_empty() {
                0
            } else {
                compacted.lines().count()
            };
            let compact_bytes = compacted.len();
            let compression_ratio = if original_bytes > 0 {
                (1.0 - compact_bytes as f64 / original_bytes as f64) * 100.0
            } else {
                0.0
            };

            all_output.push_str(&"-".repeat(80));
            all_output.push_str(&format!("\nCase {} - {} ({})\n", case_id, title, file_name));
            all_output.push_str(&"-".repeat(80));
            all_output.push_str(&format!(
                "\nOriginal: {} lines, {} bytes | Compact: {} lines, {} bytes | Compression: {:.1}%\n",
                original_lines, original_bytes, compact_lines, compact_bytes, compression_ratio
            ));

            all_output.push_str("-- Case text --\n");
            all_output.push_str(&"-".repeat(80));
            all_output.push_str("\n");
            all_output.push_str(&raw);
            if !all_output.ends_with('\n') {
                all_output.push('\n');
            }

            all_output.push_str("-- Compact Output (full) --\n");
            all_output.push_str(&"-".repeat(80));
            all_output.push_str("\n");
            all_output.push_str(&compacted);
            if !all_output.ends_with('\n') {
                all_output.push('\n');
            }
        }

        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("ansi_cleaner_compact_showcase_report.txt"),
            &all_output,
        )
        .unwrap();
    }
}
