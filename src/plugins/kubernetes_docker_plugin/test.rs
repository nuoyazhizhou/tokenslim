//! kubernetes docker plugin 测试模块

#[cfg(test)]
mod tests {
    use super::super::types::KubernetesDockerPlugin;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::{Slice, SliceType};
    use bumpalo::Bump;
    use std::borrow::Cow;
    use std::path::{Path, PathBuf};

    fn sample_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("kubernetes_docker_plugin")
    }

    fn read_sample(file_name: &str) -> String {
        std::fs::read_to_string(sample_dir().join(file_name))
            .expect("read kubernetes/docker sample")
    }

    fn compress_sample(plugin: &KubernetesDockerPlugin, raw: &str) -> String {
        let slice = Slice {
            id: 10,
            text: Cow::Borrowed(raw),
            slice_type: SliceType::LogBlock,
            offset: 0,
            line_start: 1,
            line_end: raw.lines().count().max(1),
            file_metadata: None,
            flags: Default::default(),
        };
        let mut dict_engine = DictionaryEngine::new();
        let arena = Bump::new();
        let mut dedup_engine = DedupEngine::new(DedupConfig::default());
        let result = plugin.compress(&slice, &mut dict_engine, &mut dedup_engine, &arena);
        result
            .tokens
            .iter()
            .filter_map(|token| match token {
                crate::core::compression::Token::Text(text) => Some(text.as_ref()),
                _ => None,
            })
            .collect::<String>()
    }

    #[test]
    /// 内部辅助函数：执行与 test kubernetes detection 相关的具体逻辑。
    fn test_kubernetes_detection() {
        let plugin = KubernetesDockerPlugin::new();

        let log = "prod/auth-service-6f5d4b8c9d-abcde: User logged in";
        let slice = Slice {
            id: 1,
            text: Cow::Borrowed(log),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,
            flags: Default::default(),
        };

        let score = plugin.detect(&slice);
        assert!(score.is_some());
    }

    #[test]
    /// 内部辅助函数：执行与 test kubernetes compression pod 相关的具体逻辑。
    fn test_kubernetes_compression_pod() {
        let plugin = KubernetesDockerPlugin::new();
        let mut dict_engine = DictionaryEngine::new();
        let arena = Bump::new();
        let mut dedup_engine = DedupEngine::new(DedupConfig::default());

        let log = "production/auth-service-6f5d4b8c9d-abcde: login attempt";
        let slice = Slice {
            id: 1,
            text: Cow::Borrowed(log),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,
            flags: Default::default(),
        };

        let result = plugin.compress(&slice, &mut dict_engine, &mut dedup_engine, &arena);
        let compressed = match &result.tokens[0] {
            crate::core::compression::Token::Text(s) => s,
            _ => panic!("Expected text token"),
        };

        // production -> $PK1, auth-service-... -> $P1
        assert!(compressed.contains("$PK1/$P1"));
    }

    #[test]
    /// 内部辅助函数：执行与 test cloudwatch unwrap 相关的具体逻辑。
    fn test_cloudwatch_unwrap() {
        let plugin = KubernetesDockerPlugin::new();
        let mut dict_engine = DictionaryEngine::new();
        let arena = Bump::new();
        let mut dedup_engine = DedupEngine::new(DedupConfig::default());

        let json_log = r#"{"message": "Database connection failed", "logStream": "stream-123"}"#;
        let slice = Slice {
            id: 1,
            text: Cow::Borrowed(json_log),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,
            flags: Default::default(),
        };

        let result = plugin.compress(&slice, &mut dict_engine, &mut dedup_engine, &arena);
        let compressed = match &result.tokens[0] {
            crate::core::compression::Token::Text(s) => s,
            _ => panic!("Expected text token"),
        };

        assert_eq!(compressed, "Database connection failed");
    }

    #[test]
    fn test_cloudwatch_unwrap_with_noisy_prefix() {
        let plugin = KubernetesDockerPlugin::new();
        let mut dict_engine = DictionaryEngine::new();
        let arena = Bump::new();
        let mut dedup_engine = DedupEngine::new(DedupConfig::default());

        let noisy =
            r#"pnpm notice env=prod {"message":"Timeout on upstream","logStream":"s-1"} tail"#;
        let slice = Slice {
            id: 2,
            text: Cow::Borrowed(noisy),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,
            flags: Default::default(),
        };

        let result = plugin.compress(&slice, &mut dict_engine, &mut dedup_engine, &arena);
        let compressed = match &result.tokens[0] {
            crate::core::compression::Token::Text(s) => s,
            _ => panic!("Expected text token"),
        };

        assert_eq!(compressed, "Timeout on upstream");
    }

    #[test]
    fn detects_ci_docker_and_kubectl_outputs() {
        let plugin = KubernetesDockerPlugin::new();
        for file_name in [
            "case_016_docker_buildkit_failure.log",
            "case_017_docker_buildx_multi_platform.log",
            "case_018_docker_compose_up_build.log",
            "case_020_kubectl_rollout_success.log",
            "case_021_kubectl_rollout_timeout.log",
            "case_022_kubectl_apply_summary.log",
            "case_023_kubectl_diff.log",
        ] {
            let raw = read_sample(file_name);
            let slice = Slice {
                id: 11,
                text: Cow::Borrowed(&raw),
                slice_type: SliceType::LogBlock,
                offset: 0,
                line_start: 1,
                line_end: raw.lines().count().max(1),
                file_metadata: None,
                flags: Default::default(),
            };
            assert!(
                plugin.detect(&slice).is_some(),
                "{file_name} should be detected"
            );
        }
    }

    #[test]
    fn preserves_ci_error_signals_for_new_cases() {
        let plugin = KubernetesDockerPlugin::new();
        for file_name in [
            "case_016_docker_buildkit_failure.log",
            "case_019_docker_compose_failure.log",
            "case_021_kubectl_rollout_timeout.log",
            "case_024_kubectl_logs_previous_crashloop.log",
        ] {
            let raw = read_sample(file_name);
            let compacted = compress_sample(&plugin, &raw);
            let lower = compacted.to_ascii_lowercase();
            assert!(
                !compacted.is_empty(),
                "{file_name} compact output should be non-empty"
            );
            assert!(
                lower.contains("error") || lower.contains("failed") || lower.contains("panic"),
                "{file_name} should keep failure signal"
            );
        }
    }
}
