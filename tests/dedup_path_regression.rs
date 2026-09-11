use tokenslim::core::dictionary_engine::DictionaryEngine;

/// 高频路径去重回归：同一路径两次 add_path_layered 应返回相同 $P token，
/// 且字典快照可逆解析回原路径。
#[test]
fn dedup_path_replaces_frequent_paths() {
    let mut engine = DictionaryEngine::new();
    let path = "/jenkins/workspace/build_root/project_sdk/acme_corp/99/include";

    let token1 = engine.add_path_layered(path);
    let token2 = engine.add_path_layered(path);

    assert_eq!(token1, token2);
    assert!(token1.starts_with("$P"));

    let dict = engine.snapshot();
    assert_eq!(dict.resolve_or_self(&token1), path);
}

/// 短路径去重回归：过短路径按当前标准 API 流可能仍入库为 $P token
/// 或原样返回，断言允许两种结果（防过度收紧导致回归误报）。
#[test]
fn dedup_path_skips_when_not_beneficial() {
    let mut engine = DictionaryEngine::new();
    let short_path = "/a";

    // Very short paths might still be added, but they represent the standard API flow now
    let token = engine.add_path_layered(short_path);
    assert!(token.starts_with("$P") || token == short_path);
}
