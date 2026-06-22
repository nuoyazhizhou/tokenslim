//! smart_code_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::smart_code_plugin::types::SmartCodePlugin;
    use crate::plugins::test_utils::*;

    #[test]
    fn detects_simple_code_sample() {
        let plugin = SmartCodePlugin::new();
        let raw = read_sample_log("smart_code_plugin", "case_001_simple_code");
        assert!(plugin
            .detect(&make_test_slice(&raw, SliceType::Unknown))
            .is_some());
    }

    #[test]
    fn detects_code_block_sample() {
        let plugin = SmartCodePlugin::new();
        let raw = read_sample_log("smart_code_plugin", "case_008_code_block");
        assert!(plugin
            .detect(&make_test_slice(&raw, SliceType::Unknown))
            .is_some());
    }

    #[test]
    fn compresses_long_code_sample_without_expansion() {
        let plugin = SmartCodePlugin::new();
        let raw = read_sample_log("smart_code_plugin", "case_009_long_code");
        let out = compress_to_string(&plugin, &raw, SliceType::Unknown);
        assert!(
            out.len() <= raw.len() + 16,
            "smart_code 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 法则 D 防失忆红线：即便 smart_code 是通用源码压缩，也必须保留异常类字面量。
    #[test]
    fn preserves_exception_class_names_literally() {
        let plugin = SmartCodePlugin::new();
        let cases = [
            ("case_002_code_error", "SyntaxError"),
            ("case_011_stack_trace", "ZeroDivisionError"),
        ];
        for (stem, class_name) in cases {
            let raw = read_sample_log("smart_code_plugin", stem);
            let out = compress_to_string(&plugin, &raw, SliceType::Unknown);
            assert!(
                out.contains(class_name),
                "compact 必须保留异常类名字面量 `{class_name}`（stem={stem}），实际输出：{out}"
            );
        }
    }
}
