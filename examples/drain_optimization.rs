//! Drain 算法参数优化测试接口
//! 用于实时调整参数并观察模板生成效果

use std::fs;
use std::time::Instant;
use tokenslim::core::content_analyzer::drain::{DrainConfig, DrainManager};

fn main() {
    let test_file = "benchmarks/input_128kb.txt"; // 或者使用更大的日志文件
    if !std::path::Path::new(test_file).exists() {
        println!("Error: Test file {} not found.", test_file);
        return;
    }

    let content = fs::read_to_string(test_file).unwrap();
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

    println!("--- Drain Parameter Optimization Test ---");
    println!("File: {}, Total Lines: {}", test_file, lines.len());

    let test_cases = vec![
        (0.3, 3), // 激进合并
        (0.5, 4), // 默认
        (0.7, 6), // 保守分叉
    ];

    for (sim, depth) in test_cases {
        run_test(&lines, sim, depth);
    }
}

fn run_test(lines: &[&str], threshold: f32, depth: usize) {
    let config = DrainConfig {
        sim_threshold: threshold,
        max_depth: depth,
        ..DrainConfig::default()
    };

    let mut manager = DrainManager::new(config);

    let start = Instant::now();
    for line in lines {
        manager.add_log_message(line);
    }
    let duration = start.elapsed();

    let clusters = manager.get_templates();

    println!("\nTest Results (Sim: {}, Depth: {}):", threshold, depth);
    println!("  Time: {:?}", duration);
    println!("  Templates: {}", clusters.len());

    // 计算前 5 个模板的覆盖率
    let mut sorted = clusters.to_vec();
    sorted.sort_by(|a, b| b.size.cmp(&a.size));

    let top_size: usize = sorted.iter().take(5).map(|c| c.size).sum();
    let coverage = (top_size as f32 / lines.len() as f32) * 100.0;
    println!("  Top 5 Coverage: {:.2}%", coverage);

    if let Some(c) = sorted.first() {
        println!("  Example Top Template: {}", c.template.join(" "));
    }
}
