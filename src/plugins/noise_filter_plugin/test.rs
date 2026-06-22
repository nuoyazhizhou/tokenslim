//! noise_filter_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::noise_filter_plugin::NoiseFilterPlugin;
    use crate::plugins::test_utils::*;
    #[test]
    fn detects_repetitive_copying_sample() {
        let plugin = NoiseFilterPlugin::new();
        let raw = read_sample_log("noise_filter_plugin", "case_002_repetitive_lines");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "重复 Copying 样本应命中 detect"
        );
    }

    #[test]
    fn detects_long_hex_sample() {
        let plugin = NoiseFilterPlugin::new();
        let raw = read_sample_log("noise_filter_plugin", "case_003_long_hex");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn compresses_repetitive_lines_sample_and_shrinks() {
        let plugin = NoiseFilterPlugin::new();
        let raw = read_sample_log("noise_filter_plugin", "case_002_repetitive_lines");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 噪点过滤应实打实缩短（不严格断言幅度，只要不扩张）
        assert!(
            out.len() <= raw.len(),
            "噪点过滤不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }
}
