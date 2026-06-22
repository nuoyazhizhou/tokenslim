//! template_driven_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::template_driven_plugin::{TemplateConfig, TemplateDrivenPlugin};
    use crate::plugins::test_utils::*;
    #[test]
    fn empty_config_detect_returns_none_on_sample() {
        let plugin = TemplateDrivenPlugin::new(TemplateConfig::default());
        let raw = read_sample_log("template_driven_plugin", "case_001_simple_template");
        assert!(plugin.detect(&make_log_slice(&raw)).is_none());
    }

    #[test]
    fn empty_config_compress_does_not_expand_sample() {
        let plugin = TemplateDrivenPlugin::new(TemplateConfig::default());
        let raw = read_sample_log("template_driven_plugin", "case_001_simple_template");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len() + 64,
            "template_driven 空规则不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn empty_config_handles_noise_sample_safely() {
        let plugin = TemplateDrivenPlugin::new(TemplateConfig::default());
        let raw = read_sample_log("template_driven_plugin", "case_006_noise");
        // 关键：不 panic、不扩张
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.len() <= raw.len() + 64);
    }
}
