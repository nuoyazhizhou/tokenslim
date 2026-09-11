//! spring_boot_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::spring_boot_plugin::types::SpringBootPlugin;
    use crate::plugins::test_utils::*;
    /// 测试：真实多行 INFO 日志样例被识别。
    #[test]
    fn detects_real_multi_line_info_log_sample() {
        let plugin = SpringBootPlugin::new();
        let raw = read_sample_log("spring_boot_plugin", "case_003_info_log");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "多行 Spring Boot INFO 日志应命中 detect"
        );
    }

    /// 测试：复杂多行样例被识别。
    #[test]
    fn detects_complex_multi_line_sample() {
        let plugin = SpringBootPlugin::new();
        let raw = read_sample_log("spring_boot_plugin", "case_012_complex");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "多行复杂 Spring Boot 日志应命中 detect"
        );
    }

    /// 测试：堆栈跟踪样例压缩后不扩张。
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

    /// 测试：INFO 日志样例压缩后不扩张。
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

    /// 回归（SAP-0594）：Maven 前缀压缩不得截断动词。
    /// 历史上 `Downloading from central: <url>` 被硬编码 `&line[..10]` 截成 `Downloadin`
    /// （丢尾字母 `g`，无法还原也无法区分 Downloading/Downloaded），且仓库名 `central` 被丢弃。
    /// 修复后应完整保留 `Downloading`/`Downloaded` 与 `central`，URL 折叠为路径 token。
    #[test]
    fn compresses_maven_download_without_truncating_verb() {
        let plugin = SpringBootPlugin::new();
        let raw = read_sample_log("spring_boot_plugin", "case_014_maven_download");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 动词完整保留：每行必须以完整 Downloading/Downloaded 开头，不得出现被截断的 "Downloadin "
        assert!(
            !out.contains("Downloadin ") && !out.contains("Downloadin\n"),
            "Maven 前缀动词不得被截断为 Downloadin: {out}"
        );

        // 仓库名 central 保留为可读前缀（4 行均含 from central）
        let central_lines = out.lines().filter(|l| l.contains("central")).count();
        assert_eq!(
            central_lines, 4,
            "压缩产物应保留 4 行 central 仓库名, got {central_lines}: {out}"
        );
        for line in out.lines() {
            assert!(
                line.contains("Downloading ") || line.contains("Downloaded "),
                "每行应保留完整 Downloading/Downloaded 动词, current: {line}"
            );
        }

        // URL 折叠后仍保留 URL 叶子（jar 文件名），同时不显著扩张
        assert!(
            out.contains("spring-core-5.3.10.jar") && out.contains("spring-boot-starter-2.7.0.jar"),
            "URL 折叠后应保留 jar 叶子文件名: {out}"
        );

        // 下载完成行的转移元数据（大小/速度）不得被路径折叠吞掉（LLM 语义门禁 G5 规则 5/7）。
        assert!(
            out.contains("1.2 MB at 3.4 MB/s") && out.contains("4.5 KB at 2.1 MB/s"),
            "下载完成行应保留大小/速度元数据(1.2 MB at 3.4 MB/s, 4.5 KB at 2.1 MB/s): {out}"
        );

        assert!(
            out.len() <= raw.len(),
            "spring_boot 压缩 Maven 下载不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
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
