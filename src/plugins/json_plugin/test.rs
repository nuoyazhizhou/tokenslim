//! json_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::json_plugin::types::JsonPlugin;
    use crate::plugins::test_utils::*;

    #[test]
    fn detects_nested_json_sample_with_high_confidence() {
        let plugin = JsonPlugin::new();
        let raw = read_sample_file("json_plugin", "case_002_nested.json");
        let score = plugin.detect(&make_test_slice(&raw, SliceType::Unknown));
        assert!(score.is_some(), "嵌套 JSON 样本应命中 detect");
        assert!(score.unwrap() > 0.5);
    }

    /// 验证「压缩后再解压缩 ≡ 原始 JSON（语义等价）」。
    #[test]
    fn compress_then_decompress_is_semantically_identical_for_nested_sample() {
        let plugin = JsonPlugin::new();
        let raw = read_sample_file("json_plugin", "case_002_nested.json");
        let (compressed, dict_engine) = compress_with_dict(&plugin, &raw, SliceType::Unknown);
        assert!(
            compressed.starts_with("$JSON|{"),
            "JSON 压缩输出应以 $JSON|{{ 开头: {compressed}"
        );
        let dict = dict_engine.snapshot();
        let decompressed = plugin.decompress(&compressed, &dict);
        let parsed_orig: serde_json::Value =
            serde_json::from_str(&raw).expect("原样本应为合法 JSON");
        let parsed_decompressed: serde_json::Value =
            serde_json::from_str(&decompressed).expect("解压输出应为合法 JSON");
        assert_eq!(parsed_orig, parsed_decompressed);
    }

    #[test]
    fn detects_noisy_wrapper_sample() {
        let plugin = JsonPlugin::new();
        let raw = read_sample_file("json_plugin", "case_006_noise.log");
        assert!(plugin
            .detect(&make_test_slice(&raw, SliceType::Unknown))
            .is_some());
    }
}
