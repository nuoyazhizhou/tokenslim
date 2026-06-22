//! rehydration pipeline 测试模块
//!
//! # 测试概述
//!
//! 本模块包含 rehydration pipeline 模块的单元测试和集成测试。
//! 测试覆盖了主要功能和边界情况。

#[cfg(test)]
mod tests {
    use crate::core::compression::{CompressionMetadata, CompressionOutput, Token};
    use crate::core::dedup_engine::DedupEngine;
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::plugin_dispatcher::CompressResult;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::rehydration_pipeline::{RehydrationConfig, RehydrationPipeline};
    use crate::core::text_slicer::Slice;

    struct DummyPlugin;
    impl Plugin for DummyPlugin {
        fn name(&self) -> &'static str {
            "dummy"
        }
        fn priority(&self) -> u8 {
            100
        }
        fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
            None
        }
        fn compress<'a>(
            &self,
            _slice: &'a Slice<'a>,
            _dict: &mut DictionaryEngine,
            _dedup: &mut DedupEngine,
            _arena: &'a bumpalo::Bump,
        ) -> CompressResult<'a> {
            CompressResult {
                tokens: vec![Token::Text("dummy".into())],
                metadata: None,
                plugin_name: Some("dummy"),
            }
        }
        fn decompress(
            &self,
            compressed: &str,
            _dict: &crate::core::dictionary_engine::Dictionary,
        ) -> String {
            compressed.replace("$DUMMY|", "Hello ")
        }
    }

    #[test]
    fn test_new() {
        let dict_engine = DictionaryEngine::new();
        let dict = dict_engine.snapshot();
        let config = RehydrationConfig {
            preserve_order: false,
            fallback_on_error: false,
        };
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(DummyPlugin)];

        let pipeline = RehydrationPipeline::new(dict, plugins, config);
        assert!(pipeline.plugins.contains_key("dummy"));
    }

    #[test]
    fn test_rehydrate_tokens_text_and_dict() {
        let mut dict_engine = DictionaryEngine::new();
        let path_token = dict_engine.add_path_layered("/var/log/syslog");
        let dict = dict_engine.snapshot();

        let config = RehydrationConfig {
            preserve_order: false,
            fallback_on_error: false,
        };
        let pipeline = RehydrationPipeline::new(dict, vec![], config);

        let tokens = vec![
            Token::Text(std::borrow::Cow::Borrowed("Error found in file: ")),
            Token::DictRef(std::borrow::Cow::Owned(path_token)),
            Token::Text(std::borrow::Cow::Borrowed("\n")),
        ];

        let result = pipeline.rehydrate_tokens(&tokens).unwrap();
        assert_eq!(result, "Error found in file: /var/log/syslog\n");
    }

    #[test]
    fn test_rehydrate_tokens_repeat() {
        let dict_engine = DictionaryEngine::new();
        let dict = dict_engine.snapshot();

        let config = RehydrationConfig {
            preserve_order: false,
            fallback_on_error: false,
        };
        let pipeline = RehydrationPipeline::new(dict, vec![], config);

        let tokens = vec![Token::Repeat {
            token: Box::new(Token::Text(std::borrow::Cow::Borrowed("A"))),
            count: 5,
        }];

        let result = pipeline.rehydrate_tokens(&tokens).unwrap();
        assert_eq!(result, "AAAAA");
    }

    #[test]
    fn test_rehydrate_tokens_fallback() {
        let dict_engine = DictionaryEngine::new();
        let dict = dict_engine.snapshot();

        let config = RehydrationConfig {
            preserve_order: false,
            fallback_on_error: true,
        };
        let pipeline = RehydrationPipeline::new(dict, vec![], config);

        let tokens = vec![Token::DictRef(std::borrow::Cow::Borrowed("$P999"))];

        let result = pipeline.rehydrate_tokens(&tokens).unwrap();
        assert_eq!(result, "$P999");
    }

    #[test]
    fn test_rehydrate_with_plugin() {
        let dict_engine = DictionaryEngine::new();
        let dict = dict_engine.snapshot();

        let config = RehydrationConfig {
            preserve_order: false,
            fallback_on_error: false,
        };
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(DummyPlugin)];
        let pipeline = RehydrationPipeline::new(dict, plugins, config);

        let tokens = vec![Token::Text(std::borrow::Cow::Borrowed("$DUMMY|World"))];

        let output = CompressionOutput {
            tokens,
            dictionary: dict_engine.snapshot(),
            metadata: CompressionMetadata {
                original_size: 10,
                compressed_size: 5,
                compression_ratio: 0.5,
                original_tokens: 10,
                compressed_tokens: 5,
                token_savings: 5,
                token_ratio: 0.5,
                slice_count: 0,
                processing_time_ms: 0,
                order_info: None,
                base_timestamp: None,
            },
        };

        let result = pipeline.rehydrate(&output).unwrap();
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_rehydrate_tokens_diff_uses_dictionary_base() {
        let mut dict_engine = DictionaryEngine::new();
        let base_token = dict_engine.add_macro("build step failed");
        let dict = dict_engine.snapshot();

        let config = RehydrationConfig {
            preserve_order: false,
            fallback_on_error: false,
        };
        let pipeline = RehydrationPipeline::new(dict, vec![], config);

        let tokens = vec![Token::Diff {
            base: std::borrow::Cow::Owned(base_token),
            patch: std::borrow::Cow::Borrowed("2:failed->succeeded"),
        }];

        let restored = pipeline.rehydrate_tokens(&tokens).unwrap();
        assert_eq!(restored, "build step succeeded");
    }

    #[test]
    fn test_rehydrate_tokens_for_ai_diff_uses_dictionary_base() {
        let mut dict_engine = DictionaryEngine::new();
        let base_token = dict_engine.add_macro("http request failed");
        let dict = dict_engine.snapshot();

        let config = RehydrationConfig {
            preserve_order: false,
            fallback_on_error: false,
        };
        let pipeline = RehydrationPipeline::new(dict, vec![], config);

        let tokens = vec![Token::Diff {
            base: std::borrow::Cow::Owned(base_token),
            patch: std::borrow::Cow::Borrowed("2:failed->ok"),
        }];

        let restored = pipeline.rehydrate_tokens_for_ai(&tokens).unwrap();
        assert_eq!(restored, "http request ok");
    }

    #[test]
    fn test_rehydrate_for_ai_keeps_metadata_and_error_context() {
        let dict_engine = DictionaryEngine::new();
        let dict = dict_engine.snapshot();

        let config = RehydrationConfig {
            preserve_order: false,
            fallback_on_error: false,
        };
        let pipeline = RehydrationPipeline::new(dict.clone(), vec![], config);

        let tokens = vec![Token::Text(std::borrow::Cow::Borrowed(
            "[git] branch=main\nnormal build line\nERROR compile failed\nnext context line\nexit code: 1\n",
        ))];

        let output = CompressionOutput {
            tokens,
            dictionary: dict,
            metadata: CompressionMetadata {
                original_size: 10,
                compressed_size: 5,
                compression_ratio: 0.5,
                original_tokens: 10,
                compressed_tokens: 5,
                token_savings: 5,
                token_ratio: 0.5,
                slice_count: 0,
                processing_time_ms: 0,
                order_info: None,
                base_timestamp: Some("2026-03-27T00:00:00Z".to_string()),
            },
        };

        let ai = pipeline.rehydrate_for_ai(&output).unwrap();
        assert!(ai.contains("branch=main"));
        assert!(ai.contains("ERROR compile failed"));
        assert!(ai.contains("next context line"));
        assert!(ai.contains("exit code: 1"));
        assert!(ai.contains("Note: [T+Xms]"));
    }
}
