//! ansi_cleaner_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::ansi_cleaner_plugin::types::AnsiCleanerPlugin;
    use crate::plugins::test_utils::*;
    #[test]
    fn detects_ansi_color_sample() {
        let plugin = AnsiCleanerPlugin::new();
        let raw = read_sample_log("ansi_cleaner_plugin", "case_001_ansi_colors");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn compresses_ansi_colors_sample_strips_escape_and_does_not_expand() {
        let plugin = AnsiCleanerPlugin::new();
        let raw = read_sample_log("ansi_cleaner_plugin", "case_001_ansi_colors");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(!out.contains('\x1B'), "ANSI 转义必须被剥离: {out:?}");
        assert!(
            out.len() <= raw.len(),
            "ANSI 剥离不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn compresses_ansi_mixed_sample_strips_escape() {
        let plugin = AnsiCleanerPlugin::new();
        let raw = read_sample_log("ansi_cleaner_plugin", "case_004_ansi_mixed");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(!out.contains('\x1B'));
    }
}
