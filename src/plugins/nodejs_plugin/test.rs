//! nodejs_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::nodejs_plugin::NodeJsPlugin;
    use crate::plugins::test_utils::*;
    /// 规则：node 错误栈样式例（异常头 + ≥2 个 ` at .../x.js:line:col` 栈帧）应交由
    /// node_error 做栈级去重/精简，nodejs 不得抢占（返回 None 让位）。
    /// 否则 nodejs 0.85 > node_error 0.8，会抢走错误栈、使 node_error 形同虚设。
    #[test]
    fn defers_error_stack_to_node_error_via_none() {
        let plugin = NodeJsPlugin::new();
        for stem in [
            "case_001_simple_error",
            "case_002_node_modules",
            "case_012_long_stack",
        ] {
            let raw = read_sample_log("nodejs_plugin", stem);
            assert!(
                plugin.detect(&make_log_slice(&raw)).is_none(),
                "错误栈样例 {stem}.log 应由 node_error 承接，nodejs detect 应返回 None"
            );
        }
    }

    /// 测试：含 npm 安装进度（无错误栈、无 docker/k8s 信号）的样例被 nodejs 识别。
    #[test]
    fn detects_npm_install_sample() {
        let plugin = NodeJsPlugin::new();
        // cmp 压缩仍走高级压缩，detect 以纯文本判定；npm 关键字应命中。
        let raw = read_sample_log("nodejs_plugin", "case_003_npm_install");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "npm install 样例应被 nodejs detect 命中"
        );
    }

    /// 测试：node_modules 样例压缩后不扩张。
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

    // ========== P2-74 ROI 收口回归 ==========

    /// 测试（P2-74）：单行 `PASS` 的 jest 极小样本——jest 压缩器会把它重写成
    /// 零统计的 `[JEST]` 摘要头（P3-135 最重实例，24B→46B 扩张）。插件级
    /// `prefer_non_expanding` 出口收口必须回退原文整段透传，禁止扩张。
    #[test]
    fn p2_74_tiny_jest_sample_never_expands() {
        let plugin = NodeJsPlugin::new();
        let raw = "jest --config jest.config.js\nPASS src/App.test.tsx\n";
        assert!(
            plugin.detect(&make_log_slice(raw)).is_some(),
            "jest 关键字样本应被 nodejs detect 命中"
        );
        let out = compress_to_string(&plugin, raw, SliceType::LogBlock);
        assert_eq!(
            out, raw,
            "极小样本经 ROI 收口应回退原文整段透传，不得重写为 [JEST] 摘要"
        );
    }

    // ========== v2 新增测试（5 个高级压缩功能） ==========

    /// 测试：npm install 输出被压缩为 added 摘要。
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

    /// 测试：TypeScript 编译输出被压缩为 [TSC] 摘要。
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

    /// 测试：ESLint 输出被压缩为 [ESLINT] 摘要。
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

    /// 测试：Webpack 输出被压缩为 [WEBPACK] 摘要。
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

    /// 测试：Jest 输出被压缩为 [JEST] 摘要。
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

    /// 测试：pnpm/yarn CI 输出被识别。
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

    /// 测试：pnpm 与 yarn install 噪声被折叠压缩。
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

    /// 测试：Node CI 失败信号在压缩后被保留。
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

    /// 测试：vitest 全通过输出被压缩为 [VITEST] 摘要，✓ 通过行折叠。
    #[test]
    fn test_compress_vitest_pass() {
        let plugin = NodeJsPlugin::new();
        let raw = read_sample_log("nodejs_plugin", "case_023_vitest_pass");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "vitest 输出应被 detect 命中"
        );

        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 权威统计保留
        assert!(out.contains("[VITEST]"), "应包含 [VITEST] 标记: {out}");
        assert!(out.contains("35 passed"), "应保留通过计数: {out}");
        // 底部汇总行 `✓ 13 passed` 不触发第二次压缩，全文件只应有一个摘要
        assert_eq!(
            out.matches("[VITEST]").count(),
            1,
            "vitest 全通过只应输出一个 [VITEST] 摘要: {out}"
        );
        // ✓ 通过行折叠
        assert!(!out.contains("useDebounce"), "✓ 通过行应被折叠: {out}");
        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "vitest 通过压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 测试：vitest 部分失败输出保留失败测试名与详情，✓ 通过行折叠。
    #[test]
    fn test_compress_vitest_fail() {
        let plugin = NodeJsPlugin::new();
        let raw = read_sample_log("nodejs_plugin", "case_024_vitest_fail");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_some(),
            "vitest 输出应被 detect 命中"
        );

        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 失败计数与失败测试名保留
        assert!(out.contains("[VITEST]"), "应包含 [VITEST] 标记: {out}");
        assert!(out.contains("3 failed"), "应保留失败计数: {out}");
        assert!(out.contains("formats negative"), "应保留失败测试名: {out}");
        // 失败详情保留（含超时信号）
        assert!(out.contains("Test timed out"), "应保留超时失败详情: {out}");
        assert!(
            out.contains("Expected: \"$-100.00\""),
            "应保留断言失败期望值: {out}"
        );
        // ✓ 通过行折叠
        assert!(!out.contains("useDebounce"), "✓ 通过行应被折叠: {out}");
        // 失败尾行不得落入 jest 分支（防回归）
        assert!(
            !out.contains("[JEST]"),
            "vitest 输出不得被误判为 jest: {out}"
        );
        // ROI 门控
        assert!(
            out.len() <= raw.len(),
            "vitest 失败压缩不得扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }
}
