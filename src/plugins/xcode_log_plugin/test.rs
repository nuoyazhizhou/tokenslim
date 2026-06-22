//! xcode_log_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::test_utils::*;
    use crate::plugins::xcode_log_plugin::XcodeLogPlugin;

    #[test]
    fn detects_xcode_case() {
        let plugin = XcodeLogPlugin::new();
        let raw = read_sample_log("xcode_log_plugin", "case_001_compile");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn compresses_probe_case() {
        let plugin = XcodeLogPlugin::new();
        let raw = read_sample_log("xcode_log_plugin", "case_002_probe");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // case_002_probe 是 60 行 /dev/null 探针，ROI 正收益时应产出 $XC|PROBE| 标记。
        assert!(
            out.contains("$XC|PROBE|"),
            "probe 样本应产出 $XC|PROBE| 标记: {out}"
        );
    }
}
