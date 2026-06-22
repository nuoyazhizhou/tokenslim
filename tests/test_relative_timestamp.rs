//! 相对时间戳压缩测试
//!
//! 将绝对时间戳 [2026-03-05T02:52:31.597Z] 替换为相对时间 [T+0ms]

use std::fs;

fn main() {
    println!("=== 相对时间戳压缩测试 ===\n");

    // 读取日志
    let log_file = "tests/data/android_build_success.txt";
    let content = fs::read_to_string(log_file).unwrap_or_else(|e| {
        eprintln!("读取文件失败：{}", e);
        std::process::exit(1);
    });

    println!("原始日志：{:.2} KB", content.len() as f64 / 1024.0);
    println!("原始行数：{}\n", content.lines().count());

    // 提取第一个时间戳作为基准
    let base_timestamp = extract_base_timestamp(&content);
    println!("基准时间：{}\n", base_timestamp);

    // 替换所有时间戳为相对时间
    let compressed = replace_with_relative_timestamps(&content, base_timestamp);

    println!("压缩后：{:.2} KB", compressed.len() as f64 / 1024.0);
    let compression_ratio = (compressed.len() as f64 / content.len() as f64) * 100.0;
    println!("压缩率：{:.2}%", compression_ratio);
    println!("节省：{:.2}%", 100.0 - compression_ratio);

    // 显示前 20 行对比
    println!("\n=== 前 20 行对比 ===");
    let original_lines: Vec<&str> = content.lines().take(20).collect();
    let compressed_lines: Vec<&str> = compressed.lines().take(20).collect();

    for (i, (orig, comp)) in original_lines
        .iter()
        .zip(compressed_lines.iter())
        .enumerate()
    {
        println!("{:3}: 原始：{}", i + 1, orig);
        println!("     压缩：{}\n", comp);
    }

    // 保存结果
    fs::write("tests/output/relative_timestamp.txt", &compressed).expect("保存失败");
    println!("压缩文件已保存到：tests/output/relative_timestamp.txt");
}

/// 提取第一个时间戳作为基准（返回毫秒数）
fn extract_base_timestamp(content: &str) -> String {
    // 查找第一个 [2026-03-05T02:52:31.597Z] 格式的时间戳
    for line in content.lines() {
        if let Some(start) = line.find('[') {
            if let Some(end) = line.find(']') {
                if end > start {
                    let timestamp = &line[start + 1..end];
                    // 验证是否是时间戳格式（包含 T 和 Z）
                    if timestamp.contains('T') && timestamp.contains('Z') && timestamp.len() == 24 {
                        return timestamp.to_string();
                    }
                }
            }
        }
    }
    String::new()
}

/// 解析 ISO 时间戳为毫秒数（从 Unix 纪元开始）
fn parse_timestamp_to_ms(timestamp: &str) -> i64 {
    // 格式：2026-03-05T02:52:31.597Z
    let parts: Vec<&str> = timestamp.split('T').collect();
    if parts.len() != 2 {
        return 0;
    }

    let date_part = parts[0];
    let time_part = parts[1].trim_end_matches('Z');

    // 解析日期
    let date_parts: Vec<&str> = date_part.split('-').collect();
    if date_parts.len() != 3 {
        return 0;
    }

    let year: i64 = date_parts[0].parse().unwrap_or(0);
    let month: i64 = date_parts[1].parse().unwrap_or(0);
    let day: i64 = date_parts[2].parse().unwrap_or(0);

    // 解析时间
    let time_parts: Vec<&str> = time_part.split(':').collect();
    if time_parts.len() != 3 {
        return 0;
    }

    let hour: i64 = time_parts[0].parse().unwrap_or(0);
    let minute: i64 = time_parts[1].parse().unwrap_or(0);
    let second_part: Vec<&str> = time_parts[2].split('.').collect();
    let second: i64 = second_part[0].parse().unwrap_or(0);
    let millis: i64 = if second_part.len() > 1 {
        second_part[1].parse().unwrap_or(0)
    } else {
        0
    };

    // 简化计算：只计算相对偏移（不需要精确的 Unix 时间戳）
    // 假设所有时间戳都在同一天
    let total_ms = ((hour * 3600 + minute * 60 + second) * 1000 + millis) as i64;

    // 加上年月日的简化表示（用于跨天情况）
    let day_offset = (year * 366 + month * 31 + day) * 24 * 3600 * 1000;

    total_ms + day_offset
}

/// 替换所有时间戳为相对时间（优化版本）
fn replace_with_relative_timestamps(content: &str, base_timestamp: String) -> String {
    let base_ms = parse_timestamp_to_ms(&base_timestamp);
    let mut result = String::with_capacity(content.len()); // 预分配内存

    for line in content.lines() {
        let mut last_end = 0;
        // 查找时间戳 [2026-03-05T02:52:31.597Z]
        for i in 0..line.len() {
            if line.as_bytes()[i] == b'[' && i + 25 < line.len() && line.as_bytes()[i + 25] == b']'
            {
                let timestamp = &line[i + 1..i + 25];

                // 快速验证：检查是否包含 'T' 和 'Z'
                if timestamp.len() == 24 && timestamp.contains('T') && timestamp.ends_with('Z') {
                    // 添加时间戳前的内容
                    result.push_str(&line[last_end..i]);

                    // 替换为相对时间
                    let current_ms = parse_timestamp_to_ms(timestamp);
                    let relative_ms = current_ms - base_ms;
                    result.push_str(&format!("[T+{}ms]", relative_ms));

                    last_end = i + 26; // 跳过 ']'
                }
            }
        }

        // 添加剩余内容
        result.push_str(&line[last_end..]);
        result.push('\n');

        // 每 1000 行显示进度
        if result.lines().count() % 1000 == 0 {
            eprintln!("处理中... {} 行", result.lines().count());
        }
    }

    result
}
