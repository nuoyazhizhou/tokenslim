use std::path::PathBuf;

use bumpalo::Bump;
use std::borrow::Cow;
use tokenslim::core::compression::CompressionOutput;
use tokenslim::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
use tokenslim::core::dedup_engine::DedupEngine;
use tokenslim::core::dictionary_engine::{Dictionary, DictionaryEngine};
use tokenslim::core::dynamic_plugin_loader::DynamicPlugin;
use tokenslim::core::metrics::{MetricsCollector, MetricsConfig};
use tokenslim::core::plugin_dispatcher::{CompressResult, Plugin};
use tokenslim::core::rehydration_pipeline::{RehydrationConfig, RehydrationPipeline};
use tokenslim::core::text_slicer::{Slice, SliceMode};

struct IdentityPlugin;

impl Plugin for IdentityPlugin {
    fn name(&self) -> &'static str {
        "identity"
    }

    fn priority(&self) -> u8 {
        255
    }

    fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
        Some(1.0)
    }

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

    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        dict.resolve_recursive(compressed)
    }
}

fn default_metrics() -> MetricsCollector {
    MetricsCollector::new(MetricsConfig {
        enabled: false,
        enable_module_timing: false,
        enable_plugin_stats: false,
        enable_error_logging: false,
        max_error_logs: 0,
    })
}

fn build_pipeline(plugins: Vec<Box<dyn Plugin>>) -> CompressionPipeline {
    let mut config = PipelineConfig::default();
    config.slicer_config.mode = SliceMode::Line;
    config.dictionary_threshold = 0;
    CompressionPipeline::new(config, plugins, default_metrics())
}

fn rehydrate_with_plugins(
    output: &CompressionOutput,
    plugins: Vec<Box<dyn Plugin>>,
) -> Result<String, String> {
    let rehydrator = RehydrationPipeline::new(
        output.dictionary.clone(),
        plugins,
        RehydrationConfig {
            preserve_order: false,
            fallback_on_error: false,
        },
    );
    rehydrator.rehydrate(output).map_err(|e| e.to_string())
}

fn dynamic_library_candidates() -> Vec<PathBuf> {
    let (prefix, ext) = if cfg!(target_os = "windows") {
        ("", "dll")
    } else if cfg!(target_os = "macos") {
        ("lib", "dylib")
    } else {
        ("lib", "so")
    };

    let names = [
        "db_log_plugin",
        "syslog_plugin",
        "web_log_plugin",
        "xcode_log_plugin",
        "rust_go_plugin",
    ];

    let mut paths = Vec::new();
    if let Ok(custom) = std::env::var("TOKENSLIM_DYNAMIC_PLUGIN_FILE") {
        paths.push(PathBuf::from(custom));
    }

    for name in names {
        paths.push(PathBuf::from(format!(
            "target/debug/{}{}.{}",
            prefix, name, ext
        )));
        paths.push(PathBuf::from(format!(
            "target/debug/deps/{}{}.{}",
            prefix, name, ext
        )));
    }
    paths
}

fn maybe_load_dynamic_plugin() -> Option<(PathBuf, DynamicPlugin)> {
    for candidate in dynamic_library_candidates() {
        if !candidate.exists() {
            continue;
        }
        if let Ok(plugin) = DynamicPlugin::new(candidate.to_str().unwrap(), "dynamic") {
            return Some((candidate, plugin));
        }
    }
    None
}

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

#[test]
fn roundtrip_mixed_static_dynamic_if_available() {
    let Some((dynamic_path, dynamic_plugin_for_compress)) = maybe_load_dynamic_plugin() else {
        eprintln!("dynamic plugin not available; skipping mixed regression test");
        return;
    };

    let mut pipeline = build_pipeline(vec![
        Box::new(IdentityPlugin),
        Box::new(dynamic_plugin_for_compress),
    ]);

    let input = "plain text for mixed static/dynamic roundtrip\nsecond line";
    let output = pipeline
        .compress_str(input)
        .unwrap_or_else(|e| panic!("compress_str failed: {}", e));

    let dynamic_plugin_for_rehydrate =
        DynamicPlugin::new(dynamic_path.to_str().unwrap(), "dynamic")
            .unwrap_or_else(|e| panic!("failed to reload dynamic plugin: {}", e));
    let restored = rehydrate_with_plugins(
        &output,
        vec![
            Box::new(IdentityPlugin),
            Box::new(dynamic_plugin_for_rehydrate),
        ],
    )
    .unwrap_or_else(|e| panic!("rehydrate failed: {}", e));

    assert_eq!(restored, input);
}
