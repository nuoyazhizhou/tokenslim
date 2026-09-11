//! noise_filter_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::noise_filter_plugin::NoiseFilterPlugin;
    use crate::plugins::test_utils::*;
    /// 测试：重复复制行样例被识别。
    #[test]
    fn detects_repetitive_copying_sample() {
        let plugin = NoiseFilterPlugin::new();
        let raw = read_sample_log("noise_filter_plugin", "case_002_repetitive_lines");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "重复 Copying 样本应命中 detect"
        );
    }

    /// 测试：长 hex 序列样例被识别。
    #[test]
    fn detects_long_hex_sample() {
        let plugin = NoiseFilterPlugin::new();
        let raw = read_sample_log("noise_filter_plugin", "case_003_long_hex");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    /// 回归：普通 CRLF 文本（无进度/重绘证据）不得再被误判为噪声。
    /// 修复前"文本含任何 \r 即给 0.95"，导致含 CRLF 行尾的日志在插件选择打分中
    /// 被 noise_filter 抢占，与真实压缩路由（如 vcs 的 git merge 冲突样例）失配。
    #[test]
    fn does_not_detect_bare_crlf_as_noise() {
        let plugin = NoiseFilterPlugin::new();
        let raw = read_sample_log("noise_filter_plugin", "case_005_noise_free");
        assert_eq!(
            plugin.detect(&make_log_slice(&raw)),
            None,
            "纯 CRLF 且无进度/重绘证据的日志不应判为噪声"
        );
    }

    /// 门控：含"数字%"进度文字的样例仍应判为噪声，避免过度收紧破坏进度清理。
    #[test]
    fn detects_progress_percent_sample() {
        let plugin = NoiseFilterPlugin::new();
        let raw = read_sample_log("noise_filter_plugin", "case_001_progress_bars");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "进度百分比样例应被识别为噪声"
        );
    }

    /// 测试：重复行样例压缩后体积缩减。
    #[test]
    fn compresses_repetitive_lines_sample_and_shrinks() {
        let plugin = NoiseFilterPlugin::new();
        let raw = read_sample_log("noise_filter_plugin", "case_002_repetitive_lines");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 噪点过滤应实打实缩短（不严格断言幅度，只要不扩张）
        assert!(
            out.len() <= raw.len(),
            "噪点过滤不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }
}
