//! rust_go_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::rust_go_plugin::RustGoPlugin;
    use crate::plugins::test_utils::*;

    #[test]
    fn detects_rust_case() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_001_rust_warning");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn compresses_go_panic_case() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_002_go_panic");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 新格式不再使用 IR 标签，直接是原始格式或路径字典化后的格式
        assert!(out.contains("goroutine") || out.contains("$P"));
    }

    /// 易误判：Python Traceback 里掺了 `error:`/`warning:` 关键字，不得被识别为 Rust。
    #[test]
    fn does_not_detect_python_traceback_that_contains_rust_keywords() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_013_looks_rust_but_python");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_none(),
            "rust_go 不应把 Python Traceback 误识别为 Rust/Go"
        );
    }

    /// 易误判：Java 堆栈与 Go 栈帧格式近似，不应命中。
    #[test]
    fn does_not_detect_java_exception_stack_as_go() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_014_looks_go_but_java");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_none(),
            "rust_go 不应把 Java 堆栈误识别为 Go"
        );
    }

    /// 功能 1: 测试 Cargo 编译输出折叠
    #[test]
    fn compresses_cargo_compiling_output() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_015_cargo_compiling");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 应该包含折叠标记
        assert!(
            out.contains("[CARGO] Compiling"),
            "应该包含 Cargo 编译折叠标记"
        );
        assert!(
            out.contains("crates (details suppressed)"),
            "应该包含折叠说明"
        );

        // 不应该包含所有单独的 Compiling 行
        let compiling_count = out.matches("Compiling libc").count();
        assert_eq!(compiling_count, 0, "不应该包含单独的 Compiling 行");
    }

    /// 功能 2: 测试错误码统计
    #[test]
    fn extracts_error_code_stats() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_016_error_code_stats");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 应该包含错误码统计
        assert!(out.contains("[ERROR_STATS]"), "应该包含错误码统计标记");
        assert!(out.contains("E0425"), "应该包含 E0425 错误码");
        assert!(out.contains("E0308"), "应该包含 E0308 错误码");
        assert!(out.contains("occurred"), "应该包含出现次数");
        assert!(out.contains("error"), "压缩后必须保留 error 信号");
        assert!(out.len() <= raw.len(), "错误码统计不得绕过 ROI 门控");
    }

    /// 功能 3: 测试 Cargo test 输出压缩
    #[test]
    fn compresses_cargo_test_output() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_017_cargo_test");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 应该包含测试摘要
        assert!(out.contains("[TEST]"), "应该包含测试标记");
        assert!(out.contains("passed"), "应该包含通过数量");
        assert!(out.contains("failed"), "应该包含失败数量");

        // 应该保留失败的测试名称
        assert!(
            out.contains("test_fail_divide_by_zero") || out.contains("FAILED"),
            "应该保留失败的测试"
        );
        let lower = out.to_ascii_lowercase();
        assert!(
            lower.contains("error") || lower.contains("panic"),
            "Cargo test 压缩后必须保留 error/panic 信号"
        );
        assert!(out.len() <= raw.len(), "Cargo test 压缩不得扩张");

        // 不应该包含所有通过的测试详情
        let test_ok_count = out.matches("test tests::test_add ... ok").count();
        assert_eq!(test_ok_count, 0, "不应该包含所有通过的测试详情");
    }

    /// 功能 4: 测试 Go test 输出压缩
    #[test]
    fn compresses_go_test_output() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_018_go_test");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 应该包含 Go 测试摘要
        assert!(out.contains("[GO TEST]"), "应该包含 Go 测试标记");
        assert!(out.contains("passed"), "应该包含通过数量");
        assert!(out.contains("failed"), "应该包含失败数量");

        // 应该保留失败的测试
        assert!(
            out.contains("TestDivideByZero") || out.contains("FAIL"),
            "应该保留失败的测试"
        );
        assert!(out.contains("error"), "Go test 压缩后必须保留 error 信号");
        assert!(out.len() <= raw.len(), "Go test 压缩不得扩张");

        // 不应该包含所有通过的测试详情
        let test_pass_count = out.matches("=== RUN   TestAdd").count();
        assert_eq!(test_pass_count, 0, "不应该包含所有通过的测试详情");
    }
}
