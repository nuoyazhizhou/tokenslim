//! sql_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::sql_plugin::types::SqlPlugin;
    use crate::plugins::test_utils::*;

    #[test]
    fn detects_select_sample() {
        let plugin = SqlPlugin::new();
        let raw = read_sample_log("sql_plugin", "case_001_select");
        let score = plugin.detect(&make_test_slice(&raw, SliceType::Line));
        assert!(score.is_some());
        assert!(score.unwrap() > 0.5);
    }

    #[test]
    fn detects_transaction_sample() {
        let plugin = SqlPlugin::new();
        let raw = read_sample_log("sql_plugin", "case_011_transaction");
        assert!(plugin
            .detect(&make_test_slice(&raw, SliceType::Line))
            .is_some());
    }

    #[test]
    fn compresses_complex_sample_without_expansion() {
        let plugin = SqlPlugin::new();
        let raw = read_sample_log("sql_plugin", "case_003_complex");
        let out = compress_to_string(&plugin, &raw, SliceType::Line);
        assert!(
            out.len() <= raw.len() + 16,
            "sql 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn compresses_complex_join_sample_without_expansion() {
        let plugin = SqlPlugin::new();
        let raw = read_sample_log("sql_plugin", "case_012_complex_join");
        let out = compress_to_string(&plugin, &raw, SliceType::Line);
        assert!(
            out.len() <= raw.len() + 16,
            "sql 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }
}
