use tokenslim::core::path_analyzer::{
    analyze_path_hierarchy, extract_all_paths, replace_paths_with_tokens,
};

fn main() {
    // 测试文本，包含多个路径
    let test_text = "-I/jenkins/workspace/build_root/project_sdk/vendor/amazon/wwe_amazon/pryon_lite/cortexA5\n-I/jenkins/workspace/build_root/project_sdk/vendor/amazon/mrm/cortexA5\n-I/jenkins/workspace/build_root/project_sdk/test/99/include\n-I/jenkins/workspace/build_root/project_sdk/opensource/99/include\n-I/jenkins/workspace/build_root/project_sdk/opensource/99/include/json-c\n-I/jenkins/workspace/build_root/application/include\n-I/jenkins/workspace/build_root/out/Sound_Lite_V2/platform/include\n-I/jenkins/workspace/build_root/out/Sound_Lite_V2/install/dist/release/include/\n-I/jenkins/workspace/build_root/out/Sound_Lite_V2/platform/include\n-I/jenkins/workspace/build_root/application/include\n-I/jenkins/workspace/build_root/out/Sound_Lite_V2/install/dist/release/include/\n-I/jenkins/workspace/build_root/system/misc/script/project/\n-I/jenkins/workspace/build_root/project_sdk/opensource/99/include\n-I/jenkins/workspace/build_root/out/Sound_Lite_V2/install/dist/release/include\n-I/jenkins/workspace/build_root/platform/amlogic/a113/output/mesonaxg_s420_32_release/host/usr/include";

    println!("原始文本:");
    println!("{}", test_text);
    println!("\n====================================");

    // 提取所有路径
    let paths = extract_all_paths(test_text);
    println!("提取到的路径 ({} 个):", paths.len());
    for path in &paths {
        println!("  - {}", path);
    }
    println!("\n====================================");

    // 分析路径层次结构
    let (dict, common_paths) = analyze_path_hierarchy(&paths);
    println!("生成的路径字典 ({} 个条目):", dict.len());
    for (path, token) in &dict {
        println!("  {} -> {}", path, token);
    }
    println!("\n====================================");

    println!("公共路径 (前 10 个):");
    for path in common_paths.iter().take(10) {
        println!("  - {}", path);
    }
    println!("\n====================================");

    // 替换路径为 token
    let replaced = replace_paths_with_tokens(test_text, &dict);
    println!("替换后的文本:");
    println!("{}", replaced);
    println!("\n====================================");

    // 计算压缩率
    let original_len = test_text.len();
    let replaced_len = replaced.len();
    let compression_ratio = (1.0 - (replaced_len as f64 / original_len as f64)) * 100.0;
    println!("压缩率: {:.2}%", compression_ratio);
    println!("原始长度: {} 字符", original_len);
    println!("压缩后长度: {} 字符", replaced_len);
}
