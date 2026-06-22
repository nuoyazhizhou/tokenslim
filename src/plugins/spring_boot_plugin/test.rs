//! spring_boot_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::spring_boot_plugin::types::SpringBootPlugin;
    use crate::plugins::test_utils::*;
    #[test]
    fn detects_real_multi_line_info_log_sample() {
        let plugin = SpringBootPlugin::new();
        let raw = read_sample_log("spring_boot_plugin", "case_003_info_log");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "多行 Spring Boot INFO 日志应命中 detect"
        );
    }

    #[test]
    fn detects_complex_multi_line_sample() {
        let plugin = SpringBootPlugin::new();
        let raw = read_sample_log("spring_boot_plugin", "case_012_complex");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "多行复杂 Spring Boot 日志应命中 detect"
        );
    }

    #[test]
    fn compresses_stacktrace_sample_without_expansion() {
        let plugin = SpringBootPlugin::new();
        let raw = read_sample_log("spring_boot_plugin", "case_002_stacktrace");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len() + 16,
            "spring_boot 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn compresses_info_log_sample_without_expansion() {
        let plugin = SpringBootPlugin::new();
        let raw = read_sample_log("spring_boot_plugin", "case_003_info_log");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len() + 16,
            "spring_boot 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 单行 Spring Boot 日志，能被 SPRING_LIFECYCLE_RE 匹配。
    /// 历史上由于 `$` 不跨 `\n` 需要 `trim_end()`，建议 2 的 `(?m)` 修复让 `$` 按行匹配，
    /// 现在可以直接喂原样本（含尾 `\n`）。
    #[test]
    fn detects_real_spring_single_line_sample() {
        let plugin = SpringBootPlugin::new();
        let raw = read_sample_log("spring_boot_plugin", "case_013_real_spring_single_line");
        let score = plugin.detect(&make_log_slice(&raw));
        assert!(
            score.is_some(),
            "单行 Spring Boot 日志应命中 detect，当前 score={:?}",
            score
        );
    }

    /// Maven 下载日志触发 `Downloaded from` / `Downloading from` 分支。
    #[test]
    fn detects_maven_download_sample() {
        let plugin = SpringBootPlugin::new();
        let raw = read_sample_log("spring_boot_plugin", "case_014_maven_download");
        let score = plugin.detect(&make_log_slice(&raw));
        assert!(score.is_some(), "Maven 下载日志应命中 detect");
        assert!(score.unwrap() >= 0.5);
    }

    /// Log4j JSON 格式日志：每行是合法 JSON（含 `"logger"`/`"level"` 等 Spring 项目常见字段），
    /// 但既不含 `Downloaded from` / `Spring Boot` 等 Spring 触发词，也不匹配 `timestamp INFO ... --- [thread] logger : msg`
    /// 的 SPRING_LIFECYCLE_RE 结构，应 detect=None。
    #[test]
    fn does_not_detect_log4j_json_as_spring_boot() {
        let plugin = SpringBootPlugin::new();
        let raw = read_sample_log("spring_boot_plugin", "case_015_looks_spring_but_log4j_json");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_none(),
            "spring_boot 不应把 Log4j JSON 日志误识别为 Spring Boot"
        );
    }
}
