//! maven_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::maven_plugin::MavenPlugin;
    use crate::plugins::test_utils::*;

    /// 测试：Maven 构建成功样例被识别。
    #[test]
    fn detects_maven_build_success_sample() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_001_build_success");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "Maven build 成功样本应命中 detect"
        );
    }

    /// 测试：Maven 构建错误样例被识别。
    #[test]
    fn detects_maven_build_error_sample() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_002_build_error");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    /// 测试：长样本压缩后无明显扩张。
    #[test]
    fn compresses_long_sample_without_significant_expansion() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_015_dependencies");
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

    /// P2-75 负路径回归：非 javac 格式的 [ERROR] 行（`Failed to execute goal ...` 根因行）
    /// 必须透传保留，不得被 javac 错误块压缩静默丢弃——错误信号净丢失比体积更贵。
    #[test]
    fn preserves_failed_to_execute_goal_root_cause_line() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_002_build_error");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        assert!(
            out.contains("Failed to execute goal"),
            "非 javac 格式 [ERROR] 根因行必须透传保留（P2-75），实际输出：{out}"
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

    /// 功能 3a: 测试 JUnit 测试计数正确（覆盖而非累加）与失败详情保留
    /// surefire 在 class 级与 `Results:` 汇总级各打印一次 `Tests run`，累加会把
    /// 3 run/1 fail 误报成 6 run/2 fail。只采信汇总级计数。同时必须保留失败测试名与断言详情。
    #[test]
    fn compresses_junit_output_counts_not_doubled() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_003_test_failure");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 汇总计数应为 3 run/1 fail，不得翻倍为 6/2
        assert!(
            out.contains("Tests run: 3, Failures: 1"),
            "应保留准确汇总计数（3 run/1 fail），实际输出: {:?}",
            out
        );
        assert!(
            !out.contains("6 tests") && !out.contains("Tests run: 6"),
            "不得把 class 级与汇总级计数累加翻倍，实际输出: {:?}",
            out
        );
        // 失败测试名与断言详情必须保留
        assert!(
            out.contains("testSomething") && out.contains("expected:<5> but was:<3>"),
            "应保留失败测试名与断言详情，实际输出: {:?}",
            out
        );
    }

    /// 功能 2a: 测试 Javac 警告与错误的边界归属
    /// 警告块结束后的 `[ERROR] Main.java` 头行连同其 `symbol:`/`location:` 续行
    /// 必须归属错误块，不得误吞到警告汇总下方。
    #[test]
    fn compresses_javac_error_context_not_leaked_to_warning() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_013_javac_warnings");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 错误续行上下文（symbol/location）必须出现在错误标记之后，而非警告汇总之后
        let warn_idx = out.find("[JAVAC] Warning:").unwrap_or(usize::MAX);
        let err_idx = out.find("[JAVAC] Error:").unwrap_or(usize::MAX);
        let sym_idx = out.find("symbol:").unwrap_or(usize::MAX);
        assert!(
            warn_idx < err_idx && err_idx < sym_idx,
            "symbol 续行应归属错误块而非警告块，实际输出: {:?}",
            out
        );
        // 错误本体与上下文完整保留
        assert!(
            out.contains("cannot find symbol"),
            "应保留 cannot find symbol 错误，实际输出: {:?}",
            out
        );
        assert!(
            out.contains("variable foo") && out.contains("class com.example.Main"),
            "应保留 symbol:/location: 错误上下文，实际输出: {:?}",
            out
        );
    }

    /// 功能 6: 测试 dependency tree 压缩
    /// 折叠 `:jar:` type 样板、保留树形结构、精准压缩不得扩张。
    #[test]
    fn compresses_dependency_tree() {
        let plugin = MavenPlugin::new();
        let raw = read_sample_log("maven_plugin", "case_005_dependency_tree");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 保留所有坐标与层级信号：group、artifact、version、scope、树形前缀
        assert!(
            out.contains("junit-jupiter-api:5.9.1:test"),
            "应保留 artifact:version:scope，实际输出: {:?}",
            out
        );
        assert!(
            out.contains("jackson-databind:2.14.2:compile"),
            "应保留 root 子节点坐标，实际输出: {:?}",
            out
        );
        assert!(
            out.contains("+-") && out.contains("\\-"),
            "应保留树形层级前缀",
        );

        // 树形层级前缀缺失时（如 root 行 `com.example:myproject:jar:1.0.0`）不命中折叠，保留原样。
        // 但带树形前缀的子节点坐标中的 `:jar:` type 样板应被折叠。
        assert!(
            !out.contains("junit-jupiter-api:jar:5.9.1"),
            "子节点 :jar: type 样板应被折叠，实际输出: {:?}",
            out
        );
        assert!(
            !out.contains("jackson-databind:jar:2.14.2"),
            "子节点 :jar: type 样板应被折叠，实际输出: {:?}",
            out
        );

        // 不得扩张
        assert!(
            out.len() <= raw.len(),
            "raw={} out={}",
            raw.len(),
            out.len()
        );
    }
}
