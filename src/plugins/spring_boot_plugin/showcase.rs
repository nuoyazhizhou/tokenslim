#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::{Slice, SliceType};
    use crate::plugins::spring_boot_plugin::SpringBootPlugin;
    use std::borrow::Cow;

    /// 测试辅助：读取 samples/spring_boot_plugin 目录下的样例文件。
    fn read_sample(file_name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("spring_boot_plugin")
            .join(file_name);
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    /// 测试辅助：构造 Slice 并调用插件 compress，拼接 Text token 得到压缩文本；
    /// 同时返回压缩过程中产生的字典快照 JSON（`$PK` 包 / `$P` 路径 / `$M` 宏 等映射），供审计侧通道携带。
    fn compress_text(plugin: &SpringBootPlugin, text: &str) -> (String, String) {
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
        let compacted = result
            .tokens
            .iter()
            .filter_map(|t| match t {
                Token::Text(s) => Some(s.as_ref()),
                _ => None,
            })
            .collect::<String>();
        // 包/路径/宏/目录/命令 token 合并进快照映射；用 BTreeMap 保证序列化序稳定，便于审计报告可复现。
        // spring_boot 会把 logger（`$PK`）、Maven 下载 URL 前缀（`$P`）、Bean 名（`$M`）等登记为 token，
        // 映射存于快照 packages/paths/macros/directories/flags。
        let snap = dict.snapshot();
        let mut merged: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        merged.extend(snap.packages.iter().map(|(k, v)| (k.clone(), v.clone())));
        merged.extend(snap.paths.iter().map(|(k, v)| (k.clone(), v.clone())));
        merged.extend(snap.macros.iter().map(|(k, v)| (k.clone(), v.clone())));
        merged.extend(snap.directories.iter().map(|(k, v)| (k.clone(), v.clone())));
        merged.extend(snap.flags.iter().map(|(k, v)| (k.clone(), v.clone())));
        let dict_json = if merged.is_empty() {
            String::new()
        } else {
            let btree: std::collections::BTreeMap<String, String> = merged.into_iter().collect();
            serde_json::to_string(&btree).unwrap_or_default()
        };
        (compacted, dict_json)
    }

    /// 测试：遍历样例生成 spring_boot 插件的 showcase 对比报告并写入 target 目录。
    #[test]
    fn generate_spring_boot_showcase_report() {
        let plugin = SpringBootPlugin::new();
        let cases = [
            ("case_001_simple_error.log", "简单错误"),
            ("case_002_stacktrace.log", "堆栈跟踪"),
            ("case_003_info_log.log", "信息日志"),
            ("case_004_empty.log", "空内容"),
            ("case_005_single_line.log", "单行"),
            ("case_006_noise.log", "带噪声"),
            ("case_007_special_chars.log", "特殊字符"),
            ("case_008_no_compress.log", "不压缩"),
            ("case_009_mixed.log", "混合内容"),
            ("case_010_sql_error.log", "SQL错误"),
            ("case_011_config_error.log", "配置错误"),
            ("case_012_complex.log", "复杂场景"),
            ("case_013_real_spring_single_line.log", "真实Spring单行"),
            ("case_014_maven_download.log", "Maven下载"),
            (
                "case_015_looks_spring_but_log4j_json.log",
                "伪Spring-Log4jJSON",
            ),
        ];

        let mut all_output = String::new();
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n  Spring Boot AI Compact Showcase\n");
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

            // 字典侧通道：仅在压缩产生 `$PK`/`$P`/`$M` 等 token 时携带 token->原文 映射，
            // 供审计产物隔离到 compact.txt 后仍可逆解析，不改变上方 compact 段内容（哈希不变）。
            if !compacted.1.is_empty() {
                all_output.push_str("-- Dictionary (full) --\n");
                all_output.push_str(&"-".repeat(80));
                all_output.push_str("\n");
                all_output.push_str(&compacted.1);
                all_output.push('\n');
            }
        }

        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("spring_boot_compact_showcase_report.txt"),
            &all_output,
        )
        .unwrap();
    }
}
