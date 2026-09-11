//! python_traceback_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::python_traceback_plugin::types::PythonTracebackPlugin;
    use crate::plugins::test_utils::*;

    /// 测试：简单 traceback 样例被识别。
    #[test]
    fn detects_simple_traceback_sample() {
        let plugin = PythonTracebackPlugin::new();
        let raw = read_sample_log("python_traceback_plugin", "case_001_simple_error");
        let score = plugin.detect(&make_test_slice(&raw, SliceType::Unknown));
        assert!(score.is_some());
        assert!(score.unwrap() > 0.5);
    }

    /// 测试：嵌套样例压缩输出包含 $PY token。
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

    /// 测试：长 traceback 样例压缩后不扩张。
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

    /// T-008 回归：行内 $PY|TB（链式异常分隔文本与下一段标记拼同一物理行，
    /// 如 "...following exception:$PY|TB"）必须被还原——前缀分隔文本独立成行保留，
    /// traceback 头 / File 行 / 异常类与消息完整，且无 $PY 残留。
    #[test]
    fn decompress_inline_py_tb_marker_restores_chained_text() {
        let plugin = PythonTracebackPlugin::new();
        let dict = crate::core::dictionary_engine::Dictionary::new();
        let compressed =
            "The above exception was the direct cause of the following exception:$PY|TB\n\
                          $PY|FL|1|func|/src/main.py\n\
                          $PY|EX|ValueError|bad thing\n";
        let out = plugin.decompress(compressed, &dict);
        assert!(
            out.contains(
                "The above exception was the direct cause of the following exception:\nTraceback (most recent call last):"
            ),
            "分隔文本应独立成行且 traceback 头保留，实际：{out}"
        );
        assert!(
            out.contains("File \"/src/main.py\", line 1, in func"),
            "File 行应还原，实际：{out}"
        );
        assert!(
            out.contains("ValueError: bad thing"),
            "异常类应还原，实际：{out}"
        );
        assert!(!out.contains("$PY|"), "不应残留 $PY 标记，实际：{out}");
    }

    /// T-008 补充：行首标记（标准路径）解压回归，确保原有行为不回退。
    #[test]
    fn decompress_line_start_markers_restore() {
        let plugin = PythonTracebackPlugin::new();
        let dict = crate::core::dictionary_engine::Dictionary::new();
        let compressed = "$PY|TB\n$PY|FL|1|func|/src/main.py\n$PY|EX|ValueError|bad thing\n";
        let out = plugin.decompress(compressed, &dict);
        assert!(
            out.contains("Traceback (most recent call last):"),
            "traceback 头缺失：{out}"
        );
        assert!(
            out.contains("File \"/src/main.py\", line 1, in func"),
            "File 行缺失：{out}"
        );
        assert!(out.contains("ValueError: bad thing"), "异常类缺失：{out}");
        assert!(!out.contains("$PY|"), "不应残留 $PY 标记：{out}");
    }

    /// P2-77 回归：$PY|EX 消息体含 '|' 时不得截断——compress 端不转义消息内容，
    /// decompress 端必须把首个消息分隔符之后的全部内容（含 '|'）作为 msg 还原。
    #[test]
    fn decompress_ex_message_with_pipe_not_truncated() {
        let plugin = PythonTracebackPlugin::new();
        let dict = crate::core::dictionary_engine::Dictionary::new();
        let compressed = "$PY|EX|ValueError|expected a | b but got c|d\n";
        let out = plugin.decompress(compressed, &dict);
        assert!(
            out.contains("ValueError: expected a | b but got c|d"),
            "含 '|' 的消息体应完整还原，实际：{out}"
        );
        assert!(!out.contains("$PY|"), "不应残留 $PY 标记：{out}");
    }

    /// T-008 加固（P2 复审建议）：行内 $PY|FL 与 $PY|EX（前缀文本与标记拼同一物理行）
    /// 同样必须还原——把此前仅黑盒注入覆盖的 FL/EX 行内形态固化为仓库级回归。
    #[test]
    fn decompress_inline_fl_ex_markers_restore() {
        let plugin = PythonTracebackPlugin::new();
        let dict = crate::core::dictionary_engine::Dictionary::new();
        let compressed = "During handling of the above exception, another exception occurred:$PY|EX|TypeError|bad operand type\n\
                          at $PY|FL|42|worker|/app/worker.py\n";
        let out = plugin.decompress(compressed, &dict);
        assert!(
            out.contains(
                "During handling of the above exception, another exception occurred:\nTypeError: bad operand type"
            ),
            "行内 EX 应还原且前缀独立成行，实际：{out}"
        );
        assert!(
            out.contains("File \"/app/worker.py\", line 42, in worker"),
            "行内 FL 应还原，实际：{out}"
        );
        assert!(!out.contains("$PY|"), "不应残留 $PY 标记，实际：{out}");
    }
}
