use tokenslim::core::dictionary_engine::DictionaryEngine;

#[test]
fn custom_type_snapshot_and_resolve() {
    let mut engine = DictionaryEngine::new();
    let token = engine.add_macro("service payment");
    assert!(token.starts_with("$M"));

    let dict = engine.snapshot();
    assert_eq!(dict.resolve(&token), Some("service payment".to_string()));
}

#[test]
fn basic_type_resolution() {
    let mut engine = DictionaryEngine::new();
    let token = engine.add_path_layered("/var/log/syslog");

    let dict = engine.snapshot();
    assert_eq!(dict.resolve_or_self(&token), "/var/log/syslog");
}
