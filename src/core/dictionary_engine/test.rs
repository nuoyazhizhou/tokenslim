//! dictionary engine 测试模块

#[cfg(test)]
mod tests {
    use crate::core::dictionary_engine::DictionaryEngine;

    #[test]
    fn test_new() {
        let engine = DictionaryEngine::new();
        assert!(engine.paths.is_empty());
        assert!(engine.packages.is_empty());
        assert!(engine.macros.is_empty());
    }

    #[test]
    fn test_add_path() {
        let mut engine = DictionaryEngine::new();

        let token1 = engine.add_path_layered("/home/user/project");
        assert!(token1.starts_with("$P") || token1.starts_with("$D"));

        let token2 = engine.add_path_layered("/home/user/project");
        assert_eq!(token1, token2);
    }

    #[test]
    fn test_add_package() {
        let mut engine = DictionaryEngine::new();

        let token1 = engine.add_package("com.example.service");
        assert_eq!(token1, "$PK1");

        let token2 = engine.add_package("com.example.service");
        assert_eq!(token2, "$PK1");

        let token3 = engine.add_package("org.apache.commons");
        assert_eq!(token3, "$PK2");
    }

    #[test]
    fn test_add_macro() {
        let mut engine = DictionaryEngine::new();

        let token1 = engine.add_macro("-DDEBUG_VERBOSE_MODE");
        assert_eq!(token1, "$M1");

        let token2 = engine.add_macro("-DDEBUG_VERBOSE_MODE");
        assert_eq!(token2, "$M1");

        let token3 = engine.add_macro("-O2_COMPILER_FLAG");
        assert_eq!(token3, "$M2");
    }

    #[test]
    fn test_resolve() {
        let mut engine = DictionaryEngine::new();

        let t1 = engine.add_path_layered("/home/user/project");
        let t2 = engine.add_package("com.example.service");
        let t3 = engine.add_macro("-DDEBUG_VERBOSE_MODE");

        let dict = engine.snapshot();

        assert_eq!(
            dict.resolve_recursive(&t1),
            "/home/user/project".to_string()
        );
        assert_eq!(
            dict.packages.get(&t2).cloned(),
            Some("com.example.service".to_string())
        );
        assert_eq!(dict.resolve(&t3), Some("-DDEBUG_VERBOSE_MODE".to_string()));

        assert!(dict.resolve("$P999").is_none());
    }

    #[test]
    fn test_add_path_layered_and_resolve_or_self() {
        let mut engine = DictionaryEngine::new();
        let layered = engine.add_path_layered(
            "/jenkins/workspace/build_root/project_sdk/acme_corp/build/include",
        );
        let dict = engine.snapshot();

        let restored = dict.resolve_or_self(&layered);
        assert_eq!(
            restored,
            "/jenkins/workspace/build_root/project_sdk/acme_corp/build/include"
        );
    }

    #[test]
    fn test_snapshot() {
        let mut engine = DictionaryEngine::new();

        let t1 = engine.add_path_layered("/home/user/project");
        let t2 = engine.add_package("com.example.service");

        let snapshot = engine.snapshot();

        let t1_prefix = t1.split('/').next().unwrap_or(&t1).to_string();
        assert!(snapshot.paths.get(&t1_prefix).is_some());
        assert_eq!(
            snapshot.packages.get(&t2),
            Some(&"com.example.service".to_string())
        );

        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("$P"));
        assert!(json.contains("/home/user"));
    }

    #[test]
    fn test_add_path_layered_keeps_filename_readable() {
        let mut engine = DictionaryEngine::new();
        let layered = engine.add_path_layered("/very/long/prefix/path/file.rs");
        assert!(layered.starts_with("$P"));
        assert!(layered.ends_with("/file.rs"));

        let dict = engine.snapshot();
        assert_eq!(
            dict.resolve_recursive(&layered),
            "/very/long/prefix/path/file.rs".to_string()
        );
    }
}
