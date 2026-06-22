//! webpack_vite_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::test_utils::*;
    use crate::plugins::webpack_vite_plugin::WebpackVitePlugin;
    #[test]
    fn detects_vite_dev_server_sample() {
        let plugin = WebpackVitePlugin::new();
        let raw = read_sample_log("webpack_vite_plugin", "case_004_dev_server");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "Vite dev server 样本（包含 `VITE v` / `ready in`）应命中 detect"
        );
    }

    #[test]
    fn detects_webpack_complex_sample() {
        let plugin = WebpackVitePlugin::new();
        let raw = read_sample_log("webpack_vite_plugin", "case_012_complex");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "Webpack 复杂样本（包含 `Hash: ` / `Version: webpack`）应命中 detect"
        );
    }

    #[test]
    fn compresses_complex_sample_without_expansion() {
        let plugin = WebpackVitePlugin::new();
        let raw = read_sample_log("webpack_vite_plugin", "case_012_complex");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len() + 4,
            "webpack_vite 插件压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn preserves_distinct_warning_messages() {
        let plugin = WebpackVitePlugin::new();
        let raw = read_sample_log("webpack_vite_plugin", "case_008_special_chars");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        for signal in ["console.log", "process.env.NODE_ENV", "require.ensure"] {
            assert!(
                out.contains(signal),
                "webpack_vite compact must preserve warning signal `{signal}`; output: {out}"
            );
        }
        assert!(
            out.contains("3 warnings") || !out.contains("[BUILD_SUMMARY]"),
            "warning summary must not undercount distinct warnings; output: {out}"
        );
    }
}
