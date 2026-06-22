//! java_stack_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::java_stack_plugin::types::JavaStackPlugin;
    use crate::plugins::test_utils::*;

    #[test]
    fn new_has_expected_name_and_priority() {
        let plugin = JavaStackPlugin::new();
        assert_eq!(plugin.name(), "java_stack");
        assert_eq!(plugin.priority(), 200);
    }

    #[test]
    fn detects_simple_exception_sample() {
        let plugin = JavaStackPlugin::new();
        let raw = read_sample_log("java_stack_plugin", "case_001_simple_exception");
        let score = plugin.detect(&make_log_slice(&raw));
        assert!(score.is_some());
        assert!(score.unwrap() > 0.5);
    }

    #[test]
    fn detects_chained_exceptions_sample() {
        let plugin = JavaStackPlugin::new();
        let raw = read_sample_log("java_stack_plugin", "case_005_chained_exceptions");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn compresses_long_stack_sample_produces_jst_or_jex_tokens() {
        let plugin = JavaStackPlugin::new();
        let raw = read_sample_log("java_stack_plugin", "case_003_long_stack");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.contains("$JST|") || out.contains("$JEX|") || out.contains("$JCB|"),
            "长堆栈压缩输出应包含 JST/JEX/JCB 标记: {out}"
        );
        assert!(
            out.len() <= raw.len() + 16,
            "java_stack 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 法则 D 防失忆红线：Java 异常简单类名必须以字面量保留，不得被整体字典化为 `$PKn`。
    #[test]
    fn preserves_exception_class_names_literally() {
        let plugin = JavaStackPlugin::new();
        let cases: &[(&str, &[&str])] = &[
            ("case_001_simple_exception", &["NullPointerException"]),
            ("case_003_long_stack", &["StackOverflowError"]),
            (
                "case_005_chained_exceptions",
                &["IOException", "FileNotFoundException"],
            ),
        ];
        for (stem, names) in cases {
            let raw = read_sample_log("java_stack_plugin", stem);
            let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
            for class_name in *names {
                assert!(
                    out.contains(class_name),
                    "compact 必须保留异常类名字面量 `{class_name}`（stem={stem}），实际输出：{out}"
                );
            }
        }
    }

    /// 新功能 1：相同堆栈去重
    #[test]
    fn deduplicates_same_stack_frames() {
        let plugin = JavaStackPlugin::new();
        let raw = read_sample_log("java_stack_plugin", "case_013_duplicate_stack");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 应该包含去重标记
        assert!(
            out.contains("[DUPLICATE]"),
            "去重后应包含 [DUPLICATE] 标记，实际输出：{out}"
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
        let plugin = JavaStackPlugin::new();
        let raw = read_sample_log("java_stack_plugin", "case_014_deep_stack");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
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

    /// 新功能 3：异常摘要
    #[test]
    fn generates_exception_summary() {
        let plugin = JavaStackPlugin::new();
        let raw = read_sample_log("java_stack_plugin", "case_015_exception_summary");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 应该包含摘要标记
        assert!(
            out.contains("[SUMMARY]"),
            "异常摘要应包含 [SUMMARY] 标记，实际输出：{out}"
        );
        // 应该保留异常类型
        assert!(
            out.contains("NullPointerException") || out.contains("IllegalArgumentException"),
            "摘要应保留异常类型，实际输出：{out}"
        );
    }

    /// 新功能 4：Suppressed 异常压缩
    #[test]
    fn compresses_suppressed_exceptions() {
        let plugin = JavaStackPlugin::new();
        let raw = read_sample_log("java_stack_plugin", "case_016_suppressed");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 应该包含 Suppressed 摘要标记
        assert!(
            out.contains("[SUPPRESSED]"),
            "Suppressed 异常应包含 [SUPPRESSED] 标记，实际输出：{out}"
        );
        // 压缩率应该显著提升
        assert!(
            out.len() < raw.len() * 80 / 100,
            "Suppressed 压缩应该显著减少输出大小: raw={} out={}",
            raw.len(),
            out.len()
        );
    }
}
