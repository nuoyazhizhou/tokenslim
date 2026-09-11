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
        /// 测试桩插件名称：固定返回 "dummy"。
        fn name(&self) -> &'static str {
            "dummy"
        }
        /// 测试桩插件优先级：固定返回 100。
        fn priority(&self) -> u8 {
            100
        }
        /// 测试桩插件内容检测：始终返回 None，即从不主动匹配任何输入。
        fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
            None
        }
        /// 测试桩插件压缩：固定产出一个 Text("dummy") token，不写入字典与去重。
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
        /// 测试桩插件解压：将压缩文本中的 "$DUMMY|" 前缀替换为 "Hello "。
        fn decompress(
            &self,
            compressed: &str,
            _dict: &crate::core::dictionary_engine::Dictionary,
        ) -> String {
            compressed.replace("$DUMMY|", "Hello ")
        }
    }

    /// 测试：构造 RehydrationPipeline 后，插件名称映射表中应包含 "dummy"。
    #[test]
    fn test_new() {
        let dict_engine = DictionaryEngine::new();
        let dict = dict_engine.snapshot();
        let config = RehydrationConfig {
            fallback_on_error: false,
        };
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(DummyPlugin)];

        let pipeline = RehydrationPipeline::new(dict, plugins, config);
        assert!(
            pipeline.plugins.iter().any(|pl| pl.name() == "dummy"),
            "插件表应包含 dummy（P1-02 后为有序 Vec）"
        );
    }

    /// 测试：还原混合 Text 与 DictRef 的 token 序列，字典引用应解析为原始路径文本。
    #[test]
    fn test_rehydrate_tokens_text_and_dict() {
        let mut dict_engine = DictionaryEngine::new();
        let path_token = dict_engine.add_path_layered("/var/log/syslog");
        let dict = dict_engine.snapshot();

        let config = RehydrationConfig {
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

    /// 测试：无法解析的字典引用（如 $P999）在 fallback 模式下原样输出、不报错。
    #[test]
    fn test_rehydrate_tokens_fallback() {
        let dict_engine = DictionaryEngine::new();
        let dict = dict_engine.snapshot();

        let config = RehydrationConfig {
            fallback_on_error: true,
        };
        let pipeline = RehydrationPipeline::new(dict, vec![], config);

        let tokens = vec![Token::DictRef(std::borrow::Cow::Borrowed("$P999"))];

        let result = pipeline.rehydrate_tokens(&tokens).unwrap();
        assert_eq!(result, "$P999");
    }

    /// 测试：经插件压缩产生的特殊编码（$DUMMY|World）在还原时被插件解压逻辑还原为明文。
    #[test]
    fn test_rehydrate_with_plugin() {
        let dict_engine = DictionaryEngine::new();
        let dict = dict_engine.snapshot();

        let config = RehydrationConfig {
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
                source_encoding: None,
            },
        };

        let result = pipeline.rehydrate(&output).unwrap();
        assert_eq!(result, "Hello World");
    }

    /// 测试：rehydrate_for_ai 保留 git 元数据行与 Error 上下文窗口，并注入 Base Timestamp 说明前缀。
    #[test]
    fn test_rehydrate_for_ai_keeps_metadata_and_error_context() {
        let dict_engine = DictionaryEngine::new();
        let dict = dict_engine.snapshot();

        let config = RehydrationConfig {
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
                source_encoding: None,
            },
        };

        let ai = pipeline.rehydrate_for_ai(&output).unwrap();
        assert!(ai.contains("branch=main"));
        assert!(ai.contains("ERROR compile failed"));
        assert!(ai.contains("next context line"));
        assert!(ai.contains("exit code: 1"));
        assert!(ai.contains("Note: [T+Xms]"));
    }

    /// 测试桩：priority=1，decompress 把 "alpha" 改写为 "beta"。
    struct OrderAPlugin;
    impl Plugin for OrderAPlugin {
        fn name(&self) -> &'static str {
            "order_a"
        }
        fn priority(&self) -> u8 {
            1
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
                tokens: vec![Token::Text("alpha".into())],
                metadata: None,
                plugin_name: Some("order_a"),
            }
        }
        fn decompress(
            &self,
            compressed: &str,
            _dict: &crate::core::dictionary_engine::Dictionary,
        ) -> String {
            compressed.replace("alpha", "beta")
        }
    }

    /// 测试桩：priority=2，decompress 把 "beta" 改写为 "gamma"。
    struct OrderBPlugin;
    impl Plugin for OrderBPlugin {
        fn name(&self) -> &'static str {
            "order_b"
        }
        fn priority(&self) -> u8 {
            2
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
                tokens: vec![Token::Text("beta".into())],
                metadata: None,
                plugin_name: Some("order_b"),
            }
        }
        fn decompress(
            &self,
            compressed: &str,
            _dict: &crate::core::dictionary_engine::Dictionary,
        ) -> String {
            compressed.replace("beta", "gamma")
        }
    }

    /// 构造最小 CompressionOutput 的测试辅助。
    pub(crate) fn make_output(
        tokens: Vec<Token<'static>>,
        dict: crate::core::dictionary_engine::Dictionary,
    ) -> CompressionOutput {
        CompressionOutput {
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
                slice_count: 1,
                processing_time_ms: 0,
                order_info: None,
                base_timestamp: None,
                source_encoding: None,
            },
        }
    }

    /// P1-01 回归：普通解压模式下内联进 Text 的 `$Pn` 必须被全局兜底解析——
    /// 旧实现 `resolve_recursive` 被 `$PL/$FL` 门控挡住，绝大多数输出（不含
    /// `$PL`）的 `$Pn` 原样残留（协议 token 泄漏到用户可见输出）。
    #[test]
    fn rehydrate_resolves_inline_path_token_without_pl_gate() {
        let mut dict_engine = DictionaryEngine::new();
        let path_token = dict_engine.add_path_layered("/var/log/app/error.log");
        let dict = dict_engine.snapshot();

        let config = RehydrationConfig {
            fallback_on_error: false,
        };
        let pipeline = RehydrationPipeline::new(dict.clone(), vec![], config);

        let tokens = vec![Token::Text(std::borrow::Cow::Owned(format!(
            " --> {}:5:9\n",
            path_token
        )))];
        let output = make_output(tokens, dict);

        let restored = pipeline.rehydrate(&output).unwrap();
        assert!(
            restored.contains("/var/log/app/error.log"),
            "内联路径 token 应被还原: {restored}"
        );
        assert!(
            !restored.contains("$P"),
            "不允许残留协议 token（P1-01）: {restored}"
        );
    }

    /// P1-02 回归：插件 decompress 顺序不得依赖插入顺序/HashMap 迭代序——
    /// 同一插件集合、不同插入顺序的两个 Pipeline，解压结果必须逐字节一致。
    #[test]
    fn plugin_decompress_order_is_deterministic_regardless_of_insertion_order() {
        let dict_engine = DictionaryEngine::new();
        let dict = dict_engine.snapshot();
        let config = RehydrationConfig {
            fallback_on_error: false,
        };

        // OrderA(priority=1): alpha -> beta；OrderB(priority=2): beta -> gamma。
        // 按 priority 排序后两管线执行序均为 A→B，输出 "gamma"。
        let p_reverse = RehydrationPipeline::new(
            dict.clone(),
            vec![Box::new(OrderBPlugin), Box::new(OrderAPlugin)],
            config.clone(),
        );
        let p_forward = RehydrationPipeline::new(
            dict.clone(),
            vec![Box::new(OrderAPlugin), Box::new(OrderBPlugin)],
            config,
        );

        let tokens = vec![Token::Text(std::borrow::Cow::Borrowed("alpha"))];
        let out1 = make_output(tokens.clone(), dict.clone());
        let out2 = make_output(tokens, dict);

        let r1 = p_reverse.rehydrate(&out1).unwrap();
        let r2 = p_forward.rehydrate(&out2).unwrap();
        assert_eq!(r1, "gamma", "固定优先级下链式改写应得 gamma: {r1}");
        assert_eq!(r1, r2, "插入顺序不同不得影响解压结果（P1-02）");
    }
}

/// P2-65 接线回归：RehydrationError 死错误通道激活（严格模式构造 Err，
/// 宽松模式保持历史行为），以及残留检测对 shell 变量字面量零误报。
#[cfg(test)]
mod p2_65_strict_rehydration_tests {
    use super::tests::make_output;
    use crate::core::compression::Token;
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::rehydration_pipeline::{RehydrationConfig, RehydrationPipeline};

    fn strict_config() -> RehydrationConfig {
        RehydrationConfig {
            fallback_on_error: false,
        }
    }

    /// 严格模式下，无法解析的 DictRef（$P999）应构造 DictResolutionFailed 上传，
    /// 而非静默残留（对应宽松版 test_rehydrate_tokens_fallback）。
    #[test]
    fn strict_mode_unresolved_dict_ref_errors() {
        let dict_engine = DictionaryEngine::new();
        let dict = dict_engine.snapshot();
        let pipeline = RehydrationPipeline::new(dict.clone(), vec![], strict_config());

        let tokens = vec![Token::DictRef(std::borrow::Cow::Borrowed("$P999"))];
        let err = pipeline
            .rehydrate_tokens(&tokens)
            .expect_err("严格模式下未解析 DictRef 应报错");
        assert!(
            err.to_string().contains("E_REHYDRATION_DICT_RESOLUTION_FAILED:$P999"),
            "错误信息应含错误码与 token：{err}"
        );
    }

    /// 宽松模式（CLI 默认）行为不变：终态残留 $P99 原样保留、不报错——
    /// 兼容旧版本压缩产物，基线零影响。
    #[test]
    fn lenient_mode_residue_kept_and_no_error() {
        let dict_engine = DictionaryEngine::new();
        let dict = dict_engine.snapshot();
        let pipeline = RehydrationPipeline::new(dict.clone(), vec![], RehydrationConfig::default());

        let tokens = vec![Token::Text(std::borrow::Cow::Borrowed("path=$P99 end\n"))];
        let output = make_output(tokens, dict);
        let result = pipeline.rehydrate(&output).unwrap();
        assert!(
            result.contains("$P99"),
            "宽松模式残留 token 应原样保留：{result}"
        );
    }

    /// 严格模式下，普通模式解压终态残留字典键 token 应构造 UnknownToken 上传。
    #[test]
    fn strict_mode_residue_errors_as_unknown_token() {
        let dict_engine = DictionaryEngine::new();
        let dict = dict_engine.snapshot();
        let pipeline = RehydrationPipeline::new(dict.clone(), vec![], strict_config());

        let tokens = vec![Token::Text(std::borrow::Cow::Borrowed("path=$P99 end\n"))];
        let output = make_output(tokens, dict);
        let err = pipeline
            .rehydrate(&output)
            .expect_err("严格模式残留 $P99 应报错");
        assert!(
            err.to_string().contains("E_REHYDRATION_UNKNOWN_TOKEN:$P99"),
            "错误信息应含错误码与残留 token：{err}"
        );
    }

    /// 严格模式零误报：日志原文中的 $PATH/$HOME/${VAR} 与可正常解析的 $P1
    /// 均不得触发错误；可解析 token 应展开为真实路径。
    #[test]
    fn strict_mode_no_false_positive_on_shell_variables() {
        let mut dict_engine = DictionaryEngine::new();
        let path_token = dict_engine.add_path_layered("/var/log/app/error.log");
        let dict = dict_engine.snapshot();
        let pipeline = RehydrationPipeline::new(dict.clone(), vec![], strict_config());

        let tokens = vec![Token::Text(std::borrow::Cow::Owned(format!(
            "export PATH=$PATH HOME=$HOME EXPAND=${{VAR}} log={}\n",
            path_token
        )))];
        let output = make_output(tokens, dict);
        let result = pipeline.rehydrate(&output).unwrap();
        assert!(
            result.contains("/var/log/app/error.log"),
            "可解析 $P token 应展开：{result}"
        );
        assert!(
            result.contains("$PATH") && result.contains("$HOME") && result.contains("${VAR}"),
            "shell 变量字面量应原样保留：{result}"
        );
    }

    /// AI 模式严格检查豁免 $D：目录 token 是 resolve_for_ai 的刻意保留设计
    ///（配合导出侧 [Directories] 节消费），不算残留；但未解析的 $P99 仍报错。
    #[test]
    fn strict_ai_mode_allows_directory_tokens_but_flags_path_residue() {
        let dict_engine = DictionaryEngine::new();
        let dict = dict_engine.snapshot();
        let pipeline = RehydrationPipeline::new(dict.clone(), vec![], strict_config());

        // $D1 保留 → Ok（ERROR 前缀使行进入 AI 上下文保留窗口，不被折叠）
        let tokens_d = vec![Token::Text(std::borrow::Cow::Borrowed("ERROR dir=$D1\n"))];
        let out_d = make_output(tokens_d, dict.clone());
        pipeline
            .rehydrate_for_ai(&out_d)
            .expect("AI 模式 $D 保留不算残留");

        // $P99 残留 → Err
        let tokens_p = vec![Token::Text(std::borrow::Cow::Borrowed("ERROR path=$P99\n"))];
        let out_p = make_output(tokens_p, dict);
        let err = pipeline.rehydrate_for_ai(&out_p).expect_err("AI 模式 $P99 残留应报错");
        assert!(
            err.to_string().contains("E_REHYDRATION_UNKNOWN_TOKEN:$P99"),
            "错误信息应含错误码与残留 token：{err}"
        );
    }
}
