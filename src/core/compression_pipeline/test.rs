//! compression pipeline 测试模块
//!
//! # 测试概述
//!
//! 本模块包含 compression pipeline 模块的单元测试 and 集成测试。
//! 测试覆盖了主要功能 and 边界情况。

#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
    use crate::core::content_analyzer::AnalyzerConfig;
    use crate::core::dedup_engine::DedupConfig;
    use crate::core::dictionary_engine::Dictionary;
    use crate::core::metrics::MetricsCollector;
    use crate::core::metrics::MetricsConfig;
    use crate::core::plugin_dispatcher::{CompressResult, DispatcherConfig, Plugin};
    use crate::core::text_slicer::{Slice, SlicerConfig};
    use bumpalo::Bump;
    use std::borrow::Cow;

    // 测试插件实现
    struct TestPlugin {
        name: &'static str,
        priority: u8,
    }

    impl Plugin for TestPlugin {
        fn name(&self) -> &'static str {
            self.name
        }

        fn priority(&self) -> u8 {
            self.priority
        }

        fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
            Some(0.9)
        }

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

        fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
            compressed.to_string()
        }

        fn unwrap(&self, text: &str) -> Option<String> {
            if text.starts_with("csv_header") && text.contains('\n') {
                // Return a mocked unwrapped multi-line string
                Some("unwrapped_line1\nunwrapped_line2\n".to_string())
            } else {
                None
            }
        }
    }

    #[test]
    fn test_new() {
        let slicer_config = SlicerConfig::default();

        let analyzer_config = AnalyzerConfig {
            enable_rules: true,
            rules: vec![],
            fallback_type: crate::core::text_slicer::SliceType::Unknown,
            confidence_threshold: 0.5,
            enable_script_detection: false,
            enable_vocabulary: false,
            vocabulary: None,
        };

        let dispatcher_config = DispatcherConfig {
            fallback_plugin: "test".to_string(),
            plugin_timeout_ms: 1000,
        };

        let dedup_config = DedupConfig {
            line_threshold: 2,
            stack_frame_threshold: 2,
            path_threshold: 2,
            pattern_threshold: 2,
            fuzzy_threshold: 0.9,
        };

        let pipeline_config = PipelineConfig {
            slicer_config,
            analyzer_config,
            dispatcher_config,
            dedup_config,
            reorder_config: crate::core::log_reorderer::ReorderConfig::default(),
            stream_buffer_size: 4096,
            parallel_threshold: 1024 * 1024,
            stream_mmap_threshold: None,
            dictionary_threshold: 0,
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

    #[test]
    fn test_compress_str() {
        let slicer_config = SlicerConfig::default();

        let analyzer_config = AnalyzerConfig {
            enable_rules: true,
            rules: vec![],
            fallback_type: crate::core::text_slicer::SliceType::Unknown,
            confidence_threshold: 0.5,
            enable_script_detection: false,
            enable_vocabulary: false,
            vocabulary: None,
        };

        let dispatcher_config = DispatcherConfig {
            fallback_plugin: "test".to_string(),
            plugin_timeout_ms: 1000,
        };

        let dedup_config = DedupConfig {
            line_threshold: 2,
            stack_frame_threshold: 2,
            path_threshold: 2,
            pattern_threshold: 2,
            fuzzy_threshold: 0.9,
        };

        let pipeline_config = PipelineConfig {
            slicer_config,
            analyzer_config,
            dispatcher_config,
            dedup_config,
            reorder_config: crate::core::log_reorderer::ReorderConfig::default(),
            stream_buffer_size: 4096,
            parallel_threshold: 1024 * 1024,
            stream_mmap_threshold: None,
            dictionary_threshold: 0,
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

    #[test]
    fn test_reorder_enabled_forces_serial_path_even_for_large_input() {
        let slicer_config = SlicerConfig::default();

        let analyzer_config = AnalyzerConfig {
            enable_rules: true,
            rules: vec![],
            fallback_type: crate::core::text_slicer::SliceType::Unknown,
            confidence_threshold: 0.5,
            enable_script_detection: false,
            enable_vocabulary: false,
            vocabulary: None,
        };

        let dispatcher_config = DispatcherConfig {
            fallback_plugin: "test".to_string(),
            plugin_timeout_ms: 1000,
        };

        let dedup_config = DedupConfig {
            line_threshold: 2,
            stack_frame_threshold: 2,
            path_threshold: 2,
            pattern_threshold: 2,
            fuzzy_threshold: 0.9,
        };

        let pipeline_config = PipelineConfig {
            slicer_config,
            analyzer_config,
            dispatcher_config,
            dedup_config,
            reorder_config: crate::core::log_reorderer::ReorderConfig {
                enabled: true,
                ..Default::default()
            },
            stream_buffer_size: 4096,
            parallel_threshold: 128 * 1024,
            stream_mmap_threshold: None,
            dictionary_threshold: 0,
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

    #[test]
    fn test_parallel_threshold_respected_when_reorder_disabled() {
        let slicer_config = SlicerConfig::default();

        let analyzer_config = AnalyzerConfig {
            enable_rules: true,
            rules: vec![],
            fallback_type: crate::core::text_slicer::SliceType::Unknown,
            confidence_threshold: 0.5,
            enable_script_detection: false,
            enable_vocabulary: false,
            vocabulary: None,
        };

        let dispatcher_config = DispatcherConfig {
            fallback_plugin: "test".to_string(),
            plugin_timeout_ms: 1000,
        };

        let dedup_config = DedupConfig {
            line_threshold: 2,
            stack_frame_threshold: 2,
            path_threshold: 2,
            pattern_threshold: 2,
            fuzzy_threshold: 0.9,
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
                analyzer_config: analyzer_config.clone(),
                dispatcher_config: dispatcher_config.clone(),
                dedup_config: dedup_config.clone(),
                reorder_config: crate::core::log_reorderer::ReorderConfig::default(),
                stream_buffer_size: 4096,
                parallel_threshold: 128 * 1024,
                stream_mmap_threshold: None,
                dictionary_threshold: 0,
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
                analyzer_config,
                dispatcher_config,
                dedup_config,
                reorder_config: crate::core::log_reorderer::ReorderConfig::default(),
                stream_buffer_size: 4096,
                parallel_threshold: usize::MAX / 2,
                stream_mmap_threshold: None,
                dictionary_threshold: 0,
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
        assert!(passthrough.fallback_count > 0);
    }

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
}
