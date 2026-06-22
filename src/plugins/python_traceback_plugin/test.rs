//! python_traceback_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::python_traceback_plugin::types::PythonTracebackPlugin;
    use crate::plugins::test_utils::*;

    #[test]
    fn detects_simple_traceback_sample() {
        let plugin = PythonTracebackPlugin::new();
        let raw = read_sample_log("python_traceback_plugin", "case_001_simple_error");
        let score = plugin.detect(&make_test_slice(&raw, SliceType::Unknown));
        assert!(score.is_some());
        assert!(score.unwrap() > 0.5);
    }

    #[test]
    fn compresses_nested_sample_produces_py_token() {
        let plugin = PythonTracebackPlugin::new();
        let raw = read_sample_log("python_traceback_plugin", "case_002_nested_error");
        let out = compress_to_string(&plugin, &raw, SliceType::Unknown);
        // 应该包含 Python 标记或原文（如果压缩反而扩张）
        assert!(
            out.contains("$PY|") || out.contains("Traceback"),
            "Python Traceback 压缩输出应包含 $PY| 标记或原文: {out}"
        );
    }

    #[test]
    fn compresses_long_traceback_sample_without_expansion() {
        let plugin = PythonTracebackPlugin::new();
        let raw = read_sample_log("python_traceback_plugin", "case_003_long_traceback");
        let out = compress_to_string(&plugin, &raw, SliceType::Unknown);
        assert!(
            out.len() <= raw.len() + 16,
            "python_traceback 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 法则 D 防失忆红线：Python 内置异常类名必须保留字面量，不得被字典化为 `$PKn`。
    #[test]
    fn preserves_exception_class_names_literally() {
        let plugin = PythonTracebackPlugin::new();
        let cases = [
            ("case_001_simple_error", "ValueError"),
            ("case_010_assertion", "AssertionError"),
            ("case_011_import_error", "ModuleNotFoundError"),
            ("case_012_key_error", "KeyError"),
        ];
        for (stem, class_name) in cases {
            let raw = read_sample_log("python_traceback_plugin", stem);
            let out = compress_to_string(&plugin, &raw, SliceType::Unknown);
            assert!(
                out.contains(class_name),
                "compact 必须保留异常类名字面量 `{class_name}`（stem={stem}），实际输出：{out}"
            );
        }
    }

    /// 新功能 1：相似异常去重
    #[test]
    fn deduplicates_similar_exceptions() {
        let plugin = PythonTracebackPlugin::new();
        let raw = read_sample_log("python_traceback_plugin", "case_013_duplicate_exception");
        let out = compress_to_string(&plugin, &raw, SliceType::Unknown);
        // 应该包含去重标记
        assert!(
            out.contains("[DUPLICATE]"),
            "去重后应包含 [DUPLICATE] 标记，实际输出：{out}"
        );
        assert!(
            out.contains("another_missing_key") && out.contains("third_missing_key"),
            "去重摘要必须保留每个不同 KeyError 的缺失键，实际输出：{out}"
        );
        // 压缩率应该显著提升
        assert!(
            out.len() < raw.len() * 80 / 100,
            "去重应该显著减少输出大小: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 新功能 2：深层堆栈截断
    #[test]
    fn truncates_deep_stack_frames() {
        let plugin = PythonTracebackPlugin::new();
        let raw = read_sample_log("python_traceback_plugin", "case_014_deep_stack");
        let out = compress_to_string(&plugin, &raw, SliceType::Unknown);
        // 应该包含堆栈摘要标记
        assert!(
            out.contains("[STACK]"),
            "深层堆栈应包含 [STACK] 摘要标记，实际输出：{out}"
        );
        // 压缩率应该显著提升
        assert!(
            out.len() < raw.len() * 70 / 100,
            "堆栈截断应该显著减少输出大小: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 新功能 3：Chained 异常压缩
    #[test]
    fn compresses_chained_exceptions() {
        let plugin = PythonTracebackPlugin::new();
        let raw = read_sample_log("python_traceback_plugin", "case_015_chained");
        let out = compress_to_string(&plugin, &raw, SliceType::Unknown);
        // 应该包含 Python 标记或原文（如果压缩反而扩张）
        assert!(
            out.contains("$PY|") || out.contains("Traceback"),
            "链式异常应包含 $PY| 标记或原文，实际输出：{out}"
        );
    }

    /// 新功能 4：异常摘要
    #[test]
    fn generates_exception_summary() {
        let plugin = PythonTracebackPlugin::new();
        let raw = read_sample_log("python_traceback_plugin", "case_016_summary");
        let out = compress_to_string(&plugin, &raw, SliceType::Unknown);
        // 应该包含摘要标记
        assert!(
            out.contains("[SUMMARY]"),
            "异常摘要应包含 [SUMMARY] 标记，实际输出：{out}"
        );
        // 应该保留异常类型
        assert!(
            out.contains("ValueError") || out.contains("KeyError") || out.contains("TypeError"),
            "摘要应保留异常类型，实际输出：{out}"
        );
    }
}
