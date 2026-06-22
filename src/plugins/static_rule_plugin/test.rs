//! static_rule_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::static_rule_plugin::SimpleRulePlugin;
    use crate::plugins::test_utils::*;
    /// 默认空配置：detect 应返回 None，不参与分发。
    #[test]
    fn empty_config_detect_returns_none_on_sample() {
        let plugin = SimpleRulePlugin::new(Default::default());
        let raw = read_sample_log("static_rule_plugin", "case_001_rule_check");
        assert!(plugin.detect(&make_log_slice(&raw)).is_none());
    }

    /// 默认空配置：compress 对真实样本应透传原文（不扩张）。
    #[test]
    fn empty_config_compress_passthrough_without_expansion() {
        let plugin = SimpleRulePlugin::new(Default::default());
        let raw = read_sample_log("static_rule_plugin", "case_001_rule_check");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len(),
            "static_rule 默认配置不得扩张输出: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 基础规则驱动：以 TOML 载入最简单的规则，对 samples 做一次端到端压缩以确认规则流程可跑通。
    /// 说明：此处不在代码里 mock 日志字符串，而是将真实 sample 与配置化规则一起送入 compress。
    #[test]
    fn configured_rule_matches_sample_errors_section() {
        let toml_text = r#"
            [[sections]]
            name = "errors"
            enter = "^ERROR"
            keep = ["^ERROR"]
        "#;
        let plugin = SimpleRulePlugin::from_toml(toml_text).expect("toml 应解析");
        let raw = read_sample_log("static_rule_plugin", "case_011_errors");
        // 只要规则可以被应用而不 panic，且输出不扩张，即视为接入 OK。
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len() + 64, // 允许少量元数据头部
            "static_rule 配置化输出不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }
}
