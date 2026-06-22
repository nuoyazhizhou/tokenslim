//! nodejs_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::nodejs_plugin::NodeJsPlugin;
    use crate::plugins::test_utils::*;
    #[test]
    fn detects_node_modules_sample() {
        let plugin = NodeJsPlugin::new();
        let raw = read_sample_log("nodejs_plugin", "case_002_node_modules");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn detects_error_sample() {
        let plugin = NodeJsPlugin::new();
        let raw = read_sample_log("nodejs_plugin", "case_001_simple_error");
        // case_001 里出现 "Error:" 关键字会被 detect 命中
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn compresses_node_modules_without_expansion() {
        let plugin = NodeJsPlugin::new();
        let raw = read_sample_log("nodejs_plugin", "case_002_node_modules");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len(),
            "nodejs 插件压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    // ========== v2 新增测试（5 个高级压缩功能） ==========

    #[test]
    fn test_compress_npm_install() {
        let plugin = NodeJsPlugin::new();
        let raw = read_sample_log("nodejs_plugin", "case_012_npm_install");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 验证压缩效果
        assert!(out.contains("[NPM]"), "应包含 [NPM] 标记");
        assert!(
            out.contains("deprecation warnings suppressed") || out.contains("added"),
            "应包含压缩标记或原始内容"
        );

        // ROI 门控：压缩后不得扩张
        assert!(
            out.len() <= raw.len(),
            "npm install 压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn test_compress_tsc_output() {
        let plugin = NodeJsPlugin::new();
        let raw = read_sample_log("nodejs_plugin", "case_013_tsc_compile");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 验证压缩效果
        assert!(out.contains("[TSC]"), "应包含 [TSC] 标记");
        assert!(
            out.contains("errors") || out.contains("warnings"),
            "应包含错误/警告统计"
        );

        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "TypeScript 压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn test_compress_eslint_output() {
        let plugin = NodeJsPlugin::new();
        let raw = read_sample_log("nodejs_plugin", "case_014_eslint");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 验证压缩效果
        assert!(out.contains("[ESLINT]"), "应包含 [ESLINT] 标记");
        assert!(out.contains("problems"), "应包含 problems 统计");

        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "ESLint 压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn test_compress_webpack_output() {
        let plugin = NodeJsPlugin::new();
        let raw = read_sample_log("nodejs_plugin", "case_015_webpack");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 验证压缩效果
        assert!(out.contains("[WEBPACK]"), "应包含 [WEBPACK] 标记");

        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "Webpack 压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn test_compress_jest_output() {
        let plugin = NodeJsPlugin::new();
        let raw = read_sample_log("nodejs_plugin", "case_016_jest");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 验证压缩效果
        assert!(out.contains("[JEST]"), "应包含 [JEST] 标记");
        assert!(out.contains("Tests:"), "应包含测试统计");

        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "Jest 压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn detects_pnpm_yarn_ci_outputs() {
        let plugin = NodeJsPlugin::new();
        for case_id in [
            "case_017_pnpm_install_ci",
            "case_018_yarn_install_ci",
            "case_019_pnpm_jest_ci_failure",
            "case_020_yarn_tsc_ci_failure",
            "case_022_pnpm_eslint_ci",
        ] {
            let raw = read_sample_log("nodejs_plugin", case_id);
            assert!(plugin.detect(&make_log_slice(&raw)).is_some());
        }
    }

    #[test]
    fn compresses_pnpm_and_yarn_install_noise() {
        let plugin = NodeJsPlugin::new();
        let pnpm = read_sample_log("nodejs_plugin", "case_017_pnpm_install_ci");
        let pnpm_out = compress_to_string(&plugin, &pnpm, SliceType::LogBlock);
        assert!(pnpm_out.contains("[PNPM]"));
        assert!(pnpm_out.contains("deprecated=3"));
        assert!(pnpm_out.len() <= pnpm.len());

        let yarn = read_sample_log("nodejs_plugin", "case_018_yarn_install_ci");
        let yarn_out = compress_to_string(&plugin, &yarn, SliceType::LogBlock);
        assert!(yarn_out.contains("[YARN]"));
        assert!(yarn_out.contains("warnings=2"));
        assert!(yarn_out.len() <= yarn.len());
    }

    #[test]
    fn preserves_node_ci_failure_signals() {
        let plugin = NodeJsPlugin::new();
        let raw = read_sample_log("nodejs_plugin", "case_019_pnpm_jest_ci_failure");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("[JEST]"));
        assert!(out.contains("session.test.ts"));
        assert!(out.contains("Expected: 401"));
        assert!(out.len() <= raw.len());
    }
}
