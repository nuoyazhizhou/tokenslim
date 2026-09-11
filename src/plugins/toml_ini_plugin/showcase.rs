#[cfg(test)]
mod tests {
    use crate::core::text_slicer::SliceType;
    use crate::plugins::test_utils::{compress_with_dict, full_dict_json};
    use crate::plugins::toml_ini_plugin::TomlIniPlugin;

    /// showcase 注册表：审计 `parse_showcase_rs_cases` 据此将物理样本绑定到展示用例。
    pub const SHOWCASE_CASES: &[(&str, &str)] = &[
        ("case_001_pyproject_toml", "pyproject.toml"),
        ("case_002_agent_config_ini", "CI agent 配置"),
        ("case_003_flask_ini", "Flask 配置"),
    ];

    /// 测试辅助：读取 samples/toml_ini_plugin 目录下的样例文件（case_id 不含扩展名）。
    fn read_sample(case_id: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("toml_ini_plugin")
            .join(format!("{}.log", case_id));
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    /// 测试辅助：构造 Slice 并调用插件 compress，拼接 Text token 得到压缩文本；
    /// 同时返回压缩过程中产生的字典 JSON，供审计 sidecar 侧通道携带。
    fn compress_text(plugin: &TomlIniPlugin, text: &str) -> (String, String) {
        let (compacted, engine) = compress_with_dict(plugin, text, SliceType::LogBlock);
        let dict_json = full_dict_json(&engine, &compacted);
        (compacted, dict_json)
    }

    /// 测试：遍历样例生成 toml_ini 插件的 showcase 对比报告并写入 target 目录。
    #[test]
    fn generate_toml_ini_showcase_report() {
        let plugin = TomlIniPlugin::new();

        let mut all_output = String::new();
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n  TOML/INI AI Compact Showcase\n");
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n\n");

        for (case_id, title) in SHOWCASE_CASES {
            let raw = read_sample(case_id);
            let file_name = format!("{}.log", case_id);

            if raw.is_empty() {
                all_output.push_str(&format!(
                    "[SKIP] {} - file not found or empty: {}\n\n",
                    title, file_name
                ));
                continue;
            }

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
                .join("toml_ini_compact_showcase_report.txt"),
            &all_output,
        )
        .unwrap();
    }
}
