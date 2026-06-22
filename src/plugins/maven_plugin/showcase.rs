#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::{Slice, SliceType};
    use crate::plugins::maven_plugin::MavenPlugin;
    use std::borrow::Cow;

    const SHOWCASE_CASES: &[(&str, &str)] = &[
        ("case_001_build_success.log", "构建成功"),
        ("case_002_build_error.log", "构建错误"),
        ("case_003_test_failure.log", "测试失败"),
        ("case_004_clean.log", "清理"),
        ("case_005_dependency_tree.log", "依赖树"),
        ("case_006_noise.log", "带噪声"),
        ("case_007_single_line.log", "单行输出"),
        ("case_008_empty.log", "空输入"),
        ("case_009_special_chars.log", "特殊字符"),
        ("case_010_mixed.log", "混合场景"),
        ("case_011_no_compress.log", "不压缩场景"),
        ("case_012_long.log", "长输出"),
        ("case_013_javac_warnings.log", "Javac 警告/错误"),
        ("case_014_junit_tests.log", "JUnit 测试输出"),
        ("case_015_dependencies.log", "依赖下载"),
        ("case_016_build_summary.log", "构建摘要"),
    ];

    fn read_sample(file_name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("maven_plugin")
            .join(file_name);
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    fn compress_text(plugin: &MavenPlugin, text: &str) -> String {
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
    fn generate_maven_showcase_report() {
        let plugin = MavenPlugin::new();

        let mut all_output = String::new();
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n  Maven AI Compact Showcase\n");
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n\n");

        for (file_name, title) in SHOWCASE_CASES {
            let raw = read_sample(file_name);

            if raw.is_empty() {
                all_output.push_str(&format!(
                    "[SKIP] {} - file not found or empty: {}\n\n",
                    title, file_name
                ));
                continue;
            }

            let case_id = file_name.trim_end_matches(".log");
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
                .join("maven_compact_showcase_report.txt"),
            &all_output,
        )
        .unwrap();
    }

    #[test]
    fn showcase_catalog_keeps_enhancement_cases() {
        let case_ids: Vec<&str> = SHOWCASE_CASES
            .iter()
            .map(|(file_name, _)| file_name.trim_end_matches(".log"))
            .collect();

        for expected in [
            "case_013_javac_warnings",
            "case_014_junit_tests",
            "case_015_dependencies",
            "case_016_build_summary",
        ] {
            assert!(
                case_ids.contains(&expected),
                "Maven showcase must include enhanced sample {}",
                expected
            );
        }
    }
}
