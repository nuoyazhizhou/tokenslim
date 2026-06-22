//! xml_html_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::test_utils::*;
    use crate::plugins::xml_html_plugin::types::XmlHtmlPlugin;

    #[test]
    fn detects_simple_xml_sample() {
        let plugin = XmlHtmlPlugin::new();
        let raw = read_sample_log("xml_html_plugin", "case_001_simple_xml");
        let score = plugin.detect(&make_test_slice(&raw, SliceType::Unknown));
        assert!(score.is_some());
        assert!(score.unwrap() > 0.5);
    }

    #[test]
    fn detects_simple_html_sample() {
        let plugin = XmlHtmlPlugin::new();
        let raw = read_sample_log("xml_html_plugin", "case_002_simple_html");
        assert!(plugin
            .detect(&make_test_slice(&raw, SliceType::Unknown))
            .is_some());
    }

    #[test]
    fn compresses_complex_xml_sample_without_expansion() {
        let plugin = XmlHtmlPlugin::new();
        let raw = read_sample_log("xml_html_plugin", "case_003_complex_xml");
        let out = compress_to_string(&plugin, &raw, SliceType::Unknown);
        assert!(
            out.len() <= raw.len() + 16,
            "xml_html 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }
}
