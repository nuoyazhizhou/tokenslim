//! dictionary engine 测试模块

#[cfg(test)]
mod tests {
    use crate::core::dictionary_engine::is_semantic_macro;
    use crate::core::dictionary_engine::DictionaryEngine;

    /// P2-11：宏「语义 / 噪声」判定谓词联网——四个语义关键字每个都必须判为语义，
    /// 纯噪声样本必须判为噪声；登记侧与解析侧共用该谓词，此契约为统一口径的锚点。
    #[test]
    fn test_is_semantic_macro_caliber() {
        for seed in ["error", "fail", "exception", "warning"] {
            assert!(is_semantic_macro(seed), "关键字 {seed} 应判为语义宏");
            assert!(
                is_semantic_macro(&format!("carrot {seed} carrot")),
                "含 {seed} 应判语义"
            );
            assert!(
                is_semantic_macro(&seed.to_uppercase()),
                "大写 {seed} 应判语义"
            );
        }
        for noise in [
            "progress",
            "building 3 of 12",
            "download started",
            "retry 10ms",
        ] {
            assert!(!is_semantic_macro(noise), "噪声样本 {noise:?} 不应判为语义");
        }
    }

    /// 验证新建引擎后仅持有 manager 委托：本地词表字段已按 P3-01/P3-86 删除，
    /// 所有登记/查询状态一律由 `DictionaryManager` 承载。
    #[test]
    fn test_new() {
        let engine = DictionaryEngine::new();
        assert!(engine.manager.is_some());
    }

    /// 验证 add_path_layered 能为同一路径生成稳定的 `$P`/`$D` token，且重复添加返回相同 token（幂等）。
    #[test]
    fn test_add_path() {
        let mut engine = DictionaryEngine::new();

        let token1 = engine.add_path_layered("/home/user/project");
        assert!(token1.starts_with("$P") || token1.starts_with("$D"));

        let token2 = engine.add_path_layered("/home/user/project");
        assert_eq!(token1, token2);
    }

    /// 验证 add_package 按包名分配递增的 `$PK` token：相同包名复用同一 token，不同包名分配新序号。
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

    /// 验证 add_macro 按宏定义分配递增的 `$M` token：相同宏复用，不同宏分配新序号。
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

    /// 验证 snapshot 后的字典能递归解析路径/包/宏 token 为原始串，且不存在的 token（`$P999`）解析为 None。
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

    /// 验证长路径经 add_path_layered 令牌化后，resolve_or_self 能无损还原为完整原始路径（保留文件名可读）。
    #[test]
    fn test_add_path_layered_and_resolve_or_self() {
        let mut engine = DictionaryEngine::new();
        let layered = engine
            .add_path_layered("/jenkins/workspace/build_root/project_sdk/acme_corp/build/include");
        let dict = engine.snapshot();

        let restored = dict.resolve_or_self(&layered);
        assert_eq!(
            restored,
            "/jenkins/workspace/build_root/project_sdk/acme_corp/build/include"
        );
    }

    /// 验证 snapshot 包含已添加的路径前缀与包映射，且序列化 JSON 含 `$P` 标记与原始路径片段。
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

    /// 验证 add_path_layered 对文件型路径仅令牌化目录前缀、保留末尾文件名可读，且 resolve_recursive 能完整还原。
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

    /// Q442 回归：`$PK` 包 token 不得被 `$P` 路径分支截获。
    /// 修复前：resolve_one_level("$PK1") 走 $P 分支查 paths → None；resolve_for_ai 中 $PK 亦被 $P 分支吞掉。
    #[test]
    fn test_pkg_token_not_shadowed_by_path_prefix() {
        let mut engine = DictionaryEngine::new();
        let pkg = engine.add_package("com.example.service");
        assert_eq!(pkg, "$PK1");
        let path_tok = engine.add_path_layered("/home/user/project");
        let dict = engine.snapshot();

        // resolve_one_level：$PK1 必须命中 packages 而非 paths
        assert_eq!(
            dict.resolve_one_level("$PK1"),
            Some("com.example.service".to_string())
        );
        // resolve：单级 + 递归均正确
        assert_eq!(
            dict.resolve("$PK1"),
            Some("com.example.service".to_string())
        );
        // 路径 token 不受顺序调整影响，仍可解析（分层 token 如 $P1/... 需走 resolve_or_self 逐 token 扫描）
        assert_eq!(
            dict.resolve_or_self(&path_tok),
            "/home/user/project".to_string()
        );
    }

    /// Q442 回归：resolve_for_ai 必须展开 $PK/$C/$FL，且 $P 路径分支仍正常。
    #[test]
    fn test_resolve_for_ai_expands_pkg_token() {
        let mut engine = DictionaryEngine::new();
        engine.add_package("com.example.service");
        let dict = engine.snapshot();

        // 修复前：$PK1 被 $P 分支截获 → paths 查不到 → 原样保留，无法展开
        let ai = dict.resolve_for_ai("import $PK1;");
        assert!(
            ai.contains("com.example.service"),
            "resolve_for_ai 应展开 $PK1，实际输出: {ai}"
        );
        assert!(!ai.contains("$PK1"), "$PK1 不应残留: {ai}");
    }

    /// Q442 回归：skeletonize_path 不得把 $PK 当作路径骨架化。
    #[test]
    fn test_skeletonize_excludes_pkg_token() {
        let mut engine = DictionaryEngine::new();
        engine.add_package("com.example.service");
        assert_eq!(engine.skeletonize_path("$PK1"), "$PK1");
    }
}
