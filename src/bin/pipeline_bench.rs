use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use tiktoken_rs::cl100k_base;
use tokenslim::core::compression::Token;
use tokenslim::core::compression_pipeline::{
    CompressionOutput, CompressionPipeline, PipelineConfig,
};
use tokenslim::core::metrics::{MetricsCollector, MetricsConfig};
use tokenslim::core::plugin_dispatcher::Plugin;
use tokenslim::plugins::android_gradle_plugin::AndroidGradlePlugin;
use tokenslim::plugins::gcc_log_plugin::GccLogPlugin;
use tokenslim::plugins::java_stack_plugin::JavaStackPlugin;
use tokenslim::plugins::nodejs_plugin::NodeJsPlugin;
use tokenslim::plugins::python_traceback_plugin::PythonTracebackPlugin;
use tokenslim::plugins::smart_path_plugin::SmartPathPlugin;

#[derive(Clone, Debug)]
struct Scenario {
    name: &'static str,
    mmap_enabled: bool,
    parallel_enabled: bool,
}

#[derive(Clone, Debug)]
struct Sample {
    elapsed_ms: f64,
    json_bytes: usize,
    token_ratio: f64,
}

#[derive(Clone, Debug)]
struct ScenarioResult {
    scenario: Scenario,
    avg_elapsed_ms: f64,
    min_elapsed_ms: f64,
    max_elapsed_ms: f64,
    avg_json_bytes: f64,
    avg_token_ratio: f64,
    throughput_mb_s: f64,
}

fn build_plugins() -> Vec<Box<dyn Plugin>> {
    vec![
        Box::new(AndroidGradlePlugin::new()),
        Box::new(GccLogPlugin::new()),
        Box::new(JavaStackPlugin::new()),
        Box::new(NodeJsPlugin::new()),
        Box::new(PythonTracebackPlugin::new()),
        Box::new(SmartPathPlugin::default()),
    ]
}

fn metrics_collector() -> MetricsCollector {
    MetricsCollector::new(MetricsConfig {
        enabled: false,
        enable_module_timing: false,
        enable_plugin_stats: false,
        enable_error_logging: false,
        max_error_logs: 0,
    })
}

fn make_pipeline(input_size: usize, s: &Scenario) -> CompressionPipeline {
    let mut config = PipelineConfig::default();
    config.dictionary_threshold = 0;
    config.parallel_threshold = if s.parallel_enabled { 1 } else { usize::MAX };
    config.stream_mmap_threshold = if s.mmap_enabled {
        Some(1)
    } else {
        Some(input_size.saturating_add(1024))
    };
    CompressionPipeline::new(config, build_plugins(), metrics_collector())
}

fn build_llm_payload(output: &CompressionOutput) -> String {
    let mut payload = String::new();
    payload.push_str("[DICT]\n");
    for (k, v) in &output.dictionary.paths {
        payload.push_str(&format!("{}={}\n", k, v));
    }
    for (k, v) in &output.dictionary.packages {
        payload.push_str(&format!("{}={}\n", k, v));
    }
    for (k, v) in &output.dictionary.macros {
        payload.push_str(&format!("{}={}\n", k, v));
    }
    for (k, v) in &output.dictionary.files {
        payload.push_str(&format!("{}={}\n", k, v));
    }
    payload.push_str("[DATA]\n");
    for t in &output.tokens {
        if let Token::Text(s) = t {
            payload.push_str(s.as_ref());
        }
    }
    payload
}

fn average(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.iter().sum::<f64>() / v.len() as f64
}

fn parse_arg(args: &[String], key: &str, default: &str) -> String {
    if let Some(i) = args.iter().position(|v| v == key) {
        if let Some(v) = args.get(i + 1) {
            return v.to_string();
        }
    }
    default.to_string()
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|v| v == flag)
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| format!("create dir failed: {e}"))?;
        }
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let input = PathBuf::from(parse_arg(&args, "--input", "benchmarks/input_100mb.txt"));
    let report = PathBuf::from(parse_arg(&args, "--report", "docs/benchmark_report.md"));
    let json_out = PathBuf::from(parse_arg(&args, "--json", "docs/benchmark_report.json"));
    let iterations: usize = parse_arg(&args, "--iterations", "3")
        .parse()
        .map_err(|e| format!("invalid --iterations: {e}"))?;
    let scenario_filter = parse_arg(&args, "--scenario", "all");
    let skip_tokenize = has_flag(&args, "--skip-tokenize");

    if !input.exists() {
        return Err(format!("input file not found: {}", input.display()));
    }
    let input_size = fs::metadata(&input)
        .map_err(|e| format!("read metadata failed: {e}"))?
        .len() as usize;
    let input_text =
        fs::read_to_string(&input).map_err(|e| format!("read input text failed: {e}"))?;

    let bpe = if skip_tokenize {
        None
    } else {
        Some(cl100k_base().map_err(|e| format!("tiktoken init failed: {e}"))?)
    };
    let original_tokens = if let Some(bpe) = &bpe {
        bpe.encode_with_special_tokens(&input_text).len()
    } else {
        0
    };

    let all_scenarios = vec![
        Scenario {
            name: "mmap+parallel",
            mmap_enabled: true,
            parallel_enabled: true,
        },
        Scenario {
            name: "mmap+serial",
            mmap_enabled: true,
            parallel_enabled: false,
        },
        Scenario {
            name: "non_mmap+parallel",
            mmap_enabled: false,
            parallel_enabled: true,
        },
        Scenario {
            name: "non_mmap+serial",
            mmap_enabled: false,
            parallel_enabled: false,
        },
    ];
    let scenarios: Vec<Scenario> = if scenario_filter == "all" {
        all_scenarios
    } else {
        all_scenarios
            .into_iter()
            .filter(|s| s.name == scenario_filter)
            .collect()
    };
    if scenarios.is_empty() {
        return Err(format!(
            "invalid --scenario: {} (expected one of: all, mmap+parallel, mmap+serial, non_mmap+parallel, non_mmap+serial)",
            scenario_filter
        ));
    }

    let mut results: Vec<ScenarioResult> = Vec::new();
    let mut raw_rows = Vec::new();

    for s in &scenarios {
        println!("[bench] scenario={} iterations={}", s.name, iterations);
        let mut samples: Vec<Sample> = Vec::new();
        for iter_idx in 0..iterations {
            println!("[bench] start scenario={} iter={}", s.name, iter_idx + 1);
            let mut pipeline = make_pipeline(input_size, s);
            let start = Instant::now();
            let output = pipeline
                .compress_str(&input_text)
                .map_err(|e| format!("compress failed [{}]: {e}", s.name))?;
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            let tokens_count = output.tokens.len();
            let dict_size = output.dictionary.paths.len()
                + output.dictionary.packages.len()
                + output.dictionary.macros.len()
                + output.dictionary.directories.len();

            let json_bytes = serde_json::to_vec(&output)
                .map_err(|e| format!("serialize output failed [{}]: {e}", s.name))?
                .len();
            let token_ratio = if let Some(bpe) = &bpe {
                let llm_payload = build_llm_payload(&output);
                let compressed_tokens = bpe.encode_with_special_tokens(&llm_payload).len();
                if original_tokens > 0 {
                    compressed_tokens as f64 / original_tokens as f64
                } else {
                    0.0
                }
            } else {
                0.0
            };
            samples.push(Sample {
                elapsed_ms,
                json_bytes,
                token_ratio,
            });
            println!(
                "[bench] done scenario={} iter={} elapsed_ms={:.2} json_bytes={} tokens={} dict={}",
                s.name,
                iter_idx + 1,
                elapsed_ms,
                json_bytes,
                tokens_count,
                dict_size
            );
        }

        let elapsed: Vec<f64> = samples.iter().map(|s| s.elapsed_ms).collect();
        let json_sizes: Vec<f64> = samples.iter().map(|s| s.json_bytes as f64).collect();
        let ratios: Vec<f64> = samples.iter().map(|s| s.token_ratio).collect();
        let avg_elapsed_ms = average(&elapsed);
        let avg_json_bytes = average(&json_sizes);
        let avg_token_ratio = average(&ratios);
        let min_elapsed_ms = elapsed.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_elapsed_ms = elapsed.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let throughput_mb_s = if avg_elapsed_ms > 0.0 {
            (input_size as f64 / 1024.0 / 1024.0) / (avg_elapsed_ms / 1000.0)
        } else {
            0.0
        };

        results.push(ScenarioResult {
            scenario: s.clone(),
            avg_elapsed_ms,
            min_elapsed_ms,
            max_elapsed_ms,
            avg_json_bytes,
            avg_token_ratio,
            throughput_mb_s,
        });
    }

    let recommended_parallel_threshold = {
        let best_parallel = results
            .iter()
            .filter(|r| r.scenario.parallel_enabled)
            .min_by(|a, b| a.avg_elapsed_ms.partial_cmp(&b.avg_elapsed_ms).unwrap());
        let best_serial = results
            .iter()
            .filter(|r| !r.scenario.parallel_enabled)
            .min_by(|a, b| a.avg_elapsed_ms.partial_cmp(&b.avg_elapsed_ms).unwrap());
        match (best_parallel, best_serial) {
            (Some(p), Some(s)) => {
                if p.avg_elapsed_ms < s.avg_elapsed_ms {
                    1024 * 1024usize
                } else {
                    usize::MAX
                }
            }
            _ => usize::MAX,
        }
    };
    let recommended_mmap_threshold = {
        let mmap_best = results
            .iter()
            .filter(|r| r.scenario.mmap_enabled)
            .min_by(|a, b| a.avg_elapsed_ms.partial_cmp(&b.avg_elapsed_ms).unwrap());
        let non_mmap_best = results
            .iter()
            .filter(|r| !r.scenario.mmap_enabled)
            .min_by(|a, b| a.avg_elapsed_ms.partial_cmp(&b.avg_elapsed_ms).unwrap());
        match (mmap_best, non_mmap_best) {
            (Some(m), Some(nm)) => {
                if m.avg_elapsed_ms <= nm.avg_elapsed_ms {
                    1024 * 1024usize
                } else {
                    usize::MAX
                }
            }
            _ => usize::MAX,
        }
    };

    let generated_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %z");
    let mut md = String::new();
    md.push_str("# Pipeline Benchmark Report (C2)\n\n");
    md.push_str(&format!("Generated at: {}\n\n", generated_at));
    md.push_str(&format!("- Input file: `{}`\n", input.display()));
    md.push_str(&format!(
        "- Input size: {:.2} MB\n",
        input_size as f64 / 1024.0 / 1024.0
    ));
    md.push_str(&format!("- Iterations per scenario: {}\n", iterations));
    md.push_str(&format!(
        "- Original tokens (cl100k_base): {}\n",
        original_tokens
    ));
    md.push_str(&format!("- Scenario filter: `{}`\n", scenario_filter));
    md.push_str(&format!("- Skip tokenization: `{}`\n\n", skip_tokenize));
    md.push_str(
        "| scenario | avg ms | min ms | max ms | throughput MB/s | avg output MB (json) | avg token ratio |\n",
    );
    md.push_str("|---|---:|---:|---:|---:|---:|---:|\n");
    for r in &results {
        md.push_str(&format!(
            "| {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.4} |\n",
            r.scenario.name,
            r.avg_elapsed_ms,
            r.min_elapsed_ms,
            r.max_elapsed_ms,
            r.throughput_mb_s,
            r.avg_json_bytes / 1024.0 / 1024.0,
            r.avg_token_ratio
        ));
        raw_rows.push(serde_json::json!({
            "scenario": r.scenario.name,
            "avg_ms": r.avg_elapsed_ms,
            "min_ms": r.min_elapsed_ms,
            "max_ms": r.max_elapsed_ms,
            "throughput_mb_s": r.throughput_mb_s,
            "avg_output_bytes_json": r.avg_json_bytes,
            "avg_token_ratio": r.avg_token_ratio
        }));
    }
    md.push_str("\n## Threshold Recommendation\n\n");
    md.push_str(&format!(
        "- `parallel_threshold`: `{}` ({})\n",
        recommended_parallel_threshold,
        if recommended_parallel_threshold == usize::MAX {
            "prefer serial for this profile"
        } else {
            "prefer parallel from ~1MB"
        }
    ));
    md.push_str(&format!(
        "- `stream_mmap_threshold`: `{}` ({})\n",
        recommended_mmap_threshold,
        if recommended_mmap_threshold == usize::MAX {
            "prefer non-mmap for this profile"
        } else {
            "prefer mmap from ~1MB"
        }
    ));

    ensure_parent(&report)?;
    ensure_parent(&json_out)?;
    fs::write(&report, md).map_err(|e| format!("write report failed: {e}"))?;
    fs::write(
        &json_out,
        serde_json::to_string_pretty(&serde_json::json!({
            "input": input.display().to_string(),
            "input_size_bytes": input_size,
            "iterations": iterations,
            "original_tokens": original_tokens,
            "rows": raw_rows,
            "recommended_parallel_threshold": recommended_parallel_threshold,
            "recommended_stream_mmap_threshold": recommended_mmap_threshold
        }))
        .map_err(|e| format!("json encode failed: {e}"))?,
    )
    .map_err(|e| format!("write json report failed: {e}"))?;

    println!("Benchmark report written: {}", report.display());
    println!("Benchmark json written: {}", json_out.display());

    tokenslim::core::observability::dump_profile();
    Ok(())
}
