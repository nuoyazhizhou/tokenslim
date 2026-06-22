//! maven_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::maven_plugin::MavenPlugin;
    use crate::plugins::test_utils::*;

    #[test]
    fn detects_maven_build_success_sample() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_001_build_success");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "Maven build 成功样本应命中 detect"
        );
    }

    #[test]
    fn detects_maven_build_error_sample() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_002_build_error");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn compresses_long_sample_without_significant_expansion() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_012_long");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 允许压缩流程补一个尾换行等非实质性字节波动，不得显著扩张
        assert!(
            out.len() <= raw.len() + 4,
            "Maven 插件压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 功能 1: 测试 Javac 警告压缩
    #[test]
    fn compresses_javac_warnings() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_013_javac_warnings");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 应该包含警告折叠标记
        assert!(out.contains("[JAVAC] Warning:"), "应该包含 Javac 警告标记");
        assert!(
            out.contains("similar warnings suppressed"),
            "应该包含警告折叠说明"
        );

        // 不应该包含所有单独的警告行
        let warning_count = out.matches("[WARNING]").count();
        assert!(
            warning_count < 10,
            "警告应该被折叠，不应该有 10 个 [WARNING] 标记"
        );
    }

    /// 功能 2: 测试 Javac 错误压缩
    #[test]
    fn compresses_javac_errors() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_013_javac_warnings");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 应该包含错误标记
        assert!(
            out.contains("[JAVAC] Error:") || out.contains("cannot find symbol"),
            "应该包含 Javac 错误标记"
        );
    }

    /// 功能 3: 测试 JUnit 测试输出压缩
    #[test]
    fn compresses_junit_output() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_014_junit_tests");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 应该包含测试摘要
        assert!(out.contains("[JUNIT]"), "应该包含 JUnit 测试标记");
        assert!(out.contains("Tests run:"), "应该包含测试运行摘要");
        assert!(out.contains("Failures:"), "应该包含失败数量");

        // 应该保留失败的测试
        assert!(
            out.contains("Failed tests:") || out.contains("BarTest"),
            "应该保留失败的测试类名"
        );
    }

    /// 功能 4: 测试依赖下载折叠
    #[test]
    fn folds_dependency_downloads() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_015_dependencies");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 应该包含依赖解析摘要
        assert!(
            out.contains("[MAVEN] Resolving") || out.contains("dependencies"),
            "应该包含依赖解析摘要"
        );

        // 不应该包含所有单独的下载行
        let download_count = out.matches("Downloading from central:").count();
        assert!(
            download_count < 10,
            "下载行应该被折叠，不应该有 10 个下载行"
        );
    }

    /// 功能 5: 测试构建摘要提取
    #[test]
    fn extracts_build_summary() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_016_build_summary");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 应该包含构建摘要
        assert!(out.contains("[MAVEN] BUILD SUCCESS"), "应该包含构建摘要");
        assert!(
            out.contains("classes compiled") || out.contains("warnings") || out.contains("tests"),
            "应该包含编译/警告/测试统计"
        );
        let lower = out.to_ascii_lowercase();
        assert!(
            lower.contains("error") || lower.contains("fatal") || lower.contains("panic"),
            "构建摘要必须保留 error/fatal/panic 信号"
        );
        assert!(out.len() <= raw.len(), "Maven 构建摘要不得扩张");
    }
}
