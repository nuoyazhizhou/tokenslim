//! smart_path_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::smart_path_plugin::SmartPathPlugin;
    use crate::plugins::test_utils::*;
    #[test]
    fn detects_sample_with_paths() {
        let plugin = SmartPathPlugin::new();
        let raw = read_sample_log("smart_path_plugin", "case_001_simple_path");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn compresses_multiple_paths_without_expansion() {
        let plugin = SmartPathPlugin::new();
        let raw = read_sample_log("smart_path_plugin", "case_003_multiple_paths");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len(),
            "smart_path 压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn compresses_windows_path_sample() {
        let plugin = SmartPathPlugin::new();
        let raw = read_sample_log("smart_path_plugin", "case_005_windows_path");
        // 至少要能跑通，且输出不扩张
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.len() <= raw.len());
    }
}
