//! plugin dispatcher 测试模块
//!
//! # 测试概述
//!
//! 本模块包含 plugin dispatcher 模块的单元测试 and 集成测试。
//! 测试覆盖了主要功能 and 边界情况。

#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::content_analyzer::AnalysisResult;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
    use crate::core::dictionary_manager::DictionaryManager;
    use crate::core::plugin_dispatcher::{
        CompressResult, DispatcherConfig, Plugin, PluginDispatcher,
    };
    use crate::core::text_slicer::{Slice, SliceType};
    use bumpalo::Bump;
    use std::borrow::Cow;
    use std::sync::Arc;

    struct ContextAwarePlugin;

    impl Plugin for ContextAwarePlugin {
        fn name(&self) -> &'static str {
            "context_aware"
        }

        fn priority(&self) -> u8 {
            1
        }

        fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
            Some(0.99)
        }

        fn compress<'a>(
            &self,
            _slice: &'a Slice<'a>,
            _dict_engine: &mut DictionaryEngine,
            _dedup_engine: &mut DedupEngine,
            _arena: &'a Bump,
        ) -> CompressResult<'a> {
            CompressResult {
                tokens: vec![Token::Text(Cow::Borrowed("fallback-path"))],
                metadata: None,
                plugin_name: Some(self.name()),
            }
        }

        fn compress_with_context<'a>(
            &self,
            _slice: &'a Slice<'a>,
            _dict_engine: &mut DictionaryEngine,
            _dedup_engine: &mut DedupEngine,
            _arena: &'a Bump,
            _context: &mut crate::core::compression_context::CompressionContext,
        ) -> CompressResult<'a> {
            CompressResult {
                tokens: vec![Token::Text(Cow::Borrowed("context-path"))],
                metadata: None,
                plugin_name: Some(self.name()),
            }
        }

        fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
            compressed.to_string()
        }
    }

    // 测试插件实现
    struct TestPlugin {
        name: &'static str,
        priority: u8,
        detect_result: Option<f32>,
    }

    impl Plugin for TestPlugin {
        fn name(&self) -> &'static str {
            self.name
        }

        fn priority(&self) -> u8 {
            self.priority
        }

        fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
            self.detect_result
        }

        fn compress<'a>(
            &self,
            _slice: &'a Slice<'a>,
            _dict_engine: &mut DictionaryEngine,
            _dedup_engine: &mut DedupEngine,
            _arena: &'a Bump,
        ) -> CompressResult<'a> {
            CompressResult {
                tokens: vec![Token::Text(format!("compressed by {}", self.name).into())],
                metadata: None,
                plugin_name: None,
            }
        }

        fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
            format!("decompressed: {}", compressed)
        }
    }

    #[test]
    fn test_new() {
        let plugins: Vec<Box<dyn Plugin>> = vec![
            Box::new(TestPlugin {
                name: "test1",
                priority: 10,
                detect_result: Some(0.8),
            }),
            Box::new(TestPlugin {
                name: "test2",
                priority: 20,
                detect_result: Some(0.9),
            }),
        ];

        let config = DispatcherConfig {
            fallback_plugin: "test1".to_string(),
            plugin_timeout_ms: 1000,
        };

        let dict_manager = Arc::new(DictionaryManager::new());
        let dispatcher = PluginDispatcher::new(
            plugins,
            config,
            crate::core::error_isolation::SafeExecutorConfig::default(),
            dict_manager,
        );
        assert_eq!(dispatcher.plugins.len(), 2);
        assert_eq!(dispatcher.plugin_map.len(), 2);
        assert!(dispatcher.plugin_map.contains_key("test1"));
        assert!(dispatcher.plugin_map.contains_key("test2"));
    }

    #[test]
    fn test_detect_parallel() {
        let plugins: Vec<Box<dyn Plugin>> = vec![
            Box::new(TestPlugin {
                name: "test1",
                priority: 10,
                detect_result: Some(0.8),
            }),
            Box::new(TestPlugin {
                name: "test2",
                priority: 20,
                detect_result: Some(0.9),
            }),
            Box::new(TestPlugin {
                name: "test3",
                priority: 30,
                detect_result: None,
            }),
        ];

        let config = DispatcherConfig {
            fallback_plugin: "test1".to_string(),
            plugin_timeout_ms: 1000,
        };

        let dict_manager = Arc::new(DictionaryManager::new());
        let dispatcher = PluginDispatcher::new(
            plugins,
            config,
            crate::core::error_isolation::SafeExecutorConfig::default(),
            dict_manager,
        );

        let slice = Slice {
            id: 1,
            text: Cow::Borrowed("test text"),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,
            flags: Default::default(),
        };

        let detections = dispatcher.detect_parallel(&slice);
        assert_eq!(detections.len(), 2);
        assert_eq!(detections[0].0.name(), "test2"); // 信心值更高 (0.9 vs 0.8)
    }

    #[test]
    fn test_dispatch_slice() {
        let plugins: Vec<Box<dyn Plugin>> = vec![
            Box::new(TestPlugin {
                name: "test1",
                priority: 10,
                detect_result: Some(0.8),
            }),
            Box::new(TestPlugin {
                name: "test2",
                priority: 20,
                detect_result: Some(0.9),
            }),
        ];

        let config = DispatcherConfig {
            fallback_plugin: "test1".to_string(),
            plugin_timeout_ms: 1000,
        };

        let dict_manager = Arc::new(DictionaryManager::new());
        let dispatcher = PluginDispatcher::new(
            plugins,
            config,
            crate::core::error_isolation::SafeExecutorConfig::default(),
            dict_manager,
        );

        let slice = Slice {
            id: 1,
            text: Cow::Borrowed("test text"),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,
            flags: Default::default(),
        };

        let result = AnalysisResult {
            slice_type: SliceType::Line,
            confidence: 0.9,
            details: None,
        };

        let mut dict_engine = DictionaryEngine::new();
        let mut dedup_engine = DedupEngine::new(DedupConfig {
            line_threshold: 2,
            stack_frame_threshold: 2,
            path_threshold: 2,
            pattern_threshold: 2,
            fuzzy_threshold: 0.9,
        });

        let arena = Bump::new();
        let mut context = crate::core::compression_context::CompressionContext::new();
        let compress_result = dispatcher.dispatch_slice(
            &slice,
            &result,
            None,
            &mut dict_engine,
            &mut dedup_engine,
            &arena,
            &mut context,
        );
        assert!(!compress_result.tokens.is_empty());
        assert_eq!(
            compress_result
                .metadata
                .as_ref()
                .and_then(|m| m.get("parse_tier"))
                .map(String::as_str),
            Some("passthrough")
        );
        assert_eq!(
            compress_result
                .metadata
                .as_ref()
                .and_then(|m| m.get("parse_reason"))
                .map(String::as_str),
            Some("quick_skip_no_keyword")
        );
    }

    #[test]
    fn test_dispatch_slice_uses_context_path_when_available() {
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(ContextAwarePlugin)];

        let config = DispatcherConfig {
            fallback_plugin: "context_aware".to_string(),
            plugin_timeout_ms: 1000,
        };

        let dict_manager = Arc::new(DictionaryManager::new());
        let dispatcher = PluginDispatcher::new(
            plugins,
            config,
            crate::core::error_isolation::SafeExecutorConfig::default(),
            dict_manager,
        );

        let slice = Slice {
            id: 1,
            text: Cow::Borrowed("error: context routing check"),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,
            flags: Default::default(),
        };

        let result = AnalysisResult {
            slice_type: SliceType::Line,
            confidence: 0.9,
            details: None,
        };

        let mut dict_engine = DictionaryEngine::new();
        let mut dedup_engine = DedupEngine::new(DedupConfig {
            line_threshold: 2,
            stack_frame_threshold: 2,
            path_threshold: 2,
            pattern_threshold: 2,
            fuzzy_threshold: 0.9,
        });
        let arena = Bump::new();
        let mut context = crate::core::compression_context::CompressionContext::new();

        let compress_result = dispatcher.dispatch_slice(
            &slice,
            &result,
            None,
            &mut dict_engine,
            &mut dedup_engine,
            &arena,
            &mut context,
        );

        let joined = compress_result
            .tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.as_ref(),
                _ => "",
            })
            .collect::<String>();

        assert!(joined.contains("context-path"));
        assert!(!joined.contains("fallback-path"));
        assert_eq!(
            compress_result
                .metadata
                .as_ref()
                .and_then(|m| m.get("parse_tier"))
                .map(String::as_str),
            Some("full")
        );
    }

    #[test]
    fn test_dispatch_slice_marks_passthrough_on_quick_skip() {
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(TestPlugin {
            name: "test1",
            priority: 10,
            detect_result: None,
        })];

        let config = DispatcherConfig {
            fallback_plugin: "test1".to_string(),
            plugin_timeout_ms: 1000,
        };

        let dict_manager = Arc::new(DictionaryManager::new());
        let dispatcher = PluginDispatcher::new(
            plugins,
            config,
            crate::core::error_isolation::SafeExecutorConfig::default(),
            dict_manager,
        );

        let slice = Slice {
            id: 2,
            text: Cow::Borrowed("plaintext"),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,
            flags: Default::default(),
        };

        let result = AnalysisResult {
            slice_type: SliceType::Line,
            confidence: 0.5,
            details: None,
        };

        let mut dict_engine = DictionaryEngine::new();
        let mut dedup_engine = DedupEngine::new(DedupConfig {
            line_threshold: 2,
            stack_frame_threshold: 2,
            path_threshold: 2,
            pattern_threshold: 2,
            fuzzy_threshold: 0.9,
        });
        let arena = Bump::new();
        let mut context = crate::core::compression_context::CompressionContext::new();

        let compress_result = dispatcher.dispatch_slice(
            &slice,
            &result,
            None,
            &mut dict_engine,
            &mut dedup_engine,
            &arena,
            &mut context,
        );

        assert_eq!(
            compress_result
                .metadata
                .as_ref()
                .and_then(|m| m.get("parse_tier"))
                .map(String::as_str),
            Some("passthrough")
        );
        assert_eq!(
            compress_result
                .metadata
                .as_ref()
                .and_then(|m| m.get("parse_reason"))
                .map(String::as_str),
            Some("quick_skip_no_keyword")
        );
    }

    struct ShellAPlugin;
    impl Plugin for ShellAPlugin {
        fn name(&self) -> &'static str {
            "shell_a"
        }
        fn priority(&self) -> u8 {
            10
        }
        fn unwrap(&self, text: &str) -> Option<String> {
            if let Some(rest) = text.strip_prefix("[SHELL_A] ") {
                Some(rest.to_string())
            } else {
                None
            }
        }
        fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
            None
        }
        fn compress<'a>(
            &self,
            _s: &'a Slice<'a>,
            _di: &mut DictionaryEngine,
            _de: &mut DedupEngine,
            _a: &'a Bump,
        ) -> CompressResult<'a> {
            unreachable!()
        }
        fn decompress(&self, _c: &str, _d: &Dictionary) -> String {
            unreachable!()
        }
    }

    struct ShellBPlugin;
    impl Plugin for ShellBPlugin {
        fn name(&self) -> &'static str {
            "shell_b"
        }
        fn priority(&self) -> u8 {
            20
        }
        fn unwrap(&self, text: &str) -> Option<String> {
            if let Some(rest) = text.strip_prefix("[SHELL_B] ") {
                Some(rest.to_string())
            } else {
                None
            }
        }
        fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
            None
        }
        fn compress<'a>(
            &self,
            _s: &'a Slice<'a>,
            _di: &mut DictionaryEngine,
            _de: &mut DedupEngine,
            _a: &'a Bump,
        ) -> CompressResult<'a> {
            unreachable!()
        }
        fn decompress(&self, _c: &str, _d: &Dictionary) -> String {
            unreachable!()
        }
    }

    #[test]
    fn test_unwrap_recursive_multiple_shells() {
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(ShellAPlugin), Box::new(ShellBPlugin)];

        let config = DispatcherConfig {
            fallback_plugin: "fallback".to_string(),
            plugin_timeout_ms: 1000,
        };

        let dict_manager = Arc::new(DictionaryManager::new());
        let dispatcher = PluginDispatcher::new(
            plugins,
            config,
            crate::core::error_isolation::SafeExecutorConfig::default(),
            dict_manager,
        );

        let input = "[SHELL_B] [SHELL_A] [SHELL_B] inner payload";
        let output = dispatcher.unwrap_recursive(input);

        assert_eq!(output.as_ref(), "inner payload");
    }
}
