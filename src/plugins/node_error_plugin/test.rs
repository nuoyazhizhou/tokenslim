//! node_error_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::node_error_plugin::types::NodeErrorPlugin;
    use crate::plugins::test_utils::*;

    #[test]
    fn detects_type_error_sample() {
        let plugin = NodeErrorPlugin::new();
        let raw = read_sample_log("node_error_plugin", "case_003_type_error");
        let score = plugin.detect(&make_test_slice(&raw, SliceType::Unknown));
        assert!(score.is_some(), "Node TypeError 样本应命中 detect");
        assert!(score.unwrap() > 0.5);
    }

    #[test]
    fn compresses_long_stack_sample_produces_nd_token() {
        let plugin = NodeErrorPlugin::new();
        let raw = read_sample_log("node_error_plugin", "case_012_long_stack");
        let out = compress_to_string(&plugin, &raw, SliceType::Unknown);
        assert!(
            out.contains("$ND|"),
            "Node 长堆栈压缩输出应包含 $ND| 标记: {out}"
        );
        assert!(
            out.len() <= raw.len() + 16,
            "node_error 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 法则 D 防失忆红线：异常类名必须保留字面量，不得被字典化为 `$PKn`。
    #[test]
    fn preserves_exception_class_names_literally() {
        let plugin = NodeErrorPlugin::new();
        let cases = [
            ("case_001_syntax_error", "SyntaxError"),
            ("case_003_type_error", "TypeError"),
            ("case_006_noise", "ReferenceError"),
            ("case_007_single_line", "ReferenceError"),
            ("case_009_special_chars", "SyntaxError"),
        ];
        for (stem, class_name) in cases {
            let raw = read_sample_log("node_error_plugin", stem);
            let out = compress_to_string(&plugin, &raw, SliceType::Unknown);
            assert!(
                out.contains(class_name),
                "compact 必须保留异常类名字面量 `{class_name}`（stem={stem}），实际输出：{out}"
            );
        }
    }
}
