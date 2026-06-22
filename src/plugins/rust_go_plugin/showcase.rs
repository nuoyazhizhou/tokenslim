#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::{Slice, SliceType};
    use crate::plugins::rust_go_plugin::RustGoPlugin;
    use std::borrow::Cow;

    fn read_sample(file_name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("rust_go_plugin")
            .join(file_name);
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    fn compress_text(plugin: &RustGoPlugin, text: &str) -> String {
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
    fn generate_rust_go_showcase_report() {
        let plugin = RustGoPlugin::new();
        let cases = [
            ("case_001_rust_warning", "Rust 编译警告"),
            ("case_002_go_panic", "Go Panic 堆栈"),
            ("case_003_rust_error", "Rust 编译错误"),
            ("case_004_go_stack", "Go 完整堆栈"),
            ("case_005_rust_noise", "Rust 噪音夹杂"),
            ("case_006_go_noise", "Go 噪音夹杂"),
            ("case_007_rust_long_line", "Rust 超长行"),
            ("case_008_go_short", "Go 简短输出"),
            ("case_009_rust_single", "Rust 单行错误"),
            ("case_010_go_mixed", "Go 混合场景"),
            ("case_011_rust_no_compress", "Rust 不压缩场景"),
            ("case_012_go_no_compress", "Go 不压缩场景"),
            ("case_013_looks_rust_but_python", "Rust 负样本：Python"),
            ("case_014_looks_go_but_java", "Go 负样本：Java"),
            ("case_015_cargo_compiling", "Cargo 编译进度"),
            ("case_016_error_code_stats", "错误码统计"),
            ("case_017_cargo_test", "Cargo 测试"),
            ("case_018_go_test", "Go 测试"),
        ];

        let mut all_output = String::new();
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n  Rust/Go AI Compact Showcase\n");
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
                .join("rust_go_compact_showcase_report.txt"),
            &all_output,
        )
        .unwrap();
    }
}
