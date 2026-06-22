use std::fs::File;
use std::io::Read;
use std::time::Instant;
use tokenslim::core::path_analyzer::{
    analyze_path_hierarchy, extract_all_paths, replace_paths_with_tokens,
};

fn main() {
    // 测试文件列表
    let test_files = [
        "tests/data/gcc_build_failure-1.txt",
        "tests/data/gcc_build_failure-2.txt",
        "tests/data/gcc_build_failure-3.txt",
        "tests/data/gcc_build_failure-4.txt",
        "tests/data/gcc_build_success.txt.txt",
        "tests/data/gcc_build_utf8.txt",
        "tests/data/gcc_coverity_success-1.txt",
        "tests/data/gcc_coverity_success-2.txt",
    ];

    println!("路径分析方法基准测试");
    println!("====================================");

    for file_path in &test_files {
        println!("测试文件: {}", file_path);

        // 读取文件内容
        let mut file = File::open(file_path).expect("无法打开文件");
        let mut content = String::new();
        match file.read_to_string(&mut content) {
            Ok(_) => (),
            Err(e) => {
                println!("警告: 无法读取文件 ({}), 跳过该文件", e);
                continue;
            }
        }

        let original_len = content.len();
        println!("原始长度: {} 字符", original_len);

        // 方法1: 提取所有路径
        let start = Instant::now();
        let paths = extract_all_paths(&content);
        let extract_time = start.elapsed();
        println!(
            "提取路径数量: {}, 提取时间: {:?}",
            paths.len(),
            extract_time
        );

        // 方法2: 分析路径层次结构
        let start = Instant::now();
        let (dict, common_paths) = analyze_path_hierarchy(&paths);
        let analyze_time = start.elapsed();
        println!(
            "生成字典条目: {}, 公共路径数量: {}, 分析时间: {:?}",
            dict.len(),
            common_paths.len(),
            analyze_time
        );

        // 方法3: 替换路径为 token
        let start = Instant::now();
        let replaced = replace_paths_with_tokens(&content, &dict);
        let replace_time = start.elapsed();
        let replaced_len = replaced.len();
        let compression_ratio = (1.0 - (replaced_len as f64 / original_len as f64)) * 100.0;
        println!(
            "压缩后长度: {} 字符, 压缩率: {:.2}%, 替换时间: {:?}",
            replaced_len, compression_ratio, replace_time
        );

        // 总执行时间
        let total_time = extract_time + analyze_time + replace_time;
        println!("总执行时间: {:?}", total_time);

        println!("====================================");
    }

    println!("测试完成!");
}
