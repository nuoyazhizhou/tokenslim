//! 临时覆盖画像器（阶段 B，跑完即删）
//!
//! 读取 `.tokenslim/audit/corpus/*.jsonl` 的采集语料，对每条文本块跑一次完整压缩，
//! 通过 metrics 快照差集得到该块实际命中的插件分布，聚合出：
//!   1. 每个插件在所有块上的真实触发次数 / 主导块数 / 平均压缩率；
//!   2. 从未被触发的僵尸插件（判断哪些插件用得到、需不需要改造）；
//!   3. 落错块（主导插件 != 采集时标注的预期插件，或其主导插件是 generic_text），
//!      这批块直接喂阶段 C 的判别词挖掘。
//!
//! 运行（需先编译）：
//!     cargo run --bin corpus_profile
//! 输出人类可读汇总到 stdout，并写一份 JSON 报告到
//! `.tokenslim/audit/corpus/_profile_report.json`。

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde_json::json;
use tokenslim::cli::get_plugins;
use tokenslim::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
use tokenslim::core::metrics::MetricsSnapshot;

/// 语料 JSONL 目录。
const CORPUS_DIR: &str = ".tokenslim/audit/corpus";
/// 报告输出路径。
const REPORT_PATH: &str = ".tokenslim/audit/corpus/_profile_report.json";

/// 无插件命中时透传到的兜底插件名（以 compress_calls 计）。
const GENERIC_PLUGIN: &str = "generic_text";

/// 一条语料块记录，与采集器 collect_error_samples.py 的 JSONL 字段对齐。
#[derive(Debug, serde::Deserialize)]
struct Record {
    #[serde(default)]
    category: String,
    #[serde(default)]
    plugin: String, // 采集时标注的"预期插件"
    #[serde(default)]
    source: String,
    #[serde(default)]
    url: String,
    text: String,
}

/// 单个插件的聚合画像。
#[derive(Debug, Default)]
struct PluginProfile {
    triggered_blocks: usize, // 该插件 compress_calls>0 的块数
    dominant_blocks: usize,  // 作为主导插件（calls 最多）的块数
    total_ratio: f64,        // 累计压缩率，样本为 triggered 块
}

/// 取两条快照之间各插件的 `compress_calls` 差值，仅保留增量为正的插件。
/// 并行路径在线程间累加后再合入全局，单次 `compress_str` 返回后快照一致。
fn plugin_calls_delta(before: &MetricsSnapshot, after: &MetricsSnapshot) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    let keys: std::collections::HashSet<String> = after
        .plugin_stats
        .keys()
        .chain(before.plugin_stats.keys())
        .cloned()
        .collect();
    for k in keys {
        let a = after
            .plugin_stats
            .get(&k)
            .map(|s| s.compress_calls)
            .unwrap_or(0);
        let b = before
            .plugin_stats
            .get(&k)
            .map(|s| s.compress_calls)
            .unwrap_or(0);
        if a > b {
            out.insert(k, a - b);
        }
    }
    out
}

fn main() {
    let dir = PathBuf::from(CORPUS_DIR);
    let mut records: Vec<(String, Record)> = Vec::new();
    for entry in fs::read_dir(&dir).expect("无法读取语料目录") {
        let p = entry.expect("目录项读取失败").path();
        if p.extension().map(|e| e == "jsonl").unwrap_or(false) {
            let body = fs::read_to_string(&p).expect("读取语料文件失败");
            for line in body.lines().filter(|l| !l.trim().is_empty()) {
                let rec: Record = serde_json::from_str(line)
                    .unwrap_or_else(|e| panic!("语料解析失败 {}: {e}", p.display()));
                records.push((
                    p.file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    rec,
                ));
            }
        }
    }
    println!("载入语料 {} 条", records.len());

    // 构建与实际 compress 一致的单 pipeline（复用全量插件注册表）。
    let pipeline_config = PipelineConfig::default();
    let plugins = get_plugins();
    let mut pipeline = CompressionPipeline::new(
        pipeline_config,
        plugins,
        tokenslim::core::metrics::MetricsCollector::new(
            tokenslim::core::metrics::MetricsConfig::default(),
        ),
    );

    let mut profiles: HashMap<String, PluginProfile> = HashMap::new();
    let mut misrouted: Vec<serde_json::Value> = Vec::new();
    let mut block_rows: Vec<serde_json::Value> = Vec::new();
    let mut total = 0;
    let mut no_active = 0;

    for (src, rec) in &records {
        let before = pipeline.get_metrics().snapshot();
        let output = match pipeline.compress_str(&rec.text) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("压缩失败（块跳过）: {e}");
                continue;
            }
        };
        let after = pipeline.get_metrics().snapshot();
        let calls = plugin_calls_delta(&before, &after);
        total += 1;

        let ratio = output.metadata.compression_ratio as f64;
        let main_plugin = calls.iter().max_by_key(|(_, &c)| c).map(|(k, _)| k.clone());

        for (plugin, _) in &calls {
            let prof = profiles.entry(plugin.clone()).or_default();
            prof.triggered_blocks += 1;
            prof.total_ratio += ratio;
            if main_plugin.as_deref() == Some(plugin.as_str()) {
                prof.dominant_blocks += 1;
            }
        }

        // 预期插件进入活跃集，取决于 calls 中是否有它。
        let expected = if rec.plugin.is_empty() {
            rec.category.as_str()
        } else {
            rec.plugin.as_str()
        };
        let expected_used = calls.contains_key(expected);

        // 活跃插件列表（按 calls 降序）。
        let mut active: Vec<(&String, &usize)> = calls.iter().collect();
        active.sort_by(|a, b| b.1.cmp(a.1));

        // 落错判定：主导插件与预期不一致，或根本没有插件命中（透传 generic）。
        let is_generic = main_plugin.as_deref() == Some(GENERIC_PLUGIN) || main_plugin.is_none();
        let misrouted_flag = !expected_used || is_generic;

        let row = serde_json::json!({
            "src": src,
            "url": rec.url,
            "expected": expected,
            "main_plugin": main_plugin,
            "expected_used": expected_used,
            "active": active.iter().map(|(p, c)| json!({"plugin": p, "calls": c})).collect::<Vec<_>>(),
            "ratio": ratio,
            "original_size": output.metadata.original_size,
            "compressed_size": output.metadata.compressed_size,
            "slice_count": output.metadata.slice_count,
            "misrouted": misrouted_flag,
            "gist": rec.text.lines().take(1).collect::<Vec<_>>().join(" "),
        });
        block_rows.push(row.clone());

        if misrouted_flag {
            misrouted.push(row);
        }
        if main_plugin.is_none() {
            no_active += 1;
        }
    }

    // ---- 汇总打印 ----
    println!("\n==== 各插件触发画像（按主导块数降序）====");
    let mut ordered: Vec<(&String, &PluginProfile)> = profiles.iter().collect();
    ordered.sort_by(|a, b| {
        b.1.dominant_blocks
            .cmp(&a.1.dominant_blocks)
            .then(b.1.triggered_blocks.cmp(&a.1.triggered_blocks))
    });
    println!(
        "{:<20} {:>10} {:>10} {:>10}",
        "插件", "主导块", "触发块", "均值压缩率"
    );
    for (name, p) in &ordered {
        let avg_ratio = if p.triggered_blocks > 0 {
            p.total_ratio / p.triggered_blocks as f64
        } else {
            1.0
        };
        println!(
            "{:<20} {:>10} {:>10} {:>10.3}",
            name, p.dominant_blocks, p.triggered_blocks, avg_ratio
        );
    }

    println!("\n合计压缩块: {total}，其中无任何插件命中(透传): {no_active}");
    println!("落错块: {}（详见 JSON 报告的 misrouted）", misrouted.len());

    let report = serde_json::json!({
        "total_blocks": total,
        "no_active_blocks": no_active,
        "misrouted_count": misrouted.len(),
        "profiles": profiles.iter().map(|(k, v)| json!({
            "plugin": k,
            "dominant_blocks": v.dominant_blocks,
            "triggered_blocks": v.triggered_blocks,
        })).collect::<Vec<_>>(),
        "misrouted": misrouted,
        "block_rows": block_rows,
    });
    fs::write(
        REPORT_PATH,
        serde_json::to_string_pretty(&report).expect("序列化失败"),
    )
    .expect("写报告失败");
    println!("\nJSON 报告已写入 {REPORT_PATH}");
}
