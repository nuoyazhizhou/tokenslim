#[cfg(test)]
mod tests {
    use crate::core::text_slicer::SliceType;
    use crate::plugins::test_utils::{compress_with_dict, full_dict_json};
    use crate::plugins::yaml_plugin::YamlPlugin;

    /// 测试辅助：读取 samples/yaml_plugin 目录下的样例文件。
    fn read_sample(file_name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("yaml_plugin")
            .join(file_name);
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    /// 测试辅助：构造 Slice 并调用插件 compress，拼接 Text token 得到压缩文本；
    /// 同时返回压缩过程中产生的可逆 token->原文 字典 JSON（`$PK` 包 / `$P` 路径 / `$D` 目录 等映射），
    /// 供审计 sidecar 侧通道携带。复用 test_utils：compress_with_dict 返回 (compact, DictionaryEngine)，
    /// full_dict_json 生成紧凑的 token->原文 映射；保证 compact 与词典一一对应、不改变 compact 内容。
    fn compress_text(plugin: &YamlPlugin, text: &str) -> (String, String) {
        let (compacted, engine) = compress_with_dict(plugin, text, SliceType::LogBlock);
        let dict_json = full_dict_json(&engine, &compacted);
        (compacted, dict_json)
    }

    /// 测试：遍历样例生成 yaml 插件的 showcase 对比报告并写入 target 目录。
    #[test]
    fn generate_yaml_showcase_report() {
        let plugin = YamlPlugin::new();
        let cases = [
            ("case_001_simple_yaml.log", "简单YAML"),
            ("case_002_complex_yaml.log", "复杂YAML"),
            ("case_003_config.log", "配置文件"),
            ("case_004_kubernetes.log", "K8s配置"),
            ("case_005_empty.log", "空内容"),
            ("case_006_single_line.log", "单行"),
            ("case_007_noise.log", "带噪声"),
            ("case_008_special_chars.log", "特殊字符"),
            ("case_009_no_compress.log", "不压缩"),
            ("case_010_mixed.log", "混合内容"),
            ("case_011_docker_compose.log", "Docker Compose"),
            ("case_012_complex.log", "复杂场景"),
            ("case_013_looks_yaml_but_json.log", "伪YAML-JSON"),
            (
                "case_014_looks_yaml_but_dockerfile.log",
                "伪YAML-Dockerfile",
            ),
        ];

        let mut all_output = String::new();
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n  YAML AI Compact Showcase\n");
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n\n");

        for (file_name, title) in cases {
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
            let compact_lines = if compacted.0.is_empty() {
                0
            } else {
                compacted.0.lines().count()
            };
            let compact_bytes = compacted.0.len();
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
            all_output.push_str(&compacted.0);
            if !all_output.ends_with('\n') {
                all_output.push('\n');
            }

            // 字典侧通道：仅在压缩产生 `$PK`/`$P`/`$D` 等 token 时携带 token->原文 映射，
            // 供审计产物隔离到 compact.txt 后仍可逆解析，不改变上方 compact 段内容（哈希不变）。
            if !compacted.1.is_empty() {
                all_output.push_str("-- Dictionary (full) --\n");
                all_output.push_str(&"-".repeat(80));
                all_output.push('\n');
                all_output.push_str(&compacted.1);
                all_output.push('\n');
            }
        }

        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("yaml_compact_showcase_report.txt"),
            &all_output,
        )
        .unwrap();
    }
}
