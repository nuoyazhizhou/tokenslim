//! gcc_log_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::gcc_log_plugin::GccLogPlugin;
    use crate::plugins::test_utils::*;

    #[test]
    fn detects_gcc_compile() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_001_compile_success");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn detects_gcc_error() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_002_compile_error");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn compresses_without_expansion() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_001_compile_success");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len(),
            "gcc_log 插件压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn test_case_013_repeated_warnings() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_013_repeated_warnings");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 验证警告折叠
        assert!(out.contains("[WARNING]"), "应包含 [WARNING] 标记");
        assert!(
            out.contains("repeated") || out.contains("suppressed"),
            "应包含折叠标记"
        );

        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "警告折叠不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn test_case_014_build_summary() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_014_build_summary");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 验证构建摘要
        assert!(out.contains("[SUMMARY]"), "应包含 [SUMMARY] 标记");
        assert!(
            out.contains("errors") || out.contains("warnings"),
            "应包含错误/警告统计"
        );

        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "构建摘要不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn test_case_015_linker_output() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_015_linker_output");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 验证链接器压缩
        assert!(out.contains("$LD"), "应包含 $LD 标记");
        assert!(
            out.contains("undefined reference"),
            "应保留 undefined reference"
        );

        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "链接器压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn test_case_016_cmake_configure() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_016_cmake_configure");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CMAKE"));
        assert!(out.len() <= raw.len());
    }

    #[test]
    fn test_case_017_ninja_progress() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_017_ninja_progress");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$NINJA"));
        assert!(out.len() <= raw.len());
    }

    #[test]
    fn test_case_019_ctest_failure() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_019_ctest_failure");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CTEST"));
        assert!(out.contains("fail") || out.contains("FAILED"));
        assert!(out.len() <= raw.len());
    }

    #[test]
    fn test_case_020_cmake_configure_failure() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_020_cmake_configure_failure");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CMAKE") || out.contains("CMake Error"));
        assert!(out.to_ascii_lowercase().contains("error"));
        assert!(out.len() <= raw.len());
    }

    #[test]
    fn test_case_023_ci_cmake_ninja_ctest() {
        let plugin = GccLogPlugin::new();
        let raw = read_sample_log("gcc_log_plugin", "case_023_ci_cmake_ninja_ctest");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("$CMAKE"));
        assert!(out.contains("$NINJA"));
        assert!(out.contains("$CTEST"));
        assert!(out.len() <= raw.len());
    }
}
