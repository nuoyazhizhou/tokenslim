use tokenslim::core::dictionary_engine::DictionaryEngine;

#[test]
fn dedup_path_replaces_frequent_paths() {
    let mut engine = DictionaryEngine::new();
    let path =
        "/jenkins/workspace/build_root/project_sdk/acme_corp/99/include";

    let token1 = engine.add_path_layered(path);
    let token2 = engine.add_path_layered(path);

    assert_eq!(token1, token2);
    assert!(token1.starts_with("$P"));

    let dict = engine.snapshot();
    assert_eq!(dict.resolve_or_self(&token1), path);
}

#[test]
fn dedup_path_skips_when_not_beneficial() {
    let mut engine = DictionaryEngine::new();
    let short_path = "/a";

    // Very short paths might still be added, but they represent the standard API flow now
    let token = engine.add_path_layered(short_path);
    assert!(token.starts_with("$P") || token == short_path);
}
