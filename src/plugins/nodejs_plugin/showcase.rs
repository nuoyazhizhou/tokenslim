#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::{Slice, SliceType};
    use crate::plugins::nodejs_plugin::NodeJsPlugin;
    use crate::plugins::test_utils::full_dict_json;
    use std::borrow::Cow;

    /// 测试辅助：读取 samples/nodejs_plugin 目录下的样例文件。
    fn read_sample(file_name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir)
            .join("samples")
            .join("nodejs_plugin")
            .join(file_name);
        std::fs::read_to_string(&path).unwrap_or_default()
    }

    /// 测试辅助：构造 Slice 并调用插件 compress，拼接 Text token 得到压缩文本；
    /// 同时返回压缩过程中产生的可逆 token->原文 字典快照 JSON，供审计侧通道携带。
    fn compress_text(plugin: &NodeJsPlugin, text: &str) -> (String, String) {
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
        // 字典侧通道：截取压缩词典快照为 token->原文 JSON，仅保留 compact 中实际引用的映射。
        let dict_json = full_dict_json(&dict, &compacted);
        (compacted, dict_json)
    }

    /// 测试：遍历样例生成 nodejs 插件的 showcase 对比报告并写入 target 目录。
    #[test]
    fn generate_nodejs_showcase_report() {
        let plugin = NodeJsPlugin::new();
        let cases = [
            ("case_001_simple_error.log", "简单错误"),
            ("case_002_node_modules.log", "Node模块路径"),
            ("case_003_npm_install.log", "NPM安装"),
            ("case_004_typescript.log", "TypeScript错误"),
            ("case_005_webpack.log", "Webpack打包"),
            ("case_006_noise.log", "带噪声"),
            ("case_007_single_line.log", "单行日志"),
            ("case_008_empty.log", "空日志"),
            ("case_009_special_chars.log", "特殊字符"),
            ("case_010_mixed.log", "混合场景"),
            ("case_011_no_compress.log", "不压缩场景"),
            ("case_012_long_stack.log", "长堆栈"),
            ("case_012_npm_install.log", "NPM 安装依赖"),
            ("case_013_tsc_compile.log", "TypeScript 编译"),
            ("case_014_eslint.log", "ESLint 输出"),
            ("case_015_webpack.log", "Webpack 输出"),
            ("case_016_jest.log", "Jest 测试"),
            ("case_017_pnpm_install_ci.log", "pnpm install in CI"),
            ("case_018_yarn_install_ci.log", "Yarn install in CI"),
            ("case_019_pnpm_jest_ci_failure.log", "pnpm Jest CI failure"),
            ("case_020_yarn_tsc_ci_failure.log", "Yarn tsc CI failure"),
            (
                "case_021_npm_ci_webpack_failure.log",
                "npm webpack CI failure",
            ),
            ("case_022_pnpm_eslint_ci.log", "pnpm ESLint CI failure"),
            ("case_023_vitest_pass.log", "vitest 全通过"),
            ("case_024_vitest_fail.log", "vitest 部分失败"),
        ];

        let mut all_output = String::new();
        all_output.push_str(&"=".repeat(80));
        all_output.push_str("\n  Node.js AI Compact Showcase\n");
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
            let (compacted, dict_json) = compress_text(&plugin, &raw);
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

            // 字典侧通道：仅在压缩产生可逆 token（`$PK/$P/$D/$M/$C/$FL`）时，追加
            // token->原文 JSON 映射段，供审计解析；不改变上方 compact 段内容（哈希不变）。
            if !dict_json.is_empty() {
                all_output.push_str("-- Dictionary (full) --\n");
                all_output.push_str(&"-".repeat(80));
                all_output.push_str("\n");
                all_output.push_str(&dict_json);
                all_output.push('\n');
            }
        }

        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("nodejs_compact_showcase_report.txt"),
            &all_output,
        )
        .unwrap();
    }
}
