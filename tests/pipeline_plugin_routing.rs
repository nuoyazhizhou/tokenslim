//! 管线级「插件路由」回归（P3-206① 附带：弥补审计的 detect/调度层盲区）。
//!
//! ## 为什么需要这个文件
//!
//! 审计（`audit_case_metrics.py`）与 `showcase.rs::compress_text` 都是**直接对整份文件调用
//! `plugin.compress()`**，绕过 `detect` 与切片层。因此 detect/调度层的缺陷在审计中
//! **结构上不可见**——P3-206① 的 `rust_go` 锚点稀释 + 跨插件超分即属此类：审计
//! `frozen_changed=0`，而真实管线下 `cargo test` 样本被 pytest / nodejs 抢走、折叠失效。
//!
//! 本文件用真实 `CompressionPipeline`（全量插件链）跑样本，经
//! `get_metrics().snapshot().plugin_stats` 读出**实际接管过压缩的插件集合**做断言。
//!
//! ## 断言语义（重要：不要用「调用次数最多者」当路由判据）
//!
//! 实测 `case_017_cargo_test`（5390B）真实管线路由指纹为
//! `pytest=4, rust_go=3, generic_text=2`——**聚合计数会被旁枝切片主导**：关键的
//! `running N tests … test result:` 块确实由 rust_go 接管（折叠成立），但 panic/backtrace/
//! `failures:` 等旁枝切片分别落到 pytest/generic_text，使计数反超。
//! 故本文件只做**存在性**断言（某插件必须出现 / 必须不出现）+ **行为**断言（折叠是否成立），
//! 二者都与切片粒度无关。
//!
//! 样本一律物理加载 `samples/`（压缩协议致命红线 2：禁止手写 mock 字符串）。

use std::collections::HashMap;
use tokenslim::core::compression::{CompressionOutput, Token};
use tokenslim::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
use tokenslim::core::metrics::{MetricsCollector, MetricsConfig};

/// 启用插件统计（路由断言依赖 `plugin_stats[*].compress_calls`），关掉其余噪声。
fn metrics() -> MetricsCollector {
    MetricsCollector::new(MetricsConfig {
        enabled: true,
        enable_module_timing: false,
        enable_plugin_stats: true,
        enable_error_logging: false,
        max_error_logs: 0,
    })
}

/// 物理加载样本。
fn read_sample(dir: &str, stem: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join(dir)
        .join(format!("{stem}.log"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读取物理样本 {} 失败: {e}", path.display()))
}

/// 用真实流水线压缩，返回 (拼接后的 Text token, 插件名 → 压缩调用次数)。
fn run_pipeline(text: &str) -> (String, HashMap<String, usize>) {
    let mut pipeline = CompressionPipeline::new(
        PipelineConfig::default(),
        tokenslim::cli::get_plugins(),
        metrics(),
    );
    let out: CompressionOutput = pipeline.compress_str(text).expect("compress_str 失败");
    let joined: String = out
        .tokens
        .iter()
        .filter_map(|t| match t {
            Token::Text(c) => Some(c.as_ref()),
            _ => None,
        })
        .collect();
    let stats = pipeline
        .get_metrics()
        .snapshot()
        .plugin_stats
        .iter()
        .map(|(name, s)| (name.clone(), s.compress_calls))
        .collect();
    (joined, stats)
}

/// 正路由守卫：`plugin` 必须接管过至少一个切片。
fn assert_plugin_present(stats: &HashMap<String, usize>, plugin: &str, sample: &str) {
    assert!(
        stats.get(plugin).copied().unwrap_or(0) > 0,
        "{sample} 的真实管线路由中 {plugin} 必须接管至少一个切片，实际指纹：{stats:?}"
    );
}

/// 负路由守卫：`plugin` 不得接管任何切片（防误命中）。
fn assert_plugin_absent(stats: &HashMap<String, usize>, plugin: &str, sample: &str) {
    assert_eq!(
        stats.get(plugin).copied().unwrap_or(0),
        0,
        "{sample} 不应被 {plugin} 接管，实际指纹：{stats:?}"
    );
}

// ---------------------------------------------------------------------------
// 正路由守卫：rust_go 家族样本必须真的由 rust_go 处理（否则跨行折叠无从谈起）
// ---------------------------------------------------------------------------

/// `case_020`（3531B cargo test verbose）必须由 rust_go 接管并折叠。
///
/// 修复前该样本在真实管线下路由到 nodejs(0.85)，rust_go 完全缺席、产物≈原文。
#[test]
fn cargo_test_verbose_large_is_routed_to_rust_go_and_folds() {
    let raw = read_sample("rust_go_plugin", "case_020_cargo_test_verbose_large");
    let (joined, stats) = run_pipeline(&raw);
    assert_plugin_present(&stats, "rust_go", "case_020_cargo_test_verbose_large");
    assert!(
        joined.contains("[TEST] Running 56 tests"),
        "真实管线必须折叠出 [TEST] 摘要，实际输出：{joined}"
    );
}

/// `case_017`（5390B cargo test，含 3 个失败）必须由 rust_go 接管并折叠。
///
/// 修复前该样本在真实管线下被 pytest(0.90) 抢走主块，rust_go 抓不到锚点段落。
#[test]
fn cargo_test_with_failures_is_routed_to_rust_go_and_folds() {
    let raw = read_sample("rust_go_plugin", "case_017_cargo_test");
    let (joined, stats) = run_pipeline(&raw);
    assert_plugin_present(&stats, "rust_go", "case_017_cargo_test");
    assert!(
        joined.contains("[TEST] Running 120 tests"),
        "真实管线必须折叠出 [TEST] 摘要，实际输出：{joined}"
    );
}

/// `case_018`（go test）必须由 rust_go 接管。
#[test]
fn go_test_sample_is_routed_to_rust_go() {
    let raw = read_sample("rust_go_plugin", "case_018_go_test");
    let (_, stats) = run_pipeline(&raw);
    assert_plugin_present(&stats, "rust_go", "case_018_go_test");
}

// ---------------------------------------------------------------------------
// 负路由守卫：不属于 rust_go 的内容不得被 rust_go 接管（防误命中）
// ---------------------------------------------------------------------------

/// `case_013`（Python Traceback 里掺了 `error:`）不得被 rust_go 接管。
#[test]
fn python_traceback_is_not_routed_to_rust_go() {
    let raw = read_sample("rust_go_plugin", "case_013_looks_rust_but_python");
    let (_, stats) = run_pipeline(&raw);
    assert_plugin_absent(&stats, "rust_go", "case_013_looks_rust_but_python");
}

/// `case_014`（Java 堆栈与 Go 栈帧格式近似）不得被 rust_go 接管。
#[test]
fn java_stack_is_not_routed_to_rust_go() {
    let raw = read_sample("rust_go_plugin", "case_014_looks_go_but_java");
    let (_, stats) = run_pipeline(&raw);
    assert_plugin_absent(&stats, "rust_go", "case_014_looks_go_but_java");
}

/// P3-206① 附带回归：普通文本 `running unit tests for parser` 不得被 rust_go 接管。
///
/// `is_cargo_test_head` 旧谓词 `contains(" tests")` 会把它判为 cargo test 块首；在锚点
/// 短路把命中提到满分后，rust_go 会以 1.0 抢走该 cloud 负样本
/// （`cloud_log_plugin/case_044_non_cloud_plain`）。谓词收紧后应完全不命中。
#[test]
fn plain_text_with_tests_word_is_not_routed_to_rust_go() {
    let raw = read_sample("cloud_log_plugin", "case_044_non_cloud_plain");
    assert!(
        raw.lines().any(|l| l.starts_with("running ")),
        "样本须含以 `running ` 开头的行，否则本条回归失去意义"
    );
    let (_, stats) = run_pipeline(&raw);
    assert_plugin_absent(&stats, "rust_go", "cloud_log_plugin/case_044_non_cloud_plain");
}

// ---------------------------------------------------------------------------
// ls_listing（P3-206②）：列式清单正/负路由守卫
// ---------------------------------------------------------------------------

/// `aws s3 ls --recursive` 大输入必须由 ls_listing 接管并折叠出 [LS] 块头。
///
/// 修复前无插件认领列式清单命令族，3.4MB 输入仅省 1.7%（路由只有 smart_path）。
#[test]
fn s3_ls_listing_is_routed_to_ls_listing_and_folds() {
    let raw = read_sample("ls_listing_plugin", "case_001_s3_ls_recursive");
    let record_count = raw
        .lines()
        .skip(1)
        .filter(|l| {
            let t = l.trim_start();
            t.len() >= 20
                && t.as_bytes()[..4].iter().all(|b| b.is_ascii_digit())
                && t.as_bytes().get(4) == Some(&b'-')
        })
        .count();
    let (joined, stats) = run_pipeline(&raw);
    assert_plugin_present(&stats, "ls_listing", "case_001_s3_ls_recursive");
    assert!(
        joined.contains(&format!("[LS] {record_count} entries")),
        "真实管线必须折叠出与记录数（{record_count}）一致的 [LS] 摘要头，实际输出前 300 字：{}",
        &joined[..joined.len().min(300)]
    );
}

/// 既有非清单样本不得被 ls_listing 接管（防误命中；设计稿 §四 的收窄约束）。
#[test]
fn non_listing_samples_are_not_routed_to_ls_listing() {
    for (dir, stem) in [
        ("cloud_log_plugin", "case_044_non_cloud_plain"),
        ("rust_go_plugin", "case_017_cargo_test"),
        ("shell_session_plugin", "case_053_ps_cargo"),
    ] {
        let raw = read_sample(dir, stem);
        let (_, stats) = run_pipeline(&raw);
        assert_plugin_absent(&stats, "ls_listing", &format!("{dir}/{stem}"));
    }
}
