//! 批量压缩测试脚本
//!
//! 功能：
//! 1. 对 tests/data/ 目录下所有文件进行压缩测试
//! 2. 记录压缩前后大小、压缩率、处理时间
//! 3. 生成详细的统计报告

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

fn main() {
    println!("=== TokenSlim 批量压缩测试 ===\n");

    let data_dir = "tests/data";
    let output_dir = "tests/output";

    // 创建输出目录
    if !Path::new(output_dir).exists() {
        fs::create_dir_all(output_dir).expect("Failed to create output directory");
    }

    // 获取所有测试文件
    let mut files: Vec<_> = fs::read_dir(data_dir)
        .expect("Failed to read data directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();

    // 按文件名排序
    files.sort();

    println!("找到 {} 个测试文件\n", files.len());

    let mut total_original_size = 0u64;
    let mut total_compressed_size = 0u64;
    let mut total_original_tokens = 0u64;
    let mut total_compressed_tokens = 0u64;
    let mut total_time = 0.0;
    let mut results = Vec::new();

    for file_path in &files {
        println!("正在处理：{}", file_path.display());

        let file_size = fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);

        let start = Instant::now();

        // 使用 CLI 进行压缩
        let output_file = Path::new(output_dir)
            .join(file_path.file_name().unwrap())
            .with_extension("json");

        let output = Command::new("cargo")
            .args(&[
                "run",
                "--release",
                "--",
                "--compress",
                "--input",
                file_path.to_str().unwrap(),
                "--output",
                output_file.to_str().unwrap(),
            ])
            .output();

        let elapsed = start.elapsed();
        let elapsed_secs = elapsed.as_secs_f64();

        match output {
            Ok(out) => {
                if !out.status.success() {
                    println!("  ❌ 压缩失败：{}", String::from_utf8_lossy(&out.stderr));
                    results.push(TestResult {
                        file: file_path
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                        original_size: file_size,
                        compressed_size: 0,
                        compression_ratio: 0.0,
                        token_savings: 0,
                        time: elapsed_secs,
                        success: false,
                    });
                    continue;
                }

                // 读取压缩后的文件大小
                let compressed_size = fs::metadata(&output_file).map(|m| m.len()).unwrap_or(0);

                // 解析 JSON 获取 token 统计
                let json_content = fs::read_to_string(&output_file).unwrap_or_default();

                let (original_tokens, compressed_tokens) = parse_token_stats(&json_content);
                let token_savings = original_tokens.saturating_sub(compressed_tokens);

                let ratio = if file_size > 0 {
                    compressed_size as f64 / file_size as f64
                } else {
                    0.0
                };

                println!(
                    "  ✓ 压缩完成：{} -> {} 字节 ({:.2}%), {:.3} 秒",
                    file_size,
                    compressed_size,
                    ratio * 100.0,
                    elapsed_secs
                );
                println!(
                    "    Token: {} -> {} (节省：{})",
                    original_tokens, compressed_tokens, token_savings
                );

                total_original_size += file_size;
                total_compressed_size += compressed_size;
                total_original_tokens += original_tokens;
                total_compressed_tokens += compressed_tokens;
                total_time += elapsed_secs;

                results.push(TestResult {
                    file: file_path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    original_size: file_size,
                    compressed_size,
                    compression_ratio: ratio,
                    token_savings,
                    time: elapsed_secs,
                    success: true,
                });
            }
            Err(e) => {
                println!("  ❌ 执行失败：{}", e);
                results.push(TestResult {
                    file: file_path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    original_size: file_size,
                    compressed_size: 0,
                    compression_ratio: 0.0,
                    token_savings: 0,
                    time: elapsed_secs,
                    success: false,
                });
            }
        }
    }

    // 生成报告
    println!("\n{}", "=".repeat(80));
    println!("批量测试报告");
    println!("{}", "=".repeat(80));

    println!("\n总体统计:");
    println!("  文件总数：{}", files.len());
    println!("  成功：{}", results.iter().filter(|r| r.success).count());
    println!("  失败：{}", results.iter().filter(|r| !r.success).count());

    if total_original_size > 0 {
        let overall_ratio = total_compressed_size as f64 / total_original_size as f64;
        println!("\n大小统计:");
        println!(
            "  原始总大小：{} 字节 ({:.2} MB)",
            total_original_size,
            total_original_size as f64 / 1024.0 / 1024.0
        );
        println!(
            "  压缩后总大小：{} 字节 ({:.2} MB)",
            total_compressed_size,
            total_compressed_size as f64 / 1024.0 / 1024.0
        );
        println!("  平均压缩率：{:.2}%", overall_ratio * 100.0);
        println!(
            "  节省空间：{} 字节 ({:.2} MB)",
            total_original_size - total_compressed_size,
            (total_original_size - total_compressed_size) as f64 / 1024.0 / 1024.0
        );
    }

    println!("\nToken 统计:");
    println!("  原始 Token: {}", total_original_tokens);
    println!("  压缩后 Token: {}", total_compressed_tokens);
    if total_original_tokens > 0 {
        let token_ratio = total_compressed_tokens as f64 / total_original_tokens as f64;
        println!("  Token 压缩率：{:.2}%", token_ratio * 100.0);
        println!(
            "  节省 Token: {}",
            total_original_tokens - total_compressed_tokens
        );
    }

    println!("\n性能统计:");
    println!("  总处理时间：{:.3} 秒", total_time);
    if total_original_size > 0 {
        let throughput = total_original_size as f64 / total_time / 1024.0 / 1024.0;
        println!("  平均吞吐量：{:.2} MB/s", throughput);
    }

    println!("\n详细结果:");
    println!(
        "{:<40} {:>10} {:>10} {:>8} {:>10} {:>8}",
        "文件名", "原始大小", "压缩后", "压缩率", "Token 节省", "时间"
    );
    println!("{}", "-".repeat(90));

    for result in &results {
        let status = if result.success { "✓" } else { "✗" };
        println!(
            "{:<40} {:>10} {:>10} {:>7.2}% {:>10} {:>7.3}s {}",
            result.file,
            result.original_size,
            result.compressed_size,
            result.compression_ratio * 100.0,
            result.token_savings,
            result.time,
            status
        );
    }

    // 保存报告到文件
    let report_path = Path::new(output_dir).join("test_report.txt");
    let report_content = generate_report_text(
        &results,
        total_original_size,
        total_compressed_size,
        total_original_tokens,
        total_compressed_tokens,
        total_time,
    );
    fs::write(&report_path, report_content).expect("Failed to write report");
    println!("\n报告已保存到：{}", report_path.display());
}

fn parse_token_stats(json_content: &str) -> (u64, u64) {
    // 简单解析 JSON 提取 token 统计
    let mut original_tokens = 0u64;
    let mut compressed_tokens = 0u64;

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_content) {
        if let Some(meta) = value.get("metadata") {
            original_tokens = meta
                .get("original_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            compressed_tokens = meta
                .get("compressed_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
        }
    }

    (original_tokens, compressed_tokens)
}

fn generate_report_text(
    results: &[TestResult],
    total_orig_size: u64,
    total_comp_size: u64,
    total_orig_tokens: u64,
    total_comp_tokens: u64,
    total_time: f64,
) -> String {
    let mut report = String::new();

    report.push_str("=== TokenSlim 批量压缩测试报告 ===\n\n");

    report.push_str(&format!("测试文件总数：{}\n", results.len()));
    report.push_str(&format!(
        "成功：{}\n",
        results.iter().filter(|r| r.success).count()
    ));
    report.push_str(&format!(
        "失败：{}\n\n",
        results.iter().filter(|r| !r.success).count()
    ));

    if total_orig_size > 0 {
        let overall_ratio = total_comp_size as f64 / total_orig_size as f64;
        report.push_str("=== 大小统计 ===\n");
        report.push_str(&format!(
            "原始总大小：{} 字节 ({:.2} MB)\n",
            total_orig_size,
            total_orig_size as f64 / 1024.0 / 1024.0
        ));
        report.push_str(&format!(
            "压缩后总大小：{} 字节 ({:.2} MB)\n",
            total_comp_size,
            total_comp_size as f64 / 1024.0 / 1024.0
        ));
        report.push_str(&format!("平均压缩率：{:.2}%\n", overall_ratio * 100.0));
        report.push_str(&format!(
            "节省空间：{} 字节 ({:.2} MB)\n\n",
            total_orig_size - total_comp_size,
            (total_orig_size - total_comp_size) as f64 / 1024.0 / 1024.0
        ));
    }

    report.push_str("=== Token 统计 ===\n");
    report.push_str(&format!("原始 Token: {}\n", total_orig_tokens));
    report.push_str(&format!("压缩后 Token: {}\n", total_comp_tokens));
    if total_orig_tokens > 0 {
        let token_ratio = total_comp_tokens as f64 / total_orig_tokens as f64;
        report.push_str(&format!("Token 压缩率：{:.2}%\n", token_ratio * 100.0));
        report.push_str(&format!(
            "节省 Token: {}\n\n",
            total_orig_tokens - total_comp_tokens
        ));
    }

    report.push_str("=== 性能统计 ===\n");
    report.push_str(&format!("总处理时间：{:.3} 秒\n", total_time));
    if total_orig_size > 0 {
        let throughput = total_orig_size as f64 / total_time / 1024.0 / 1024.0;
        report.push_str(&format!("平均吞吐量：{:.2} MB/s\n\n", throughput));
    }

    report.push_str("=== 详细结果 ===\n");
    for result in results {
        let status = if result.success { "✓" } else { "✗" };
        report.push_str(&format!(
            "[{}] {}: {} -> {} 字节 ({:.2}%), Token 节省：{}, 时间：{:.3}s\n",
            status,
            result.file,
            result.original_size,
            result.compressed_size,
            result.compression_ratio * 100.0,
            result.token_savings,
            result.time
        ));
    }

    report
}

#[derive(Debug, Clone)]
struct TestResult {
    file: String,
    original_size: u64,
    compressed_size: u64,
    compression_ratio: f64,
    token_savings: u64,
    time: f64,
    success: bool,
}
