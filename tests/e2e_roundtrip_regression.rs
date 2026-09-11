use bumpalo::Bump;
use std::borrow::Cow;
use tokenslim::core::compression::CompressionOutput;
use tokenslim::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
use tokenslim::core::dedup_engine::DedupEngine;
use tokenslim::core::dictionary_engine::{Dictionary, DictionaryEngine};
use tokenslim::core::metrics::{MetricsCollector, MetricsConfig};
use tokenslim::core::plugin_dispatcher::{CompressResult, Plugin};
use tokenslim::core::rehydration_pipeline::{RehydrationConfig, RehydrationPipeline};
use tokenslim::core::text_slicer::{Slice, SliceMode};

/// 测试桩插件：原样透传文本、不修改 token 流，作为往返一致性测试的基准参照。
struct IdentityPlugin;

impl Plugin for IdentityPlugin {
    /// 返回插件标识名 "identity"。
    fn name(&self) -> &'static str {
        "identity"
    }

    /// 最高优先级 255，保证分发时优先命中本桩插件。
    fn priority(&self) -> u8 {
        255
    }

    /// 恒返回 1.0 置信度，任何输入都判定归属本插件。
    fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
        Some(1.0)
    }

    /// 将输入文本原样包装为单个 Text token 输出，不做任何压缩。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        CompressResult {
            tokens: vec![tokenslim::core::compression::Token::Text(Cow::Owned(
                slice.text.to_string(),
            ))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 用字典递归解析压缩串（IdentityPlugin 未产生字典条目，等价于原样返回）。
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        dict.resolve_recursive(compressed)
    }
}

/// 构造全禁用指标的 MetricsCollector（测试场景无需采集开销）。
fn default_metrics() -> MetricsCollector {
    MetricsCollector::new(MetricsConfig {
        enabled: false,
        enable_module_timing: false,
        enable_plugin_stats: false,
        enable_error_logging: false,
        max_error_logs: 0,
    })
}

/// 构建行切片模式、禁用字典阈值的压缩流水线（保证按行独立压缩便于往返比对）。
fn build_pipeline(plugins: Vec<Box<dyn Plugin>>) -> CompressionPipeline {
    let mut config = PipelineConfig::default();
    config.slicer_config.mode = SliceMode::Line;
    config.dictionary_threshold = 0;
    CompressionPipeline::new(config, plugins, default_metrics())
}

/// 用给定插件集对压缩输出做再水合（rehydrate），失败时返回错误字符串。
fn rehydrate_with_plugins(
    output: &CompressionOutput,
    plugins: Vec<Box<dyn Plugin>>,
) -> Result<String, String> {
    let rehydrator = RehydrationPipeline::new(
        output.dictionary.clone(),
        plugins,
        RehydrationConfig {
            fallback_on_error: false,
        },
    );
    rehydrator.rehydrate(output).map_err(|e| e.to_string())
}

/// 串行往返一致性：4 类代表性样本（编译错误/Java 异常/Webpack 警告/HTTP 日志）
/// 经压缩→再水合后必须与原始输入逐字节一致。
#[test]
fn roundtrip_serial_core_samples_consistent() {
    let sample_cases = [
        "build failed: /jenkins/workspace/app/src/main.c:102:13 error: undefined reference to foo",
        "Exception in thread main java.lang.RuntimeException at com.demo.App.main(App.java:42)",
        "webpack compile warning in C:\\\\repo\\\\project\\\\src\\\\index.tsx with -O2 -Wall",
        "2026-03-11T10:20:30Z HTTP 500 /api/v1/login request_id=abc123",
    ];

    for (idx, input) in sample_cases.iter().enumerate() {
        let mut pipeline = build_pipeline(vec![Box::new(IdentityPlugin)]);
        let output = pipeline
            .compress_str(input)
            .unwrap_or_else(|e| panic!("compress_str failed for case {}: {}", idx, e));

        let restored = rehydrate_with_plugins(&output, vec![Box::new(IdentityPlugin)])
            .unwrap_or_else(|e| panic!("rehydrate failed for case {}: {}", idx, e));

        assert_eq!(restored, *input, "roundtrip mismatch for case {}", idx);
    }
}

/// 并行大输入往返一致性：约 1.1MB 重复行输入压缩→再水合必须还原，
/// 失败时落盘 test_restored.txt/test_input.txt 便于诊断。
#[test]
fn roundtrip_parallel_large_input_consistent() {
    let base = "parallel-case::jenkins_workspace_build_root_project_sdk_acme_corp_build_include::gcc -O2 -Wall::token\n";
    let mut input = String::new();
    while input.len() < (1024 * 1024 + 64 * 1024) {
        input.push_str(base);
    }

    let mut pipeline = build_pipeline(vec![Box::new(IdentityPlugin)]);
    let output = pipeline
        .compress_str(&input)
        .unwrap_or_else(|e| panic!("parallel compress_str failed: {}", e));

    let restored = rehydrate_with_plugins(&output, vec![Box::new(IdentityPlugin)])
        .unwrap_or_else(|e| panic!("rehydrate failed: {}", e));

    if restored != input {
        std::fs::write("test_restored.txt", &restored).unwrap();
        std::fs::write("test_input.txt", &input).unwrap();
        panic!(
            "restored len: {}, input len: {}",
            restored.len(),
            input.len()
        );
    }
    assert_eq!(restored, input);
}
