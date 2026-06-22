//! 压缩效果测试
//!
//! 使用 15 个 Jenkins 日志文件测试压缩效果

use std::fs;
use std::path::Path;

fn main() {
    let test_dir = Path::new("tests/data");

    if !test_dir.exists() {
        eprintln!("测试目录不存在：{:?}", test_dir);
        return;
    }

    println!("=== TokenSlim 压缩效果测试 ===\n");

    let mut total_original = 0u64;
    let _total_compressed = 0u64;
    let mut file_count = 0;

    // 遍历所有日志文件
    for entry in fs::read_dir(test_dir).expect("读取测试目录失败") {
        let entry = entry.expect("读取目录项失败");
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("log") {
            continue;
        }

        let content = fs::read_to_string(&path).expect("读取文件失败");
        let original_size = content.len() as u64;
        total_original += original_size;
        file_count += 1;

        println!("文件：{:?}", path.file_name().unwrap());
        println!("  原始大小：{:.2} KB", original_size as f64 / 1024.0);

        // TODO: 这里调用压缩逻辑
        // let compressed = compress(&content);
        // let compressed_size = compressed.len() as u64;
        // total_compressed += compressed_size;
        // println!("  压缩后大小：{:.2} KB", compressed_size as f64 / 1024.0);
        // println!("  压缩率：{:.2}%", (compressed_size as f64 / original_size as f64) * 100.0);
        println!("  状态：待实现\n");
    }

    println!("=== 汇总 ===");
    println!("文件总数：{}", file_count);
    println!("原始总大小：{:.2} KB", total_original as f64 / 1024.0);
    println!("压缩后总大小：待实现");
    println!("平均压缩率：待实现");
}
