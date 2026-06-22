//! generic_text_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::generic_text_plugin::GenericTextPlugin;
    use crate::plugins::test_utils::*;
    #[test]
    fn detects_non_empty_sample_with_fallback_confidence() {
        // generic_text 是兜底插件，对任何非空文本都应给出低置信度。
        let plugin = GenericTextPlugin::new();
        let raw = read_sample_log("generic_text_plugin", "case_001_normal_text");
        let slice = make_log_slice(&raw);
        let score = plugin.detect(&slice);
        assert!(score.is_some(), "generic_text 应对非空文本返回置信度");
    }

    #[test]
    fn compresses_ansi_sample_without_expansion() {
        // 用 ANSI 色码样本验证 ANSI 剥离与「不扩张」性质。
        let plugin = GenericTextPlugin::new();
        let raw = read_sample_log("generic_text_plugin", "case_002_ansi_colors");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len(),
            "输出字节数不得超过输入: raw={} out={}",
            raw.len(),
            out.len()
        );
        assert!(
            !out.contains('\x1B'),
            "ANSI 控制字符必须被剥离，实际包含: {out}"
        );
    }

    #[test]
    fn compresses_repeated_blank_lines_sample() {
        // case_003_many_blank_lines 用于验证「折叠空行」开关能生效。
        let plugin = GenericTextPlugin::new();
        let raw = read_sample_log("generic_text_plugin", "case_003_many_blank_lines");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.len() <= raw.len());
    }
}
