use tokenslim::core::path_analyzer::{
    analyze_path_hierarchy, extract_all_paths, replace_paths_with_tokens,
};

#[test]
fn test_path_extraction() {
    // 注意：以下路径是脱敏的合成测试数据，原始名称已替换为 acme_corp / demo_audio_fw。
    let test_text = "-I/jenkins/workspace/build_root/project_sdk/vendor/amazon/vendor_component_a/cortexA5\n-I/jenkins/workspace/build_root/project_sdk/vendor/amazon/vendor_component_b/cortexA5\n-I/jenkins/workspace/build_root/project_sdk/acme_corp/build/include\n-I/jenkins/workspace/build_root/project_sdk/opensource/build/include\n-I/jenkins/workspace/build_root/project_sdk/opensource/build/include/json-c\n-I/jenkins/workspace/build_root/application/include\n-I/jenkins/workspace/build_root/out/demo_audio_fw/platform/include\n-I/jenkins/workspace/build_root/out/demo_audio_fw/install/dist/release/include/\n-I/jenkins/workspace/build_root/out/demo_audio_fw/platform/include\n-I/jenkins/workspace/build_root/application/include\n-I/jenkins/workspace/build_root/out/demo_audio_fw/install/dist/release/include/\n-I/jenkins/workspace/build_root/system/misc/script/project/\n-I/jenkins/workspace/build_root/project_sdk/opensource/build/include\n-I/jenkins/workspace/build_root/out/demo_audio_fw/install/dist/release/include\n-I/jenkins/workspace/build_root/platform/amlogic/a113/output/mesonaxg_s420_32_release/host/usr/include";

    // 提取所有路径
    let paths = extract_all_paths(test_text);
    println!("Extracted paths: {:?}", paths);

    // 分析路径层级
    let (dict, common_paths): (std::collections::HashMap<String, String>, Vec<String>) =
        analyze_path_hierarchy(&paths);
    println!("Path dictionary: {:?}", dict);
    println!("Common paths: {:?}", common_paths);

    // 替换路径为 token
    let replaced = replace_paths_with_tokens(test_text, &dict);
    println!("Replaced text: {}", replaced);

    assert!(!paths.is_empty());
    assert!(!dict.is_empty());
    assert!(!common_paths.is_empty());
    assert!(replaced.contains("$P"));
}
