//! compression pipeline 测试模块
//!
//! # 测试概述
//!
//! 本模块包含 compression pipeline 模块的单元测试与集成测试。
//! 测试覆盖了主要功能与边界情况。

#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
    use crate::core::content_analyzer::ContentAnalyzer;
    use crate::core::content_classifier::Category;
    use crate::core::dedup_engine::DedupConfig;
    use crate::core::dictionary_engine::Dictionary;
    use crate::core::metrics::MetricsCollector;
    use crate::core::metrics::MetricsConfig;
    use crate::core::plugin_dispatcher::{CompressResult, DispatcherConfig, Plugin};
    use crate::core::text_slicer::{Slice, SlicerConfig};
    use bumpalo::Bump;
    use std::borrow::Cow;

    /// 测试插件实现。
    ///
    /// 以固定 name/priority 和一个始终返回 0.9 置信度的 `detect`，
    /// 配合"compress 输出前缀 `compressed: `"的简单契约，用于验证
    /// compression pipeline 的装配、调度与指标采集逻辑，不依赖真实插件。
    struct TestPlugin {
        name: &'static str,
        priority: u8,
    }

    impl Plugin for TestPlugin {
        /// 返回插件标识名（测试用固定 name）。
        fn name(&self) -> &'static str {
            self.name
        }

        /// 返回插件优先级（测试用固定 priority）。
        fn priority(&self) -> u8 {
            self.priority
        }

        /// 恒返回 0.9 置信度，保证测试中该插件总是被选中参与压缩。
        fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
            Some(0.9)
        }

        /// 测试压缩：将输入切片原文包装为 `compressed: {text}` 单个文本 Token，
        /// 便于断言压缩结果是否经过本插件路径。
        fn compress<'a>(
            &self,
            slice: &'a Slice<'a>,
            _dict_engine: &mut crate::core::dictionary_engine::DictionaryEngine,
            _dedup_engine: &mut crate::core::dedup_engine::DedupEngine,
            _arena: &'a Bump,
        ) -> CompressResult<'a> {
            CompressResult {
                tokens: vec![Token::Text(Cow::Owned(format!(
                    "compressed: {}",
                    slice.text
                )))],
                metadata: None,
                plugin_name: Some("dummy_plugin"),
            }
        }

        /// 测试解压：原样返回输入（测试插件无真正字典展开逻辑）。
        fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
            compressed.to_string()
        }

        /// 测试展开：仅当文本以 `csv_header` 开头且含换行时返回两行 mock 展开结果，
        /// 用于验证 pipeline 的 chunk unwrap 路径。
        fn unwrap(&self, text: &str) -> Option<String> {
            if text.starts_with("csv_header") && text.contains('\n') {
                // 返回 mock 的多行展开字符串
                Some("unwrapped_line1\nunwrapped_line2\n".to_string())
            } else {
                None
            }
        }
    }

    /// 验证 `CompressionPipeline::new` 在给定完整配置与插件列表下可正常构造。
    ///
    /// 覆盖 pipeline 各子配置（slicer/analyzer/dispatcher/dedup/reorder/metrics）
    /// 的装配路径，断言构造不 panic。
    #[test]
    fn test_new() {
        let slicer_config = SlicerConfig::default();

        let dispatcher_config = DispatcherConfig {
            plugin_timeout_ms: 1000,
        };

        let dedup_config = DedupConfig {
            pattern_threshold: 2,
        ..Default::default()
        };

        let pipeline_config = PipelineConfig {
            slicer_config,
            dispatcher_config,
            dedup_config,
            reorder_config: crate::core::log_reorderer::ReorderConfig::default(),
            stream_buffer_size: 4096,
            parallel_threshold: 1024 * 1024,
            dictionary_threshold: 0,
            debug_audit_jsonl: None,
            debug_audit_attribution: crate::core::debug_audit::DebugAuditAttribution::default(),
        };

        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(TestPlugin {
            name: "test",
            priority: 10,
        })];

        let metrics = MetricsCollector::new(MetricsConfig {
            enabled: false,
            enable_module_timing: false,
            enable_plugin_stats: false,
            enable_error_logging: false,
            max_error_logs: 100,
        });

        let _pipeline = CompressionPipeline::new(pipeline_config, plugins, metrics);
    }

    /// 验证 `compress_str` 端到端路径：文本经切片、分析、分派压缩后
    /// 返回非空 Token 列表、空路径字典与正数原始大小元数据。
    #[test]
    fn test_compress_str() {
        let slicer_config = SlicerConfig::default();

        let dispatcher_config = DispatcherConfig {
            plugin_timeout_ms: 1000,
        };

        let dedup_config = DedupConfig {
            pattern_threshold: 2,
        ..Default::default()
        };

        let pipeline_config = PipelineConfig {
            slicer_config,
            dispatcher_config,
            dedup_config,
            reorder_config: crate::core::log_reorderer::ReorderConfig::default(),
            stream_buffer_size: 4096,
            parallel_threshold: 1024 * 1024,
            dictionary_threshold: 0,
            debug_audit_jsonl: None,
            debug_audit_attribution: crate::core::debug_audit::DebugAuditAttribution::default(),
        };

        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(TestPlugin {
            name: "test",
            priority: 10,
        })];

        let metrics = MetricsCollector::new(MetricsConfig {
            enabled: false,
            enable_module_timing: false,
            enable_plugin_stats: false,
            enable_error_logging: false,
            max_error_logs: 100,
        });

        let mut pipeline = CompressionPipeline::new(pipeline_config, plugins, metrics);

        let text = "Hello world\nTest line";
        let result = pipeline.compress_str(text);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.tokens.is_empty());
        assert!(output.dictionary.paths.is_empty());
        assert!(output.metadata.original_size > 0);
    }

    /// 验证启用 reorder 时即使输入超过并行阈值（>128KB）仍强制走串行路径，
    /// 串行路径上报基于行数的非零 `slice_count`（并行路径当前上报 0）。
    #[test]
    fn test_reorder_enabled_forces_serial_path_even_for_large_input() {
        let slicer_config = SlicerConfig::default();

        let dispatcher_config = DispatcherConfig {
            plugin_timeout_ms: 1000,
        };

        let dedup_config = DedupConfig {
            pattern_threshold: 2,
        ..Default::default()
        };

        let pipeline_config = PipelineConfig {
            slicer_config,
            dispatcher_config,
            dedup_config,
            reorder_config: crate::core::log_reorderer::ReorderConfig {
                enabled: true,
                ..Default::default()
            },
            stream_buffer_size: 4096,
            parallel_threshold: 128 * 1024,
            dictionary_threshold: 0,
            debug_audit_jsonl: None,
            debug_audit_attribution: crate::core::debug_audit::DebugAuditAttribution::default(),
        };

        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(TestPlugin {
            name: "test",
            priority: 10,
        })];

        let metrics = MetricsCollector::new(MetricsConfig {
            enabled: false,
            enable_module_timing: false,
            enable_plugin_stats: false,
            enable_error_logging: false,
            max_error_logs: 100,
        });

        let mut pipeline = CompressionPipeline::new(pipeline_config, plugins, metrics);

        // >128KB to qualify for configured parallel threshold, but reorder=true should still force serial.
        let text = "error: sample line\n".repeat(20_000);
        let result = pipeline.compress_str(&text);
        assert!(result.is_ok());
        let output = result.unwrap();

        // Serial path reports line_count-based slice_count; parallel path currently reports 0.
        assert!(output.metadata.slice_count > 0);
    }

    /// 验证 reorder 关闭时并行阈值生效：低阈值（128KB）选中并行路径
    /// （`slice_count == 0`），高阈值（usize::MAX/2）保持串行路径（`slice_count > 0`）。
    #[test]
    fn test_parallel_threshold_respected_when_reorder_disabled() {
        let slicer_config = SlicerConfig::default();

        let dispatcher_config = DispatcherConfig {
            plugin_timeout_ms: 1000,
        };

        let dedup_config = DedupConfig {
            pattern_threshold: 2,
        ..Default::default()
        };

        let text = "error: sample line\n".repeat(20_000);

        let plugins_low: Vec<Box<dyn Plugin>> = vec![Box::new(TestPlugin {
            name: "test",
            priority: 10,
        })];
        let metrics_low = MetricsCollector::new(MetricsConfig {
            enabled: false,
            enable_module_timing: false,
            enable_plugin_stats: false,
            enable_error_logging: false,
            max_error_logs: 100,
        });
        let mut low_threshold_pipeline = CompressionPipeline::new(
            PipelineConfig {
                slicer_config: slicer_config.clone(),
                dispatcher_config: dispatcher_config.clone(),
                dedup_config: dedup_config.clone(),
                reorder_config: crate::core::log_reorderer::ReorderConfig::default(),
                stream_buffer_size: 4096,
                parallel_threshold: 128 * 1024,
                dictionary_threshold: 0,
                debug_audit_jsonl: None,
                debug_audit_attribution: crate::core::debug_audit::DebugAuditAttribution::default(),
            },
            plugins_low,
            metrics_low,
        );

        let low_result = low_threshold_pipeline.compress_str(&text).unwrap();
        assert_eq!(
            low_result.metadata.slice_count, 0,
            "low threshold should select parallel path"
        );

        let plugins_high: Vec<Box<dyn Plugin>> = vec![Box::new(TestPlugin {
            name: "test",
            priority: 10,
        })];
        let metrics_high = MetricsCollector::new(MetricsConfig {
            enabled: false,
            enable_module_timing: false,
            enable_plugin_stats: false,
            enable_error_logging: false,
            max_error_logs: 100,
        });
        let mut high_threshold_pipeline = CompressionPipeline::new(
            PipelineConfig {
                slicer_config,
                dispatcher_config,
                dedup_config,
                reorder_config: crate::core::log_reorderer::ReorderConfig::default(),
                stream_buffer_size: 4096,
                parallel_threshold: usize::MAX / 2,
                dictionary_threshold: 0,
                debug_audit_jsonl: None,
                debug_audit_attribution: crate::core::debug_audit::DebugAuditAttribution::default(),
            },
            plugins_high,
            metrics_high,
        );

        let high_result = high_threshold_pipeline.compress_str(&text).unwrap();
        assert!(
            high_result.metadata.slice_count > 0,
            "high threshold should keep serial path"
        );
    }

    /// 验证启用指标后，插件分派统计被采集：`dummy_plugin` 出现
    /// 正数 `detect_calls`/`compress_calls`，且 `plugin_dispatcher` 模块耗时非零。
    #[test]
    fn test_metrics_collects_plugin_dispatcher_stats() {
        let pipeline_config = PipelineConfig::default();
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(TestPlugin {
            name: "test",
            priority: 10,
        })];
        let metrics = MetricsCollector::new(MetricsConfig {
            enabled: true,
            enable_module_timing: true,
            enable_plugin_stats: true,
            enable_error_logging: true,
            max_error_logs: 100,
        });

        let mut pipeline = CompressionPipeline::new(pipeline_config, plugins, metrics);
        let _ = pipeline.compress_str("error: sample line\nwarning: sample");

        let snapshot = pipeline.get_metrics().snapshot();
        let plugin = snapshot.plugin_stats.get("dummy_plugin");
        assert!(plugin.is_some(), "expected dummy_plugin metrics");
        let plugin = plugin.unwrap();
        assert!(plugin.detect_calls > 0);
        assert!(plugin.compress_calls > 0);
        assert!(snapshot.module_timings.plugin_dispatcher > std::time::Duration::ZERO);
    }

    /// 验证短输入走透传兜底路径时，`dispatcher_passthrough` 指标被记录为
    /// `compress_calls`（直通单独口径）。P2-42：直通是健康路径，不再计入
    /// `fallback_count`——回退只统计真实降级/插件失败，故此处 fallback 应为 0。
    #[test]
    fn test_metrics_collects_passthrough_fallback_stats() {
        let pipeline_config = PipelineConfig::default();
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(TestPlugin {
            name: "test",
            priority: 10,
        })];
        let metrics = MetricsCollector::new(MetricsConfig {
            enabled: true,
            enable_module_timing: true,
            enable_plugin_stats: true,
            enable_error_logging: true,
            max_error_logs: 100,
        });

        let mut pipeline = CompressionPipeline::new(pipeline_config, plugins, metrics);
        let _ = pipeline.compress_str("short plain text");

        let snapshot = pipeline.get_metrics().snapshot();
        let passthrough = snapshot.plugin_stats.get("dispatcher_passthrough");
        assert!(passthrough.is_some(), "expected passthrough metrics");
        let passthrough = passthrough.unwrap();
        assert!(passthrough.compress_calls > 0);
        assert_eq!(
            passthrough.fallback_count, 0,
            "直通不应计入 fallback（P2-42），fallback={}",
            passthrough.fallback_count
        );
    }

    /// 验证 chunk unwrap 路径：以 `csv_header` 开头且含换行的整块输入
    /// 先经 `TestPlugin::unwrap` 展开为两行，再经切片与压缩后，
    /// 最终 Token 流中应包含 `unwrapped_line1`。
    #[test]
    fn test_unwrap_csv_chunk() {
        let pipeline_config = PipelineConfig::default();
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(TestPlugin {
            name: "test",
            priority: 10,
        })];
        let metrics = MetricsCollector::new(MetricsConfig {
            enabled: false,
            enable_module_timing: false,
            enable_plugin_stats: false,
            enable_error_logging: false,
            max_error_logs: 100,
        });

        let mut pipeline = CompressionPipeline::new(pipeline_config, plugins, metrics);
        // Provide a multi-line string that starts with "csv_header".
        // With chunk unwrapping, the entire string is passed to `unwrap`,
        // which matches the condition `starts_with("csv_header") && contains('\n')`.
        let input = "csv_header,value\nline1,value1\nline2,value2\n";
        let output = pipeline.compress_str(input).unwrap();

        // The TestPlugin unwraps it into "unwrapped_line1\nunwrapped_line2\n".
        // Then `compress` will slice it, and TestPlugin will compress it.
        // We just need to verify that "unwrapped_line1" is present in the final tokens.
        let mut all_text = String::new();
        for t in output.tokens {
            if let Token::Text(s) = t {
                all_text.push_str(&s);
            }
        }

        assert!(
            all_text.contains("unwrapped_line1"),
            "Chunk unwrapping failed, output text: {}",
            all_text
        );
    }
    /// 验证开启 debug audit JSONL 后，压缩事件写入审计文件：
    /// 断言原始/压缩尺寸与 Token 估算为正数、`attribution` 的
    /// client_app/model_family 透传正确、`plugin_chain` 含 `audit_test`
    /// 且 `plugin_effects` 记录了其调用次数。
    #[test]
    fn test_pipeline_audit_captures_plugin_attribution() {
        let audit_path = std::env::temp_dir().join(format!(
            "tokenslim-pipeline-audit-attribution-{}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&audit_path);

        let mut pipeline_config = PipelineConfig::default();
        pipeline_config.debug_audit_jsonl = Some(audit_path.clone());
        pipeline_config.debug_audit_attribution.client_app = Some("codex".to_string());
        pipeline_config.debug_audit_attribution.model_family = Some("openai".to_string());
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(TestPlugin {
            name: "audit_test",
            priority: 10,
        })];
        let metrics = MetricsCollector::new(MetricsConfig {
            enabled: false,
            enable_module_timing: false,
            enable_plugin_stats: false,
            enable_error_logging: false,
            max_error_logs: 100,
        });
        let mut pipeline = CompressionPipeline::new(pipeline_config, plugins, metrics);
        pipeline
            .compress_str("error: audit attribution test input\n")
            .unwrap();

        let content = std::fs::read_to_string(&audit_path).expect("audit file");
        let _ = std::fs::remove_file(&audit_path);
        let event: serde_json::Value = serde_json::from_str(content.trim()).expect("valid JSONL");
        assert_eq!(
            event["compression_metadata"]["original_size"],
            "error: audit attribution test input\n".len()
        );
        assert!(
            event["compression_metadata"]["compressed_size"]
                .as_u64()
                .expect("compressed size")
                > 0
        );
        assert!(
            event["compression_metadata"]["original_tokens"]
                .as_u64()
                .expect("original token estimate")
                > 0
        );
        assert!(
            event["compression_metadata"]["compressed_tokens"]
                .as_u64()
                .expect("compressed token estimate")
                > 0
        );
        assert_eq!(event["attribution"]["client_app"], "codex");
        assert_eq!(event["attribution"]["model_family"], "openai");
        assert!(event["plugin_chain"]
            .as_array()
            .expect("plugin chain")
            .iter()
            .any(|plugin| plugin["plugin_id"] == "audit_test"));
        assert!(event["plugin_effects"]
            .as_array()
            .expect("plugin effects")
            .iter()
            .any(|effect| effect["plugin_id"] == "audit_test"
                && effect["invocation_count"].as_u64().unwrap_or(0) >= 1));
        // schema v3: plugin_fallback 字段必须存在（此项 metrics 统计关闭时为 {}）
        assert!(event.as_object().unwrap().contains_key("plugin_fallback"));
    }

    /// 验证阶段 3 文档级定向：>2048 字节无皮文档（cargo 主体 + gcc 尾部混块）
    /// 走大输入串行路径，`document_category` 识别出 Cargo 作为 sticky 初始种子，
    /// 将 cargo 主体定向压缩；尾部 gcc 内容转向时 sticky 自动失效、由 gcc 接管并完整保留。
    #[test]
    fn test_doc_seed_directs_large_no_skin_document() {
        use crate::core::stream_reader::StreamReader;

        let analyzer = ContentAnalyzer::new();
        let mut pipeline = CompressionPipeline::new(
            PipelineConfig::default(),
            crate::cli::get_plugins(),
            MetricsCollector::new(MetricsConfig {
                enabled: false,
                enable_module_timing: false,
                enable_plugin_stats: false,
                enable_error_logging: false,
                max_error_logs: 100,
            }),
        );

        // P3-202：混块样本物理化为 classifier_holdout 物理样本（红线：禁止手写 mock）。
        // 内容 = cargo build 首行锚点 + 40 组重复 Compiling 行（cargo 主体）+ gcc 尾部混块。
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("classifier_holdout/structured/mixed/test_doc_seed_mixed_cargo_gcc.log");
        let text = std::fs::read_to_string(&path).expect("读取 doc seed 混块样本失败");
        assert!(
            text.len() > 2048,
            "input must exceed whole-input threshold: {}",
            text.len()
        );

        // 文档级语义识别：cargo 主体主导 → Cargo；无皮（非剥皮类别）→ 走大输入路径
        assert_eq!(analyzer.document_category(&text), Some(Category::Cargo));
        assert!(analyzer.document_skin(&text).is_none());

        // 大输入走串行路径，文档级定向种子把 cargo 主体定向到 rust_go
        let reader = StreamReader::from_str(&text);
        let out = pipeline.compress_stream(&reader).unwrap();

        let rendered: String = out
            .tokens
            .iter()
            .filter_map(|t| match t {
                Token::Text(s) => Some(s.to_string()),
                _ => None,
            })
            .collect();

        // 命令锚点保持在首行（法则 0）
        assert!(
            rendered.starts_with("cargo build --release"),
            "command anchor must stay first line, got: {}",
            rendered.chars().take(80).collect::<String>()
        );
        // cargo 主体被定向压缩（sticky 种子生效）
        assert!(
            rendered.contains("[CARGO]"),
            "cargo body must be directed-compressed"
        );
        // gcc 尾部内容转向时 sticky 失效、被完整保留
        assert!(
            rendered.contains("warning: implicit declaration of function 'foo'"),
            "gcc tail must survive content switch"
        );
        // 重复 cargo 编译行被大幅压缩
        assert!(
            out.metadata.compression_ratio < 0.5,
            "repeated cargo lines must be strongly compressed: {}",
            out.metadata.compression_ratio
        );
    }

    /// 4a/4c 协同验证：小输入整块路径下，大尺寸纯 TOML 配置应被 `toml_ini` 认领压缩
    /// （归一化标记 `$TOML|` 出现，而不是被 generic_text/smart_path 原文兜底）。
    /// 回归用 `classifier_holdout/structured/mixed/` 里的物理样本（红线：禁止手写 mock）。
    #[test]
    fn test_whole_input_pure_toml_routed_to_toml_ini() {
        use crate::core::stream_reader::StreamReader;

        let mut pipeline = CompressionPipeline::new(
            PipelineConfig::default(),
            crate::cli::get_plugins(),
            MetricsCollector::new(MetricsConfig {
                enabled: false,
                enable_module_timing: false,
                enable_plugin_stats: false,
                enable_error_logging: false,
                max_error_logs: 100,
            }),
        );

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("classifier_holdout/structured/mixed/case_002_large_pure_toml.conf");
        let text = std::fs::read_to_string(&path).expect("读取纯 TOML 样本失败");
        assert!(text.len() > 0 && text.len() < 2048, "应为小输入整块路径");

        let reader = StreamReader::from_str(&text);
        let out = pipeline.compress_stream(&reader).unwrap();
        let rendered: String = out
            .tokens
            .iter()
            .filter_map(|t| match t {
                Token::Text(s) => Some(s.to_string()),
                _ => None,
            })
            .collect();

        // 归一化成功则带 `$TOML|` 标记；ROI 门控可能退回原文，但绝不应退化为疯狂扩展。
        assert!(
            rendered.contains("$TOML|") || rendered.trim() == text.trim(),
            "纯 TOML 应被 toml_ini 认领：要么归一化($TOML|)，要么 ROI 门控保原文，got head={}",
            rendered.chars().take(60).collect::<String>()
        );
        assert!(
            out.metadata.compression_ratio < 0.99,
            "纯 TOML 不应被原文放大或几乎不压缩，ratio={}",
            out.metadata.compression_ratio
        );
    }

    /// Phase 5 G-1b（架构级保护硬化）：通用自然语言文本经压缩管线不应被专有插件改写。
    ///
    /// [classifier_holdout/bayesian/generic_text/] 下的 memo/update 是「无命令锚点、无强
    /// 结构」的英文自然语言。classify() 层因贝叶斯无负信号，常被弱泛化词微弱抢成
    /// web_log/git_diff（实测 0.278 / 0.131），但**管道层的 0.40 门槛（bayesian_fallback /
    /// document_category）已把这些低置信路由挡在全量 detect 之外**，使其落到 generic_text
    /// 兜底、原文保真。本测试固化该保护：断言 ratio 不显著压缩（未被专有插件归一化
    /// 改写）且原文关键句完整保留。
    #[test]
    fn test_generic_text_natural_language_not_specialized_rewrite() {
        use crate::core::stream_reader::StreamReader;

        let mut pipeline = CompressionPipeline::new(
            PipelineConfig::default(),
            crate::cli::get_plugins(),
            MetricsCollector::new(MetricsConfig {
                enabled: false,
                enable_module_timing: false,
                enable_plugin_stats: false,
                enable_error_logging: false,
                max_error_logs: 100,
            }),
        );

        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("classifier_holdout/bayesian/generic_text");
        let cases = [
            ("case_001_memo.log", "quarterly sync was productive"),
            ("case_002_update.log", "latest translations branch"),
        ];
        for (fname, key) in cases {
            let path = base.join(fname);
            let text = std::fs::read_to_string(&path).expect("读取通用文本样本失败");
            let reader = StreamReader::from_str(&text);
            let out = pipeline.compress_stream(&reader).unwrap();
            let rendered: String = out
                .tokens
                .iter()
                .filter_map(|t| match t {
                    Token::Text(s) => Some(s.to_string()),
                    _ => None,
                })
                .collect();
            // 通用文本应走 generic_text/兜底原文保真：ratio 不显著下降（未被专有插件
            // 做名词归一/字典化改写），关键句完整保留。
            assert!(
                out.metadata.compression_ratio > 0.85,
                "{fname} 通用文本不应被专有插件改写，ratio={}",
                out.metadata.compression_ratio
            );
            assert!(
                rendered.contains(key),
                "{fname} 通用文本关键句应原文保留，got head={}",
                rendered.chars().take(80).collect::<String>()
            );
        }
    }

    /// P1-04 回归：串行路径接入共享跨切片去重——60 条相同行（行模式，每行
    /// 一个切片）在串行路径（≥2048B 且 < parallel_threshold）应产出 `$M`
    /// 去重引用，而非每行重复原文（旧实现串行路径完全没有跨切片去重，
    /// 与并行路径行为分叉）。
    #[test]
    fn serial_path_dedups_identical_lines_across_slices() {
        let line = "2026-01-01T10:00:00Z INFO auth-service session established for user=alice\n";
        let data = line.repeat(60);
        assert!(
            data.len() > 2048,
            "夹具必须超过整块阈值（2048B）以进入串行流式路径"
        );

        let slicer_config = crate::core::text_slicer::SlicerConfig {
            mode: crate::core::text_slicer::SliceMode::Line,
            ..Default::default()
        };
        let dispatcher_config = DispatcherConfig {
            plugin_timeout_ms: 1000,
        };
        let dedup_config = DedupConfig {
            pattern_threshold: 2,
        ..Default::default()
        };
        let pipeline_config = PipelineConfig {
            slicer_config,
            dispatcher_config,
            dedup_config,
            reorder_config: crate::core::log_reorderer::ReorderConfig::default(),
            stream_buffer_size: 4096,
            parallel_threshold: 1024 * 1024,
            dictionary_threshold: 0,
            debug_audit_jsonl: None,
            debug_audit_attribution: crate::core::debug_audit::DebugAuditAttribution::default(),
        };

        let mut pipeline = CompressionPipeline::new(
            pipeline_config,
            vec![],
            MetricsCollector::new(MetricsConfig::default()),
        );
        let output = pipeline.compress_str(&data).unwrap();

        let mut dedup_refs = 0;
        for token in &output.tokens {
            if let Token::DictRef(d) = token {
                if d.starts_with("$M") {
                    dedup_refs += 1;
                }
            }
        }
        assert!(
            dedup_refs >= 58,
            "60 条重复行应产出 ≥58 个 $M 跨切片去重引用（P1-04），实际 {}",
            dedup_refs
        );
    }
}
