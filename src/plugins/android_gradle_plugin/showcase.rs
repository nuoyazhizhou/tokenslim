#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::{Slice, SliceType};
    use crate::plugins::android_gradle_plugin::AndroidGradlePlugin;
    use std::borrow::Cow;

    fn read_sample(file_name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("android_gradle_plugin")
            .join(file_name);
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    fn compress_text(plugin: &AndroidGradlePlugin, text: &str) -> String {
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
    fn generate_android_gradle_showcase_report() {
        let plugin = AndroidGradlePlugin::new();
        let cases = [
            ("case_001_gradle_build", "Gradle 标准构建"),
            ("case_002_gradle_task", "Gradle 任务执行"),
            ("case_003_gradle_noise", "Gradle 噪音夹杂"),
            ("case_004_gradle_long_line", "Gradle 超长行"),
            ("case_005_gradle_single", "Gradle 单行输入"),
            ("case_006_gradle_empty", "Gradle 空输入"),
            ("case_007_gradle_resource_warn", "资源警告压缩"),
            ("case_008_gradle_jenkins", "Jenkins 环境变量"),
            ("case_009_gradle_d8", "D8 编译消息"),
            ("case_010_gradle_apk", "APK 签名信息"),
            ("case_011_gradle_mixed", "混合场景"),
            ("case_012_gradle_no_compress", "不压缩场景"),
            ("case_013_gradle_generic_build", "通用 Gradle 构建"),
            ("case_014_gradle_dependency_download", "Gradle 依赖下载"),
            ("case_015_gradle_daemon_failure", "Gradle daemon 失败"),
            (
                "case_016_github_actions_gradle_test_failure",
                "GitHub Actions Gradle test failure",
            ),
            (
                "case_017_gitlab_gradle_wrapper_failure",
                "GitLab Gradle wrapper failure",
            ),
            (
                "case_018_gradle_ci_cache",
                "Gradle CI cache/download summary",
            ),
            (
                "case_019_gradle_connected_android_test_failure",
                "connectedAndroidTest failure",
            ),
            (
                "case_020_gradle_kotlin_compile_failure",
                "Kotlin compile failure in CI",
            ),
            (
                "case_021_gradle_build_scan_warning",
                "Gradle build scan warning",
            ),
            (
                "case_022_gradle_multi_project_failure",
                "Multi-project Gradle failure",
            ),
        ];

        let mut all_output = String::new();
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n  Android/Gradle AI Compact Showcase\n");
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
                .join("android_gradle_compact_showcase_report.txt"),
            &all_output,
        )
        .unwrap();
    }
}
