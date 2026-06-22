//! 高性能并行压缩测试
//!
//! 使用内存映射 + 并行处理优化 30MB 日志文件

use std::fs;
use std::time::Instant;

fn main() {
    println!("=== 高性能并行压缩测试 ===\n");

    // 测试文件
    let test_files = vec![
        "tests/data/android_build_success.txt",
        "tests/data/gcc_build_failure-1.txt",
    ];

    for file_path in test_files {
        test_parallel_compression(file_path);
        println!();
    }
}

fn test_parallel_compression(file_path: &str) {
    println!("测试文件：{}", file_path);

    // 方法 1: 传统逐行处理（慢）
    let start = Instant::now();
    let result1 = traditional_processing(file_path);
    let duration1 = start.elapsed();

    // 方法 2: 内存映射 + 并行处理（快）
    let start = Instant::now();
    let result2 = parallel_processing(file_path);
    let duration2 = start.elapsed();

    println!(
        "  传统方法：{:.2?} ({:.2} MB/s)",
        duration1,
        result1.0 as f64 / duration1.as_secs_f64() / 1024.0 / 1024.0
    );
    println!(
        "  并行方法：{:.2?} ({:.2} MB/s)",
        duration2,
        result2.0 as f64 / duration2.as_secs_f64() / 1024.0 / 1024.0
    );
    println!(
        "  性能提升：{:.2}x",
        duration1.as_secs_f64() / duration2.as_secs_f64()
    );
    println!("  原始大小：{:.2} MB", result1.0 as f64 / 1024.0 / 1024.0);
    println!("  压缩大小：{:.2} MB", result1.1 as f64 / 1024.0 / 1024.0);
    println!(
        "  压缩率：{:.2}%",
        (result1.1 as f64 / result1.0 as f64) * 100.0
    );
}

/// 传统逐行处理方法（慢）
fn traditional_processing(file_path: &str) -> (usize, usize) {
    let content = fs::read_to_string(file_path).expect("读取文件失败");
    let original_size = content.len();

    let mut compressed = String::with_capacity(original_size);

    // 逐行处理（串行）
    for line in content.lines() {
        // 模拟时间戳替换
        let processed = replace_timestamps(line);
        compressed.push_str(&processed);
        compressed.push('\n');
    }

    (original_size, compressed.len())
}

/// 内存映射 + 并行处理方法（快）
fn parallel_processing(file_path: &str) -> (usize, usize) {
    // 方法 1: 使用 memmap2 内存映射
    let file = fs::File::open(file_path).expect("打开文件失败");
    let mmap = unsafe { memmap2::Mmap::map(&file).expect("内存映射失败") };
    let original_size = mmap.len();

    // 方法 2: 使用 rayon 并行处理
    use rayon::prelude::*;

    let content = std::str::from_utf8(&mmap).expect("UTF-8 转换失败");

    // 并行处理每一行
    let lines: Vec<&str> = content.lines().collect();
    let processed_lines: Vec<String> = lines
        .par_iter() // 并行迭代器
        .map(|line| replace_timestamps(line))
        .collect();

    let compressed = processed_lines.join("\n");

    (original_size, compressed.len())
}

/// 时间戳替换函数
fn replace_timestamps(line: &str) -> String {
    // 格式 1: [2026-03-05T03:21:46.868Z]
    if line.starts_with('[') && line.len() > 26 {
        if let Some(end) = line.find(']') {
            let ts = &line[1..end];
            if ts.len() == 24 && ts.contains('T') && ts.ends_with('Z') {
                return format!("[T+0ms]{}", &line[end + 1..]);
            }
        }
    }

    // 格式 2: 17:37:26
    if line.len() >= 8 && line.chars().nth(2) == Some(':') && line.chars().nth(5) == Some(':') {
        let time_part = &line[..8];
        if time_part.chars().all(|c| c.is_ascii_digit() || c == ':') {
            return format!("[T+0s]{}", &line[8..]);
        }
    }

    line.to_string()
}
