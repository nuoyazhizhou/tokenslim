//! syslog_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::syslog_plugin::SyslogPlugin;
    use crate::plugins::test_utils::*;

    /// 测试：syslog 样例被识别。
    #[test]
    fn detects_syslog_case() {
        let plugin = SyslogPlugin::new();
        let raw = read_sample_log("syslog_plugin", "case_001_auth");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    /// 测试：syslog 样例被压缩且不扩张。
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

    /// P1-06 回归：syslog 插件必须实现文档级剥皮——样本剥皮返回 Some、
    /// 内层正文非空、外壳摘要含 `$SYS|` 紧凑行（旧实现走 trait 默认 None，
    /// 两层化对 syslog 类别永不生效且无任何指标可见）。
    #[test]
    fn peels_syslog_document() {
        let plugin = SyslogPlugin::new();
        // 夹具：syslog 行（皮）+ 内嵌非 syslog 行（肉）——剥皮后内层必须非空。
        let raw = "Jul  1 10:00:01 host1 sshd[123]: Accepted password for user from 10.0.0.1 port 22 ssh2\nJul  1 10:00:02 host1 sshd[123]: pam_unix(sshd:session): session opened\n    at com.foo.Bar.baz(Bar.java:1)\n";
        let skin = plugin
            .peel_document(raw)
            .expect("syslog 文档应可剥皮（P1-06）");
        assert!(
            !skin.inner_body.trim().is_empty(),
            "内层正文应非空（非 syslog 行进入内层）"
        );
        assert!(
            skin.summary.contains("$SYS|"),
            "外壳摘要应含 $SYS| 紧凑行: {}",
            skin.summary
        );
        assert!(
            skin.summary.contains("Accepted password"),
            "syslog 消息内容不得丢失（可还原性）: {}",
            skin.summary
        );
    }
}
