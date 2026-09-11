//! classifier holdout 盲测 runner（泛化门禁 / 基线画像）
//!
//! # 职责
//!
//! 对 `classifier_holdout/`（与 `samples/` 不同的独立盲测语料根）跑一次泛化基线：
//!
//! - **bayesian**：对 `bayesian/<cat>/case_*.log` 逐文件 `classify()`，输出
//!   30×30 混淆矩阵、逐类 recall、整体 top1 准确率、平均 margin，衡量分类器
//!   对**未知输出**的语义路由泛化能力（`sweep_*` 是自洽回归，这里是 out-of-distribution 门禁）。
//! - **structured**：对 `structured/<fmt>/*` 构造 `Slice` 依次调结构化插件 `detect()`，
//!   断言目标插件命中（0/1 布尔，无阈值脆弱性）；`gap_probes/**` 只记录实际命中，
//!   用于定位结构化覆盖缺口（TOML/INI 等已知缺项）。结构化识别不依赖贝叶斯分类器。
//!
//! # 分工语义
//!
//! `samples/` + `sweep_*` = 训练/自洽回归；`classifier_holdout/` = 泛化门禁。
//! 本 runner 是「先看数字、后定门槛」（Explore-then-assert）的第一道测量工具：
//! 作者据此记录基线，再把达标数字固化为 `holdout_gate` 测试断言。
//!
//! # 运行
//!
//!     cargo run --bin classifier_holdout
//!
//! 输出人类可读基线到 stdout，并写结构化报告到 `docs/audit/classifier_holdout_report.json`。

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::json;
use tokenslim::cli::get_plugins;
use tokenslim::core::content_classifier::features::seed_model;
use tokenslim::core::content_classifier::holdout::{
    bayesian_metrics, detect_with, load_bayesian_spec, load_structured_spec, run_bayesian_case,
    struct_detects, STRUCTURED_PLUGINS,
};
use tokenslim::core::plugin_dispatcher::Plugin;

/// 盲测除数：期望类别数与 `Category::ALL` 对齐（由指标模块校验）。
/// 报告输出路径遵循文档治理：`docs/audit/`。
const REPORT_PATH: &str = "docs/audit/classifier_holdout_report.json";

/// 读取物理样本文件（AGENTS 红线：禁止 inline mock，一律读盘）。
fn read_case(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("读取盲测样本 {} 失败: {e}", path.display()))
}

/// 把 `Vec<Box<dyn Plugin>>` 整理成「插件名 → &dyn Plugin」索引，供结构化 detect 按名寻址。
/// 返回索引借用 `plugins`（调用方须在其存活期内使用）。
fn index_plugins<'a>(plugins: &'a [Box<dyn Plugin>]) -> HashMap<String, &'a dyn Plugin> {
    plugins
        .iter()
        .map(|p| (p.name().to_string(), p.as_ref()))
        .collect()
}

/// 打印 bayesian 混合矩阵（行=期望类别，列=实际类别）。
fn print_confusion(rep: &tokenslim::core::content_classifier::holdout::BayesReport) {
    println!("\n==== 30×30 混淆矩阵（行=期望 / 列=实际，仅列非全零的类别）====");
    let n = rep.cat_order.len();
    // 只展示至少有一格的类别行/列，避免 30×30 稀疏矩阵刷屏。
    let active_cols: Vec<usize> = (0..n)
        .filter(|&c| (0..n).rev().map(|r| rep.confusion[r][c]).sum::<usize>() > 0)
        .collect();
    if active_cols.len() >= n {
        println!("（矩阵完全稠密，逐行打印全部 30 类）");
        for (r, name) in rep.cat_order.iter().enumerate() {
            let row: Vec<String> = rep.confusion[r].iter().map(|v| v.to_string()).collect();
            println!("{:<18} {}", name, row.join(" "));
        }
        return;
    }
    let header: Vec<String> = active_cols
        .iter()
        .map(|&c| rep.cat_order[c].to_string())
        .collect();
    println!("{:<18} {}", "类别", header.join("\t"));
    for (r, name) in rep.cat_order.iter().enumerate() {
        let row_sum: usize = rep.confusion[r].iter().sum();
        if row_sum == 0 {
            continue;
        }
        let cells: Vec<String> = active_cols
            .iter()
            .map(|&c| rep.confusion[r][c].to_string())
            .collect();
        println!("{:<18} {}", name, cells.join("\t"));
    }
}

fn main() {
    let root = tokenslim::core::content_classifier::holdout::holdout_root();

    // ---- 分类器：与生产链路同源（seed_model），确保结果反映真实泛化 ----
    let classifier = seed_model();

    // ---- bayesian 面 ----
    let bayes_spec = load_bayesian_spec(&root);
    println!("载入 bayesian 盲测样本 {} 个", bayes_spec.len());
    let mut bayes_results = Vec::new();
    let mut bayes_case_reports = Vec::new();
    for case in &bayes_spec {
        let text = read_case(&case.path);
        let r = run_bayesian_case(&classifier, case.expected.clone(), &text);
        bayes_case_reports.push(json!({
            "file": case.path.display().to_string(),
            "expected": r.expected,
            "actual": r.actual,
            "confidence": r.confidence,
            "margin": r.margin,
            "correct": r.expected == r.actual,
        }));
        bayes_results.push(r);
    }
    let rep = bayesian_metrics(&bayes_results);
    print_confusion(&rep);

    println!("\n==== bayesian 泛化基线 ====");
    println!("总样本: {}", rep.total);
    println!(
        "整体 top1 准确率: {:.3}（命中 {}）",
        rep.top1_acc, rep.correct
    );
    println!("平均 margin: {:.3}", rep.mean_margin);
    println!("{:<18} {:>10}", "类别", "recall");
    let mut recall_rows: Vec<(&String, &f64)> = rep.recall.iter().collect();
    recall_rows.sort_by(|a, b| a.0.cmp(b.0));
    for (name, rc) in &recall_rows {
        let val = if rc.is_nan() { f64::NAN } else { **rc };
        println!(
            "{:<18} {:>10.3} {}",
            name,
            val,
            if rc.is_nan() { "(无样本)" } else { "" }
        );
    }

    // ---- 结构化面 ----
    let plugins = get_plugins();
    let plugin_map = index_plugins(&plugins);
    let struct_spec = load_structured_spec(&root);
    println!(
        "\n载入 structured 盲测样本 {} 个（含 gap_probes）",
        struct_spec.len()
    );
    let mut struct_passes = 0usize;
    let mut struct_asserts = 0usize;
    let mut struct_rows = Vec::new();
    for case in &struct_spec {
        let text = read_case(&case.path);
        let hits = struct_detects(&plugin_map, &text);
        let hit = !hits.is_empty();
        let expected_hit = !case.probe
            && !case.expected_plugin.is_empty()
            && detect_with(&plugin_map, &case.expected_plugin, &text).is_some();
        if case.probe {
            // 探针：不参与强断言，仅记录命中分布。
        } else if expected_hit {
            struct_passes += 1;
            struct_asserts += 1;
        } else {
            struct_asserts += 1;
        }
        let hits_json: Vec<serde_json::Value> = hits
            .iter()
            .map(|(p, s)| json!({"plugin": p, "score": s}))
            .collect();
        struct_rows.push(json!({
            "file": case.path.display().to_string(),
            "probe": case.probe,
            "expected_plugin": case.expected_plugin,
            "detect_hit": hit,
            "expected_detect_hit": expected_hit,
            "hits": hits_json,
        }));
        let tag = if case.probe { "[probe]" } else { "" };
        println!(
            "{}{} {:<22} 期望={:<10} detect={:?}",
            tag,
            if expected_hit { "PASS" } else { "----" },
            case.path.display().to_string(),
            if case.expected_plugin.is_empty() {
                "(探针)"
            } else {
                &case.expected_plugin
            },
            hits,
        );
    }
    println!(
        "\n==== structured 泛化基线 ====\n强断言命中: {}/{}（探针不计入）",
        struct_passes, struct_asserts
    );

    // ---- 写报告（文档治理：报表进 docs/audit） ----
    let report = json!({
        "bayesian": {
            "cat_order": rep.cat_order,
            "confusion": rep.confusion,
            "recall": rep.recall,
            "total": rep.total,
            "correct": rep.correct,
            "top1_acc": rep.top1_acc,
            "mean_margin": rep.mean_margin,
            "cases": bayes_case_reports,
        },
        "structured": {
            "assert_passes": struct_passes,
            "assert_total": struct_asserts,
            "cases": struct_rows,
        },
    });
    let out_dir = Path::new(REPORT_PATH).parent().unwrap_or(Path::new("."));
    fs::create_dir_all(out_dir).expect("创建 docs/audit 目录失败");
    fs::write(
        REPORT_PATH,
        serde_json::to_string_pretty(&report).expect("序列化失败"),
    )
    .expect("写报告失败");
    println!("\nJSON 报告已写入 {REPORT_PATH}");
    // 提示：结构化插件名清单（供参考，其中部分可能在插件表中没有实例）。
    let _ = STRUCTURED_PLUGINS;
}
