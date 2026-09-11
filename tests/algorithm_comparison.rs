use std::time::Instant;

use tokenslim::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
use tokenslim::core::metrics::{MetricsCollector, MetricsConfig};
use tokenslim::core::plugin_dispatcher::Plugin;
use tokenslim::plugins::gcc_log_plugin::GccLogPlugin;
use tokenslim::plugins::smart_path_plugin::SmartPathPlugin;

/// 装配对比用插件集：GccLogPlugin + SmartPathPlugin（覆盖编译错误与路径两类场景）。
fn load_plugins() -> Vec<Box<dyn Plugin>> {
    vec![
        Box::new(GccLogPlugin::new()) as Box<dyn Plugin>,
        Box::new(SmartPathPlugin::new()) as Box<dyn Plugin>,
    ]
}

/// 用默认配置构建压缩流水线（指标采集全禁用）。
fn build_pipeline() -> CompressionPipeline {
    let config = PipelineConfig::default();
    let metrics = MetricsCollector::new(MetricsConfig {
        enabled: false,
        enable_module_timing: false,
        enable_plugin_stats: false,
        enable_error_logging: false,
        max_error_logs: 100,
    });
    CompressionPipeline::new(config, load_plugins(), metrics)
}

/// 冒烟测试：编译错误样本经流水线压缩应产出非空 token 且耗时非负。
#[test]
fn algorithm_comparison_smoke() {
    let input =
        "[2026-03-05T02:52:31.597Z] /home/user/project/src/main.c:12: error: unknown type\n";

    let mut pipeline = build_pipeline();
    let start = Instant::now();
    let output = pipeline
        .compress_str(input)
        .expect("compress should succeed");
    let elapsed = start.elapsed().as_secs_f64();

    assert!(
        !output.tokens.is_empty(),
        "compressed output should contain tokens"
    );
    assert!(elapsed >= 0.0, "elapsed time should be non-negative");
}
