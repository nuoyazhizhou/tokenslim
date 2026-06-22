//! 集成测试：使用 DictionaryEngine 压缩 Jenkins 日志

use std::fs;
use std::path::Path;

fn main() {
    println!("=== Jenkins 日志集成测试 ===\n");

    // 读取日志
    let log_file = Path::new("tests/data/jenkins_build_failure.txt");
    let content = fs::read_to_string(log_file).expect("读取文件失败");

    println!("原始日志：{:.2} KB", content.len() as f64 / 1024.0);
    println!("原始行数：{}\n", content.lines().count());

    // 模拟 DictionaryEngine 的替换
    let mut dict_size = 0;
    let mut compressed = content.clone();

    // 替换常见路径
    let paths = [
        "/jenkins/workspace/cloud_admin_web_deploy",
        "/var/jenkins_home/workspace",
        "ssh://gerrit@gerrit.example.com:8013",
    ];

    for (i, path) in paths.iter().enumerate() {
        if compressed.contains(path) {
            let token = format!("$P{}", i + 1);
            compressed = compressed.replace(path, &token);
            println!("替换路径 #{}: {} -> {}", i + 1, path, token);
            dict_size += 1;
        }
    }

    // 替换常见包名
    let packages = [
        "hudson.remoting",
        "org.jenkinsci.plugins",
        "jenkins.plugins.git",
    ];

    for (i, pkg) in packages.iter().enumerate() {
        if compressed.contains(pkg) {
            let token = format!("$PK{}", i + 1);
            compressed = compressed.replace(pkg, &token);
            println!("替换包名 #{}: {} -> {}", i + 1, pkg, token);
            dict_size += 1;
        }
    }

    // 替换时间戳（正则表达式模拟）
    let timestamp_pattern = "[2025-10-21T01:51:";
    if compressed.contains(timestamp_pattern) {
        compressed = compressed.replace(timestamp_pattern, "[TIMESTAMP:");
        println!("替换时间戳前缀：{} -> [TIMESTAMP:", timestamp_pattern);
    }

    println!("\n压缩后：{:.2} KB", compressed.len() as f64 / 1024.0);
    println!(
        "压缩率：{:.2}%",
        (compressed.len() as f64 / content.len() as f64) * 100.0
    );
    println!("字典大小：{} 条目", dict_size);

    // 保存压缩结果
    fs::write("tests/output/compressed.txt", &compressed).expect("保存失败");
    println!("\n压缩文件已保存到：tests/output/compressed.txt");
}
