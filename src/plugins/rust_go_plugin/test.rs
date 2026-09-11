//! rust_go_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::rust_go_plugin::RustGoPlugin;
    use crate::plugins::test_utils::*;

    /// 测试：Rust 编译样例被识别。
    #[test]
    fn detects_rust_case() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_001_rust_warning");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    /// 测试：Go panic 样例被压缩且不扩张。
    #[test]
    fn compresses_go_panic_case() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_002_go_panic");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        // 新格式不再使用 IR 标签，直接是原始格式或路径字典化后的格式
        assert!(out.contains("goroutine") || out.contains("$P"));
    }

    /// 易误判：Python Traceback 里掺了 `error:`/`warning:` 关键字，不得被识别为 Rust。
    #[test]
    fn does_not_detect_python_traceback_that_contains_rust_keywords() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_013_looks_rust_but_python");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_none(),
            "rust_go 不应把 Python Traceback 误识别为 Rust/Go"
        );
    }

    /// 易误判：Java 堆栈与 Go 栈帧格式近似，不应命中。
    #[test]
    fn does_not_detect_java_exception_stack_as_go() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_014_looks_go_but_java");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_none(),
            "rust_go 不应把 Java 堆栈误识别为 Go"
        );
    }

    /// 功能 1: 测试 Cargo 编译输出折叠
    #[test]
    fn compresses_cargo_compiling_output() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_015_cargo_compiling");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 应该包含折叠标记
        assert!(
            out.contains("[CARGO] Compiling"),
            "应该包含 Cargo 编译折叠标记"
        );
        assert!(
            out.contains("crates (details suppressed)"),
            "应该包含折叠说明"
        );

        // 不应该包含所有单独的 Compiling 行
        let compiling_count = out.matches("Compiling libc").count();
        assert_eq!(compiling_count, 0, "不应该包含单独的 Compiling 行");
    }

    /// 功能 2: 测试错误码统计
    #[test]
    fn extracts_error_code_stats() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_016_error_code_stats");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 应该包含错误码统计
        assert!(out.contains("[ERROR_STATS]"), "应该包含错误码统计标记");
        assert!(out.contains("E0425"), "应该包含 E0425 错误码");
        assert!(out.contains("E0308"), "应该包含 E0308 错误码");
        assert!(out.contains("occurred"), "应该包含出现次数");
        assert!(out.contains("error"), "压缩后必须保留 error 信号");
        assert!(out.len() <= raw.len(), "错误码统计不得绕过 ROI 门控");
    }

    /// 功能 3: 测试 Cargo test 输出压缩
    #[test]
    fn compresses_cargo_test_output() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_017_cargo_test");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 应该包含测试摘要
        assert!(out.contains("[TEST]"), "应该包含测试标记");
        assert!(out.contains("passed"), "应该包含通过数量");
        assert!(out.contains("failed"), "应该包含失败数量");

        // 应该保留失败的测试名称
        assert!(
            out.contains("test_fail_divide_by_zero") || out.contains("FAILED"),
            "应该保留失败的测试"
        );
        let lower = out.to_ascii_lowercase();
        assert!(
            lower.contains("error") || lower.contains("panic"),
            "Cargo test 压缩后必须保留 error/panic 信号"
        );
        assert!(out.len() <= raw.len(), "Cargo test 压缩不得扩张");

        // 不应该包含所有通过的测试详情
        let test_ok_count = out.matches("test tests::test_add ... ok").count();
        assert_eq!(test_ok_count, 0, "不应该包含所有通过的测试详情");
    }

    /// 集成回归：302 字节 cargo CLI 报错（脱色日志，无真实 ESC 字节）。
    ///
    /// 该样本在历史回归中一次性暴露三个独立失败，本测试用完整压缩流水线
    /// （`get_plugins()` 全链 + `debug_audit_jsonl` 捕获 plugin_effects）逐一锁定：
    /// 1. 裸 CSI 残留（`[1m[91m` 等）必须被剥离、回写进输出——不得原样保留；
    /// 2. 压缩率必须 < 0.75——30.1% 输入是纯 ANSI 垃圾，至少配额剥离后的压缩收益；
    /// 3. `rust_go` 必须进入 `plugin_effects`（被 dispatcher 调度并真实执行），
    ///    证明小输入整块处理让跨行信号粘合集齐，专用插件不再被切片剁成单句而漏检。
    #[test]
    fn cargo_err_302_removes_naked_ansi_and_routes_to_rust_go() {
        let raw = read_sample_log("rust_go_plugin", "case_019_cargo_err_302");
        // fixture 校验：样本本身不含真实 ESC 字节，否则「脱色日志」语义就失真
        assert!(
            !raw.contains('\x1b'),
            "fixture 应为脱色日志，不含真实 ESC 字节"
        );
        // fixture 校验：样本必须小于小输入整块处理阈值(<2048)，否则该样本就不算「小输入」回归
        assert!(
            raw.len() < 2048,
            "fixture 应小于小输入整块阈值，实际 {}B",
            raw.len()
        );

        // 开启调试审计，捕获本次压缩的 plugin_effects（调度与执行贡献明细）
        let audit_path =
            std::env::temp_dir().join(format!("tokenslim-rustgo-302-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&audit_path);
        let mut config = crate::core::compression_pipeline::PipelineConfig::default();
        config.debug_audit_jsonl = Some(audit_path.clone());

        let metrics =
            crate::core::metrics::MetricsCollector::new(crate::core::metrics::MetricsConfig {
                enabled: false,
                enable_module_timing: false,
                enable_plugin_stats: false,
                enable_error_logging: false,
                max_error_logs: 100,
            });
        let mut pipeline = crate::core::compression_pipeline::CompressionPipeline::new(
            config,
            crate::cli::get_plugins(),
            metrics,
        );
        let output = pipeline.compress_str(&raw).expect("pipeline 压缩不应失败");

        // 断言 1：输出不含任何裸 CSI 残留（回写剥离生效）
        let compact = output
            .tokens
            .iter()
            .filter_map(|t| match t {
                crate::core::compression::Token::Text(s) => Some(s.to_string()),
                _ => None,
            })
            .collect::<String>();
        assert!(
            !compact.contains("[1m") && !compact.contains("[0m"),
            "输出必须剥离裸 CSI，实际输出: {compact}"
        );
        assert!(
            !compact.contains("[91m")
                && !compact.contains("[93m")
                && !compact.contains("[92m")
                && !compact.contains("[96m")
                && !compact.contains("[36m"),
            "输出必须剥离颜色裸码，实际输出: {compact}"
        );

        // 断言 3（先验）：rust_go 必须进入 plugin_effects（整块处理让专用插件被调度并执行）
        let audit_content = std::fs::read_to_string(&audit_path).expect("审计文件应存在");
        let _ = std::fs::remove_file(&audit_path);
        let event: serde_json::Value =
            serde_json::from_str(audit_content.trim()).expect("合法 JSONL");
        eprintln!(
            "[regress] input={}B output={}B ratio={}",
            raw.len(),
            output.metadata.compressed_size,
            output.metadata.compression_ratio
        );
        eprintln!("[regress] compact: {compact}");
        eprintln!("[regress] plugin_effects: {}", event["plugin_effects"]);
        let rust_go_effect = event["plugin_effects"]
            .as_array()
            .expect("plugin_effects 数组")
            .iter()
            .find(|e| e["plugin_id"] == "rust_go")
            .cloned();
        assert!(
            rust_go_effect.is_some(),
            "rust_go 必须被调度并进入 plugin_effects，实际 effects: {}",
            event["plugin_effects"]
        );
        let effect = rust_go_effect.expect("已断言 Some");
        assert!(
            effect["invocation_count"].as_u64().unwrap_or(0) >= 1,
            "rust_go 至少执行一次压缩"
        );

        // 断言 4：ANSI 剥离必须删除字节（红灯指标——输入含裸码而该值为 0 即有 bug）
        let stripped = event["ansi_strip_bytes_removed"].as_u64().unwrap_or(0);
        assert!(
            stripped > 0,
            "ANSI 剥离必须删除字节，实际删除 {stripped}（输入 {raw}B 含裸 CSI）"
        );

        // 断言 5：coverage 必须 > 0（红灯指标——专用插件未真实参与压缩即接线失效）
        let coverage = event["coverage"].as_f64().unwrap_or(0.0);
        assert!(
            coverage > 0.0,
            "coverage 必须 > 0，实际 {coverage}（rust_go 未真实改变字节）"
        );

        // 断言 2：压缩率必须 < 0.75（30.1% 输入是纯 ANSI 垃圾，剥离后配额出压缩收益）
        assert!(
            output.metadata.compression_ratio < 0.75,
            "压缩率必须 < 0.75，实际 {}（{}/{} 字节），output: {compact}",
            output.metadata.compression_ratio,
            output.metadata.compressed_size,
            raw.len()
        );
    }

    /// 功能 4: 测试 Go test 输出压缩
    #[test]
    fn compresses_go_test_output() {
        let plugin = RustGoPlugin::new();
        let raw = read_sample_log("rust_go_plugin", "case_018_go_test");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        // 应该包含 Go 测试摘要
        assert!(out.contains("[GO TEST]"), "应该包含 Go 测试标记");
        assert!(out.contains("passed"), "应该包含通过数量");
        assert!(out.contains("failed"), "应该包含失败数量");

        // 应该保留失败的测试
        assert!(
            out.contains("TestDivideByZero") || out.contains("FAIL"),
            "应该保留失败的测试"
        );
        assert!(out.contains("error"), "Go test 压缩后必须保留 error 信号");
        assert!(out.len() <= raw.len(), "Go test 压缩不得扩张");

        // 不应该包含所有通过的测试详情
        let test_pass_count = out.matches("=== RUN   TestAdd").count();
        assert_eq!(test_pass_count, 0, "不应该包含所有通过的测试详情");
    }

    /// P3-206① 回归（负路径先行）：`running N tests` 块首锚点不得被 verbose 正文行稀释。
    ///
    /// ≥2048B 输入走段落/分块切片后，插件看到的切片形如「`running N tests` + N 行
    /// `test <path> ... ok`」。verbose 正文行对 detect 的任何特征都零贡献，会把锚点的 2 分
    /// 稀释进 15 行窗口（2/15≈0.133 < 0.15 阈值），导致 rust_go 永不入选、跨行折叠整体失效。
    /// 本测试先证明「旧口径确实漏判」，再断言锚点短路必须命中。
    #[test]
    fn detects_cargo_test_head_anchor_without_dilution_by_verbose_body() {
        let raw = read_sample_log("rust_go_plugin", "case_020_cargo_test_verbose_large");
        let anchor_start = raw
            .find("running ")
            .expect("样本必须含 `running N tests` 块首锚点");
        let block = &raw[anchor_start..];
        assert!(
            block.len() >= 2048,
            "锚点块须为 ≥2048B 的大输入段落形态，实际 {}B",
            block.len()
        );

        // 复现旧口径的漏判：15 行窗口内仅块首 1 行带特征 → 2/15≈0.133 < 0.15。
        let first15: Vec<&str> = block.lines().take(15).collect();
        let anchor_hits = first15.iter().filter(|l| l.starts_with("running ")).count();
        let legacy_ratio = anchor_hits as f32 / first15.len() as f32;
        assert!(
            legacy_ratio < 0.15,
            "旧口径若已 ≥0.15 则本回归失去意义，实际 {legacy_ratio}"
        );

        let plugin = RustGoPlugin::new();
        let score = plugin.detect(&make_log_slice(block));
        assert_eq!(
            score,
            Some(1.0),
            "块首 `running N tests` 锚点必须直接满分命中，不得被 verbose 正文稀释（P3-206①）"
        );
    }

    /// P3-206① 回归：≥2048B 的 verbose cargo test 样本（含 1 个失败用例）必须被跨行折叠为 [TEST] 摘要。
    #[test]
    fn folds_large_cargo_test_verbose_sample() {
        let raw = read_sample_log("rust_go_plugin", "case_020_cargo_test_verbose_large");
        assert!(
            raw.len() >= 2048,
            "该样本须为 ≥2048B 大输入，实际 {}B",
            raw.len()
        );

        let plugin = RustGoPlugin::new();
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);

        assert!(
            out.contains("[TEST] Running 56 tests"),
            "必须产出 [TEST] 摘要头，实际输出：{out}"
        );
        assert!(
            out.contains("55 passed, 1 failed, 0 ignored"),
            "权威 test result 计数（passed/failed/ignored）必须被摘要承接，实际输出：{out}"
        );
        assert!(
            out.contains("lexer::reports_unterminated_block_comment"),
            "失败用例名必须保留（Anti-Amnesia rule 5），实际输出：{out}"
        );
        assert!(
            !out.contains("test arith::adds_two_positive_integers ... ok"),
            "通过用例逐行明细必须被折叠丢弃，实际输出：{out}"
        );
        assert!(
            out.len() < raw.len(),
            "折叠后不得扩张：{} ≥ {}",
            out.len(),
            raw.len()
        );
    }

    /// P3-206① 反例：`running unit tests for parser` 一类普通文本不得被块首锚点短路满分抢占。
    ///
    /// 旧谓词 `line.contains(" tests")` 会把它误判为 cargo test 块首；锚点短路把命中提到满分后
    /// 即抢走 `cloud_log_plugin/case_044_non_cloud_plain`（cloud 负样本）。谓词收紧为
    /// 「`running ` + 十进制计数 + `tests`/`test`」后，rust_go 对该样本应完全不命中。
    #[test]
    fn rejects_plain_text_line_that_merely_contains_tests() {
        let raw = read_sample_log("cloud_log_plugin", "case_044_non_cloud_plain");
        assert!(
            raw.lines().any(|l| l.starts_with("running ")),
            "样本须含以 `running ` 开头的行，否则本条回归失去意义"
        );
        let plugin = RustGoPlugin::new();
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_none(),
            "普通文本中的 `running unit tests for parser` 不得被判定为 cargo test 块首"
        );
    }
}
