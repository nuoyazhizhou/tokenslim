//! plugin dispatcher 测试模块
//!
//! # 测试概述
//!
//! 本模块包含 plugin dispatcher 模块的单元测试 and 集成测试。
//! 测试覆盖了主要功能 and 边界情况。

#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
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

        /// 测试插件 ContextAwarePlugin 的带上下文压缩实现：返回单 token "context-path"，用于验证调度器优先走 context 路径而非 fallback 路径。
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

    /// 验证 PluginDispatcher::new 正确建立插件列表与 name→下标映射（注册 2 个插件后 plugin_map 含 test1/test2）。
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
            plugin_timeout_ms: 1000,
        };

        let dispatcher = PluginDispatcher::new(
            plugins,
            config,
            crate::core::error_isolation::SafeExecutorConfig::default(),
        );
        assert_eq!(dispatcher.plugins.len(), 2);
        assert_eq!(dispatcher.plugin_map.len(), 2);
        assert!(dispatcher.plugin_map.contains_key("test1"));
        assert!(dispatcher.plugin_map.contains_key("test2"));
    }

    /// 验证 detect_parallel 仅返回 detect 非 None 的插件，且结果按置信度降序排列（test2 的 0.9 排首位、test3 的 None 被排除）。
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
            plugin_timeout_ms: 1000,
        };

        let dispatcher = PluginDispatcher::new(
            plugins,
            config,
            crate::core::error_isolation::SafeExecutorConfig::default(),
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

    /// P2-81 回归：detect_parallel 置信度平局时按 priority 升序破平（数值小者优先），
    /// 而非回落到注册列表顺序。模拟 privacy(0) 注册在 json(146) 之后的历史事故场景——
    /// 修复前稳定排序会让 json 凭列表顺序优势先于 privacy 抢到 confidence 1.0 的含凭证切片。
    #[test]
    fn test_detect_parallel_priority_tiebreak() {
        // 注册顺序故意与优先级相反：priority 大者在前（修复前它会凭稳定序排在前面）
        let plugins: Vec<Box<dyn Plugin>> = vec![
            Box::new(TestPlugin {
                name: "json_sim",
                priority: 146,
                detect_result: Some(1.0),
            }),
            Box::new(TestPlugin {
                name: "privacy_sim",
                priority: 0,
                detect_result: Some(1.0),
            }),
        ];

        let config = DispatcherConfig {
            plugin_timeout_ms: 1000,
        };

        let dispatcher = PluginDispatcher::new(
            plugins,
            config,
            crate::core::error_isolation::SafeExecutorConfig::default(),
        );

        let slice = Slice {
            id: 1,
            text: Cow::Borrowed("error: credential leak check"),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,
            flags: Default::default(),
        };

        let detections = dispatcher.detect_parallel(&slice);
        assert_eq!(detections.len(), 2);
        assert_eq!(detections[0].0.name(), "privacy_sim", "置信度平局时 priority=0 的安全插件必须排首位");
        assert_eq!(detections[1].0.name(), "json_sim");
    }

    /// P2-81 回归（调度出口）：confidence 1.0 平局 + 候选裁剪路径下，含凭证切片由
    /// priority 最小的插件（privacy_sim）赢得压缩，而非注册列表靠前的 json_sim——
    /// 保证「凭证先于 json 折叠/字典/去重被脱敏」的安全承诺不再依赖注册顺序巧合。
    #[test]
    fn test_dispatch_confidence_tie_prefers_lowest_priority_plugin() {
        let plugins: Vec<Box<dyn Plugin>> = vec![
            Box::new(TestPlugin {
                name: "json_sim",
                priority: 146,
                detect_result: Some(1.0),
            }),
            Box::new(TestPlugin {
                name: "privacy_sim",
                priority: 0,
                detect_result: Some(1.0),
            }),
        ];

        let config = DispatcherConfig {
            plugin_timeout_ms: 1000,
        };

        let dispatcher = PluginDispatcher::new(
            plugins,
            config,
            crate::core::error_isolation::SafeExecutorConfig::default(),
        );

        let slice = Slice {
            id: 1,
            text: Cow::Borrowed("error: {\"api_key\": \"sk-secret\"}"),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,
            flags: Default::default(),
        };

        let mut dict_engine = DictionaryEngine::new();
        let mut dedup_engine = DedupEngine::new(DedupConfig {
            pattern_threshold: 2,
            ..Default::default()
        });
        let arena = Bump::new();
        let mut context = crate::core::compression_context::CompressionContext::new();

        let mut sticky: Option<&'static str> = None;
        let result = dispatcher.dispatch_slice_sticky(
            &slice,
            None,
            &mut dict_engine,
            &mut dedup_engine,
            &arena,
            &mut context,
            &mut sticky,
        );

        let joined = result
            .tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.as_ref(),
                _ => "",
            })
            .collect::<String>();
        assert!(
            joined.contains("compressed by privacy_sim"),
            "平局时必须由 priority 最小的安全插件执行压缩，实际输出: {joined}"
        );
        assert!(!joined.contains("compressed by json_sim"));
    }

    /// 验证对无关键字短文本走 quick_skip 路径，输出 parse_tier=passthrough、parse_reason=quick_skip_no_keyword。
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
            plugin_timeout_ms: 1000,
        };

        let dispatcher = PluginDispatcher::new(
            plugins,
            config,
            crate::core::error_isolation::SafeExecutorConfig::default(),
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

        let mut dict_engine = DictionaryEngine::new();
        let mut dedup_engine = DedupEngine::new(DedupConfig {
            pattern_threshold: 2,
        ..Default::default()
        });

        let arena = Bump::new();
        let mut context = crate::core::compression_context::CompressionContext::new();
        let compress_result = dispatcher.dispatch_slice_sticky(
            &slice,
            None,
            &mut dict_engine,
            &mut dedup_engine,
            &arena,
            &mut context,
            &mut None,
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

    /// 验证当插件提供 compress_with_context 时，调度结果包含 "context-path" 而非 "fallback-path"，且 parse_tier 标记为 full。
    #[test]
    fn test_dispatch_slice_uses_context_path_when_available() {
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(ContextAwarePlugin)];

        let config = DispatcherConfig {
            plugin_timeout_ms: 1000,
        };

        let dispatcher = PluginDispatcher::new(
            plugins,
            config,
            crate::core::error_isolation::SafeExecutorConfig::default(),
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

        let mut dict_engine = DictionaryEngine::new();
        let mut dedup_engine = DedupEngine::new(DedupConfig {
            pattern_threshold: 2,
        ..Default::default()
        });
        let arena = Bump::new();
        let mut context = crate::core::compression_context::CompressionContext::new();

        let compress_result = dispatcher.dispatch_slice_sticky(
            &slice,
            None,
            &mut dict_engine,
            &mut dedup_engine,
            &arena,
            &mut context,
            &mut None,
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

    /// 验证插件 detect 返回 None 且无关键字时，仍走 quick_skip 标记 passthrough（不触发 plugin_match 兜底）。
    #[test]
    fn test_dispatch_slice_marks_passthrough_on_quick_skip() {
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(TestPlugin {
            name: "test1",
            priority: 10,
            detect_result: None,
        })];

        let config = DispatcherConfig {
            plugin_timeout_ms: 1000,
        };

        let dispatcher = PluginDispatcher::new(
            plugins,
            config,
            crate::core::error_isolation::SafeExecutorConfig::default(),
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

        let mut dict_engine = DictionaryEngine::new();
        let mut dedup_engine = DedupEngine::new(DedupConfig {
            pattern_threshold: 2,
        ..Default::default()
        });
        let arena = Bump::new();
        let mut context = crate::core::compression_context::CompressionContext::new();

        let compress_result = dispatcher.dispatch_slice_sticky(
            &slice,
            None,
            &mut dict_engine,
            &mut dedup_engine,
            &arena,
            &mut context,
            &mut None,
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
        /// ShellBPlugin 的插件名称标识为 "shell_b"。
        fn name(&self) -> &'static str {
            "shell_b"
        }
        /// 返回插件优先级（数值越大越优先，此处为 20，高于 ShellA 的 10）。
        fn priority(&self) -> u8 {
            20
        }
        /// 脱壳：若文本以 "[SHELL_B] " 前缀开头则剥离并返回剩余内容，否则返回 None 表示本插件无法处理。
        fn unwrap(&self, text: &str) -> Option<String> {
            if let Some(rest) = text.strip_prefix("[SHELL_B] ") {
                Some(rest.to_string())
            } else {
                None
            }
        }
        /// 外壳插件不参与主动检测（始终返回 None），仅用于 unwrap 阶段的逐层剥离。
        fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
            None
        }
        /// 外壳插件不参与压缩（标记 unreachable!()）。
        fn compress<'a>(
            &self,
            _s: &'a Slice<'a>,
            _di: &mut DictionaryEngine,
            _de: &mut DedupEngine,
            _a: &'a Bump,
        ) -> CompressResult<'a> {
            unreachable!()
        }
        /// 外壳插件不参与解压（标记 unreachable!()）。
        fn decompress(&self, _c: &str, _d: &Dictionary) -> String {
            unreachable!()
        }
    }

    /// 验证 unwrap_recursive 能递归剥离多层嵌套外壳（[SHELL_B] [SHELL_A] [SHELL_B]），最终得到最内层 "inner payload"。
    #[test]
    fn test_unwrap_recursive_multiple_shells() {
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(ShellAPlugin), Box::new(ShellBPlugin)];

        let config = DispatcherConfig {
            plugin_timeout_ms: 1000,
        };

        let dispatcher = PluginDispatcher::new(
            plugins,
            config,
            crate::core::error_isolation::SafeExecutorConfig::default(),
        );

        let input = "[SHELL_B] [SHELL_A] [SHELL_B] inner payload";
        let output = dispatcher.unwrap_recursive(input);

        assert_eq!(output.as_ref(), "inner payload");
    }
}

#[cfg(test)]
mod p1_10_panic_isolation_tests {
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
    use crate::core::plugin_dispatcher::{
        CompressResult, DispatcherConfig, Plugin, PluginDispatcher,
    };
    use crate::core::text_slicer::{Slice, SliceType};
    use bumpalo::Bump;
    use std::borrow::Cow;
    use std::sync::Arc;

    // ─── P1-10：插件 panic 隔离与黑名单接线 ─────────────────────────────

    /// panic 注入插件：detect 高置信命中，compress_with_context 必 panic。
    struct PanickyPlugin {
        name: &'static str,
    }

    impl Plugin for PanickyPlugin {
        fn name(&self) -> &'static str {
            self.name
        }

        fn priority(&self) -> u8 {
            0
        }

        fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
            Some(0.99)
        }

        /// P1-10 注入点：直接 panic 模拟插件缺陷（索引越界/unwrap 失误等真实形态）。
        fn compress_with_context<'a>(
            &self,
            _slice: &'a Slice<'a>,
            _dict_engine: &mut DictionaryEngine,
            _dedup_engine: &mut DedupEngine,
            _arena: &'a Bump,
            _context: &mut crate::core::compression_context::CompressionContext,
        ) -> CompressResult<'a> {
            panic!("synthetic plugin panic: index out of bounds");
        }

        fn compress<'a>(
            &self,
            _slice: &'a Slice<'a>,
            _dict_engine: &mut DictionaryEngine,
            _dedup_engine: &mut DedupEngine,
            _arena: &'a Bump,
        ) -> CompressResult<'a> {
            panic!("synthetic plugin panic in compress");
        }

        fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
            compressed.to_string()
        }
    }

    fn make_test_slice<'a>(arena: &'a Bump, text: &'a str) -> Slice<'a> {
        Slice {
            id: 1,
            text: Cow::Borrowed(arena.alloc_str(text)),
            slice_type: SliceType::Line,
            offset: 0,
            line_start: 1,
            line_end: 1,
            file_metadata: None,
            flags: Default::default(),
        }
    }

    fn make_panic_dispatch_env() -> (
        DictionaryEngine,
        DedupEngine,
        crate::core::compression_context::CompressionContext,
    ) {
        (
            DictionaryEngine::new(),
            DedupEngine::new(DedupConfig::default()),
            crate::core::compression_context::CompressionContext::new(),
        )
    }

    /// P1-10 回归：插件 panic 不得击穿压缩进程——被隔离为「本次产出为空」，
    /// 调度层降级（degraded / plugin_failed），且黑名单与 panic 计数各 +1。
    #[test]
    fn plugin_panic_is_isolated_and_counted() {
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(PanickyPlugin { name: "panicky" })];
        let dispatcher = PluginDispatcher::new(
            plugins,
            DispatcherConfig::default(),
            Default::default(),
        );
        let arena = Bump::new();
        let (mut dict_engine, mut dedup, mut context) = make_panic_dispatch_env();
        let slice = make_test_slice(&arena, "http panic test line");

        let result = dispatcher.dispatch_slice_sticky(
            &slice,
            None,
            &mut dict_engine,
            &mut dedup,
            &arena,
            &mut context,
            &mut None,
        );

        // 隔离：不 panic，降级路径返回原文（无插件产出）。
        let tier = result
            .metadata
            .as_ref()
            .and_then(|m| m.get("parse_tier"))
            .cloned()
            .unwrap_or_default();
        let reason = result
            .metadata
            .as_ref()
            .and_then(|m| m.get("parse_reason"))
            .cloned()
            .unwrap_or_default();
        assert_eq!(tier, "degraded", "panic 后应走降级路径");
        assert_eq!(reason, "plugin_failed");

        // 黑名单计数 +1（P1-10 核心验收）。
        let failures = dispatcher.plugin_failures.lock().unwrap().clone();
        assert_eq!(
            failures.get("panicky"),
            Some(&1),
            "panic 必须计入失败黑名单"
        );

        // panic 指标计数 +1（P2-63 接线验收）。
        let panics = dispatcher.take_plugin_panic_counts();
        assert_eq!(panics.get("panicky"), Some(&1));
    }

    /// P1-10 回归：连续 3 次 panic 后插件进入黑名单，后续调度跳过
    /// （不再反复炸，detect 阶段直接过滤）。
    #[test]
    fn plugin_blacklisted_after_three_panics() {
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(PanickyPlugin { name: "panicky" })];
        let dispatcher = PluginDispatcher::new(
            plugins,
            DispatcherConfig::default(),
            Default::default(),
        );
        let arena = Bump::new();
        let (mut dict_engine, mut dedup, mut context) = make_panic_dispatch_env();
        let slice = make_test_slice(&arena, "http panic test line");

        for _round in 0..3 {
            dispatcher.dispatch_slice_sticky(
                &slice,
                None,
                &mut dict_engine,
                &mut dedup,
                &arena,
                &mut context,
                &mut None,
            );
        }

        // 第 3 次 panic 后黑名单生效：detect_parallel 不再返回该插件。
        let detections = dispatcher.detect_parallel(&slice);
        assert!(
            detections.iter().all(|(p, _)| p.name() != "panicky"),
            "黑名单插件不得再进入 detect 候选"
        );

        let failures = dispatcher.plugin_failures.lock().unwrap().clone();
        assert_eq!(failures.get("panicky"), Some(&3));
    }
}
