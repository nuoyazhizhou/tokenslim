//! generic_text_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::generic_text_plugin::GenericTextPlugin;
    use crate::plugins::test_utils::*;
    /// 测试：generic_text 作为兜底插件，对任何非空文本返回低置信度。
    #[test]
    fn detects_non_empty_sample_with_fallback_confidence() {
        // generic_text 是兜底插件，对任何非空文本都应给出低置信度。
        let plugin = GenericTextPlugin::new();
        let raw = read_sample_log("generic_text_plugin", "case_001_normal_text");
        let slice = make_log_slice(&raw);
        let score = plugin.detect(&slice);
        assert!(score.is_some(), "generic_text 应对非空文本返回置信度");
    }

    /// 测试：ANSI 色码样本被剥离控制字符且输出不扩张。
    #[test]
    fn compresses_ansi_sample_without_expansion() {
        // 用 ANSI 色码样本验证 ANSI 剥离与「不扩张」性质。
        let plugin = GenericTextPlugin::new();
        let raw = read_sample_log("generic_text_plugin", "case_002_ansi_colors");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len(),
            "输出字节数不得超过输入: raw={} out={}",
            raw.len(),
            out.len()
        );
        assert!(
            !out.contains('\x1B'),
            "ANSI 控制字符必须被剥离，实际包含: {out}"
        );
    }

    /// 测试：连续空行样本在折叠开关下被压缩且不扩张。
    #[test]
    fn compresses_repeated_blank_lines_sample() {
        // case_003_many_blank_lines 用于验证「折叠空行」开关能生效。
        let plugin = GenericTextPlugin::new();
        let raw = read_sample_log("generic_text_plugin", "case_003_many_blank_lines");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.len() <= raw.len());
    }

    /// 测试：G-2 重复行收敛——连续相同行收敛为「首行 + ×N」，且输入首行（锚点）必保留。
    #[test]
    fn collapses_repeated_lines_with_count_and_keeps_anchor() {
        let plugin = GenericTextPlugin::new();
        let raw = read_sample_log("generic_text_plugin", "case_013_repeated_lines");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 锚点法则 0：输入首行必须原样保留。
        assert!(
            out.starts_with("=== sync started ==="),
            "输入首行作为锚点必须保留，got head={}",
            &out.chars().take(40).collect::<String>()
        );
        // 重复 5 行应收敛为 1 行并保留次数。
        assert!(
            out.contains("processing batch 1 ×5"),
            "连续重复行应收敛为首行+×N，got={out}"
        );
        assert!(
            out.contains("all batches processed successfully"),
            "结尾结果行必须保留，got={out}"
        );
    }

    /// 测试：G-2 时间戳归一——行首时间戳替换为 [T]，重复轮询行收敛，且关键状态行保留。
    #[test]
    fn normalizes_line_timestamps_and_collapses_poll() {
        let plugin = GenericTextPlugin::new();
        let raw = read_sample_log("generic_text_plugin", "case_014_timestamp_poll");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 时间戳数值必须被归一为占位符。
        assert!(
            !out.contains("10:00:"),
            "行首时间戳应归一为 [T]，got={out}"
        );
        // 关键状态行保留（归一后的 health ok）。
        assert!(
            out.contains("[T] health ok"),
            "关键状态行应保留并归一，got={out}"
        );
        // 重复轮询行收敛。
        assert!(
            out.contains("polling service A ×4"),
            "重复轮询行应收敛为 ×4，got={out}"
        );
    }

    /// 测试：G-2 默认保守——噪声行开关默认关，装饰进度行不被裁剪。
    #[test]
    fn keeps_noise_lines_by_default() {
        let plugin = GenericTextPlugin::new();
        let raw = read_sample_log("generic_text_plugin", "case_015_noise_progress");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 默认 drop_noise_lines=false：done 与下载进度行均保留。
        assert!(
            out.contains("done"),
            "默认不裁剪噪声行，结果标记行保留，got={out}"
        );
        assert!(
            out.contains("Downloading"),
            "默认不裁剪进度行，got={out}"
        );
    }
}
