//! yaml_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::test_utils::*;
    use crate::plugins::yaml_plugin::types::YamlPlugin;

    #[test]
    fn detects_simple_yaml_sample_with_high_confidence() {
        let plugin = YamlPlugin::new();
        let raw = read_sample_log("yaml_plugin", "case_001_simple_yaml");
        let score = plugin.detect(&make_test_slice(&raw, SliceType::Unknown));
        assert!(score.is_some());
        assert!(score.unwrap() > 0.5);
    }

    #[test]
    fn detects_kubernetes_manifest_sample() {
        let plugin = YamlPlugin::new();
        let raw = read_sample_log("yaml_plugin", "case_004_kubernetes");
        assert!(plugin
            .detect(&make_test_slice(&raw, SliceType::Unknown))
            .is_some());
    }

    /// 验证 YAML 压缩 + 解压后语义等价（用 serde_yaml 做结构比较，容忍空白顺序差异）。
    #[test]
    fn compress_then_decompress_is_semantically_identical_for_kubernetes_sample() {
        let plugin = YamlPlugin::new();
        let raw = read_sample_log("yaml_plugin", "case_004_kubernetes");
        let (compressed, dict_engine) = compress_with_dict(&plugin, &raw, SliceType::Unknown);
        assert!(
            compressed.starts_with("$YAML|"),
            "YAML 压缩输出应以 $YAML| 开头: {compressed}"
        );
        let dict = dict_engine.snapshot();
        let decompressed = plugin.decompress(&compressed, &dict);
        let parsed_orig: serde_yaml::Value =
            serde_yaml::from_str(&raw).expect("原样本应为合法 YAML");
        let parsed_decompressed: serde_yaml::Value =
            serde_yaml::from_str(&decompressed).expect("解压输出应为合法 YAML");
        assert_eq!(parsed_orig, parsed_decompressed);
    }

    #[test]
    fn does_not_detect_json_that_happens_to_parse_as_yaml() {
        let plugin = YamlPlugin::new();
        let raw = read_sample_log("yaml_plugin", "case_013_looks_yaml_but_json");
        assert!(
            plugin
                .detect(&make_test_slice(&raw, SliceType::Unknown))
                .is_none(),
            "yaml 不应把 JSON 文本误识别为 YAML"
        );
    }

    #[test]
    fn does_not_detect_dockerfile_as_yaml() {
        let plugin = YamlPlugin::new();
        let raw = read_sample_log("yaml_plugin", "case_014_looks_yaml_but_dockerfile");
        assert!(
            plugin
                .detect(&make_test_slice(&raw, SliceType::Unknown))
                .is_none(),
            "yaml 不应把 Dockerfile 误识别为 YAML"
        );
    }
}
