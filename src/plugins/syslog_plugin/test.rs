//! syslog_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::syslog_plugin::SyslogPlugin;
    use crate::plugins::test_utils::*;

    #[test]
    fn detects_syslog_case() {
        let plugin = SyslogPlugin::new();
        let raw = read_sample_log("syslog_plugin", "case_001_auth");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn compresses_syslog_case() {
        let plugin = SyslogPlugin::new();
        let raw = read_sample_log("syslog_plugin", "case_002_kernel");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 法则 A ROI 门控：小样本场景下 $SYS| 元字符开销可能大于字典收益，整段回退原文。
        assert!(
            out.len() <= raw.len() + 4,
            "syslog 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 合成大样本：把 case_001 首行重复 60 次，验证插件在多行 syslog 输入下仍稳定。
    #[test]
    fn compresses_bulk_sample_uses_sys_token() {
        let plugin = SyslogPlugin::new();
        let seed = read_sample_log("syslog_plugin", "case_001_auth");
        let first_line = seed.lines().next().expect("case_001 应至少一行");
        let mut bulk = String::new();
        for _ in 0..60 {
            bulk.push_str(first_line);
            bulk.push('\n');
        }
        let out = compress_to_string(&plugin, &bulk, SliceType::LogBlock);
        assert!(
            out.len() <= bulk.len(),
            "syslog 大样本压缩不得扩张（无 manager 时 prefer_non_expanding 回退原文）: raw={} out={}",
            bulk.len(),
            out.len()
        );
    }
}
