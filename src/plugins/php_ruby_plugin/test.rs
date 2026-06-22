//! php_ruby_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::php_ruby_plugin::PhpRubyPlugin;
    use crate::plugins::test_utils::*;
    #[test]
    fn detects_php_error_sample() {
        let plugin = PhpRubyPlugin::new();
        let raw = read_sample_log("php_ruby_plugin", "case_001_php_error");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "PHP 错误样本应命中 detect"
        );
    }

    #[test]
    fn detects_ruby_error_sample() {
        let plugin = PhpRubyPlugin::new();
        let raw = read_sample_log("php_ruby_plugin", "case_002_ruby_error");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "Ruby 错误样本应命中 detect"
        );
    }

    #[test]
    fn compresses_long_stack_sample_without_expansion() {
        let plugin = PhpRubyPlugin::new();
        let raw = read_sample_log("php_ruby_plugin", "case_004_long_stack");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len(),
            "php_ruby 插件压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }
}
