//! 并行文件读取性能测试

use memmap2::Mmap;
use rayon::prelude::*;
use std::fs::File;
use std::time::Instant;

fn main() {
    println!("=== 并行文件读取性能测试 ===\n");

    let test_file = "tests/data/gcc_build_success.txt";

    // 打开文件并创建 mmap
    let file = File::open(test_file).expect("Failed to open file");
    let mmap = unsafe { Mmap::map(&file).expect("Failed to mmap file") };

    let file_size = mmap.len();
    println!(
        "文件大小：{} 字节 ({:.2} MB)",
        file_size,
        file_size as f64 / 1024.0 / 1024.0
    );

    // 测试 1: 串行读取（模拟旧实现）
    println!("\n测试 1: 串行读取...");
    let start = Instant::now();
    serial_read(&mmap, 1024 * 1024); // 1MB chunks
    let serial_time = start.elapsed();
    println!("  串行时间：{:.3} 秒", serial_time.as_secs_f64());

    // 测试 2: 并行读取（新实现）
    println!("\n测试 2: 并行读取...");
    let start = Instant::now();
    parallel_read(&mmap, 1024 * 1024); // 1MB chunks
    let parallel_time = start.elapsed();
    println!("  并行时间：{:.3} 秒", parallel_time.as_secs_f64());

    // 计算加速比
    let speedup = serial_time.as_secs_f64() / parallel_time.as_secs_f64();
    println!("\n性能提升：{:.2}x", speedup);

    // 计算吞吐量
    let serial_throughput = file_size as f64 / serial_time.as_secs_f64() / 1024.0 / 1024.0;
    let parallel_throughput = file_size as f64 / parallel_time.as_secs_f64() / 1024.0 / 1024.0;
    println!("串行吞吐量：{:.2} MB/s", serial_throughput);
    println!("并行吞吐量：{:.2} MB/s", parallel_throughput);
}

/// 串行读取（模拟旧实现）
fn serial_read(mmap: &Mmap, chunk_size: usize) -> usize {
    let mut total_chars = 0;
    let file_size = mmap.len();
    let mut offset = 0;

    while offset < file_size {
        let end = std::cmp::min(offset + chunk_size, file_size);

        // 确保在 UTF-8 边界处分割
        let mut actual_end = end;
        if actual_end < file_size {
            while actual_end > offset && !is_char_boundary(mmap, actual_end) {
                actual_end -= 1;
            }
        }

        let chunk = &mmap[offset..actual_end];
        let chunk_str = String::from_utf8_lossy(chunk);
        total_chars += chunk_str.chars().count();

        offset = actual_end;
    }

    total_chars
}

/// 并行读取（新实现）
fn parallel_read(mmap: &Mmap, chunk_size: usize) -> usize {
    let file_size = mmap.len();
    let chunk_count = (file_size + chunk_size - 1) / chunk_size;

    let total_chars: usize = (0..chunk_count)
        .into_par_iter()
        .map(|i| {
            let start = i * chunk_size;
            let end = std::cmp::min(start + chunk_size, file_size);

            // 确保在 UTF-8 边界处分割
            let mut actual_end = end;
            if actual_end < file_size {
                while actual_end > start && !is_char_boundary(mmap, actual_end) {
                    actual_end -= 1;
                }
            }

            let chunk = &mmap[start..actual_end];
            let chunk_str = String::from_utf8_lossy(chunk);
            chunk_str.chars().count()
        })
        .sum();

    total_chars
}

/// 检查是否为 UTF-8 字符边界
fn is_char_boundary(data: &[u8], index: usize) -> bool {
    if index == 0 || index == data.len() {
        return true;
    }
    let b = data[index];
    (b as i8) >= -0x40
}
