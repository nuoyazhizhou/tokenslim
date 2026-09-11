use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::fs;
use std::hint::black_box;
use std::path::Path;
use tokenslim::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
use tokenslim::core::metrics::{MetricsCollector, MetricsConfig};
use tokenslim::core::stream_reader::StreamReader;

/// 加载基准输入数据：优先读取 `benchmarks/input_128kb.txt`，文件缺失时
/// 回退为固定重复文本以保证基准可运行。
fn load_benchmark_data() -> String {
    let path = Path::new("benchmarks/input_128kb.txt");
    if path.exists() {
        fs::read_to_string(path).unwrap()
    } else {
        // Fallback generic data if file is missing
        "This is a fallback text for benchmarking. ".repeat(1000)
    }
}

/// StreamReader 基准：以字节吞吐量度量 `iter_lines` 逐行迭代性能。
fn bench_stream_reader(c: &mut Criterion) {
    let data = load_benchmark_data();
    let mut group = c.benchmark_group("StreamReader");
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("iter_lines", |b| {
        b.iter(|| {
            let reader = StreamReader::from_str(black_box(&data));
            for line in reader.iter_lines() {
                black_box(line);
            }
        });
    });

    group.finish();
}

/// 压缩流水线基准：以字节吞吐量度量 `CompressionPipeline::compress_str`
/// 的整链压缩性能（关闭指标收集以减少测量噪声）。
fn bench_compression_pipeline(c: &mut Criterion) {
    let data = load_benchmark_data();
    let mut group = c.benchmark_group("CompressionPipeline");
    group.throughput(Throughput::Bytes(data.len() as u64));

    group.bench_function("compress_str", |b| {
        b.iter(|| {
            let config = PipelineConfig::default();
            let metrics = MetricsCollector::new(MetricsConfig {
                enabled: false,
                enable_module_timing: false,
                enable_plugin_stats: false,
                enable_error_logging: false,
                max_error_logs: 0,
            });
            let mut pipeline = CompressionPipeline::new(config, vec![], metrics);
            pipeline.compress_str(black_box(&data)).unwrap()
        });
    });

    group.finish();
}

criterion_group!(benches, bench_stream_reader, bench_compression_pipeline);
criterion_main!(benches);
