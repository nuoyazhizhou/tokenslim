//! 简单性能测试 - 不使用外部依赖

use std::fs;
use std::time::Instant;

fn main() {
    println!("=== 简单性能测试 ===\n");

    let test_file = "tests/data/android_build_success.txt";

    // 测试 1: 逐行读取（慢）
    let start = Instant::now();
    let result1 = read_line_by_line(test_file);
    let duration1 = start.elapsed();

    // 测试 2: 一次性读取（快）
    let start = Instant::now();
    let result2 = read_all_at_once(test_file);
    let duration2 = start.elapsed();

    println!("文件：{}", test_file);
    println!(
        "  逐行读取：{:.2?} ({:.2} MB/s)",
        duration1,
        result1 as f64 / duration1.as_secs_f64() / 1024.0 / 1024.0
    );
    println!(
        "  一次性读：{:.2?} ({:.2} MB/s)",
        duration2,
        result2 as f64 / duration2.as_secs_f64() / 1024.0 / 1024.0
    );
    println!(
        "  性能提升：{:.2}x",
        duration1.as_secs_f64() / duration2.as_secs_f64()
    );
}

fn read_line_by_line(file_path: &str) -> usize {
    let content = fs::read_to_string(file_path).expect("读取失败");
    let mut count = 0;
    for line in content.lines() {
        count += line.len();
    }
    count
}

fn read_all_at_once(file_path: &str) -> usize {
    let content = fs::read(file_path).expect("读取失败");
    content.len()
}
