//! compression pipeline 方法实现

use super::types::*;
use crate::core::compression::{CompressionMetadata, CompressionOutput, Token};
use crate::core::content_analyzer::{AnalyzerConfig, ContentAnalyzer};
use crate::core::dedup_engine::{DedupEngine, SharedDedupEngine};
use crate::core::dictionary_engine::DictionaryEngine;
use crate::core::log_reorderer::LogReorderer;
use crate::core::metrics::MetricsCollector;
use crate::core::stream_reader::{SliceInput, StreamReader};
use crate::core::text_slicer::{Slice, SliceMode, TextSlicer};
use bumpalo::Bump;
use rayon::prelude::*;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const E_PIPELINE_ANALYZER_INIT: &str = "E_PIPELINE_ANALYZER_INIT";
const META_PARSE_TIER: &str = "parse_tier";
const META_PARSE_REASON: &str = "parse_reason";
const METRICS_DISPATCHER_PASSTHROUGH: &str = "dispatcher_passthrough";

#[derive(Default)]
struct DispatchMetricsDelta {
    plugin_detect: HashMap<String, (usize, Duration)>,
    plugin_compress: HashMap<String, (usize, Duration)>,
    plugin_fallback: HashMap<String, usize>,
    errors: Vec<(Option<String>, String, String, Option<u64>)>,
}

impl CompressionPipeline {
    pub fn new(
        config: PipelineConfig,
        plugins: Vec<Box<dyn crate::core::plugin_dispatcher::Plugin>>,
        metrics: MetricsCollector,
    ) -> Self {
        let dict_manager = Arc::new(crate::core::dictionary_manager::DictionaryManager::new());
        let dispatcher = crate::core::plugin_dispatcher::PluginDispatcher::new(
            plugins,
            config.dispatcher_config.clone(),
            crate::core::error_isolation::SafeExecutorConfig::default(),
            dict_manager.clone(),
        );

        Self {
            config: config.clone(),
            dispatcher,
            analyzer: ContentAnalyzer::new(AnalyzerConfig::default())
                .expect(E_PIPELINE_ANALYZER_INIT),
            slicer: TextSlicer::with_dict_manager(
                config.slicer_config.clone(),
                dict_manager.clone(),
            ),
            dict_engine: DictionaryEngine::with_manager(dict_manager.clone()),
            dedup_engine: Arc::new(SharedDedupEngine::new(config.dedup_config.clone())),
            metrics,
            processing_context: crate::core::compression_context::CompressionContext::new(),
            dict_manager,
            log_reorderer: LogReorderer::new(config.reorder_config.clone()),
        }
    }

    pub fn compress_str(&mut self, text: &str) -> Result<CompressionOutput, PipelineError> {
        let reader = StreamReader::from_str(text);
        self.compress_stream(&reader)
    }

    pub fn compress_file(
        &mut self,
        path: &std::path::Path,
    ) -> Result<CompressionOutput, PipelineError> {
        let reader = StreamReader::from_file(path)?;
        self.compress_stream(&reader)
    }

    pub fn compress_stream<'a>(
        &mut self,
        reader: &StreamReader<'a>,
    ) -> Result<CompressionOutput, PipelineError> {
        let original_size = reader.size();
        self.metrics.set_input_size(original_size);
        self.metrics.start_module("compression_pipeline");

        let parallel_threshold = self.config.parallel_threshold.max(1);
        let result = if original_size >= parallel_threshold && !self.config.reorder_config.enabled {
            self.compress_stream_parallel(reader)
        } else {
            self.compress_stream_serial(reader)
        };

        self.metrics.end_module("compression_pipeline");
        result
    }

    fn compress_stream_serial<'a>(
        &mut self,
        reader: &StreamReader<'a>,
    ) -> Result<CompressionOutput, PipelineError> {
        let start_time = Instant::now();
        let line_mode = matches!(self.config.slicer_config.mode, SliceMode::Line);
        self.processing_context.reset_timestamp();
        let arena = Bump::new();
        let mut tokens = Vec::new();
        let mut line_count = 0usize;
        let mut metrics_delta = DispatchMetricsDelta::default();

        self.metrics.start_module("plugin_dispatcher");

        // split_for_parallel with 1 worker returns chunks of data (capped at 5MB) ending on semantic boundaries.
        let chunks = reader.split_for_parallel(1);
        let mut processed_lines = 0usize;

        for chunk in chunks {
            let unwrapped = self.dispatcher.unwrap_recursive(chunk.raw.as_ref());
            let unwrapped_reader =
                crate::core::stream_reader::StreamReader::from_str_owned(unwrapped.into_owned());

            if self.config.reorder_config.enabled {
                for input in unwrapped_reader.iter_lines() {
                    line_count += 1;
                    let flushed = self.log_reorderer.process_line(input.raw.into_owned());
                    for line in flushed {
                        processed_lines += 1;
                        let input_line = SliceInput {
                            raw: Cow::Owned(line),
                            offset: chunk.offset + input.offset,
                            line_number: chunk.line_number + processed_lines - 1,
                            file_metadata: chunk.file_metadata,
                        };

                        let mut produced_slices = Vec::new();
                        self.slicer
                            .push_slices_by_mode(&input_line, &mut produced_slices);

                        for slice in produced_slices {
                            let dispatch_started = Instant::now();
                            let compress_result = self.compress_one_slice(&slice, &arena);
                            let dispatch_duration = dispatch_started.elapsed();
                            Self::collect_dispatch_metrics_delta(
                                &mut metrics_delta,
                                &compress_result,
                                dispatch_duration,
                                Some(slice.id),
                            );
                            tokens
                                .extend(compress_result.tokens.into_iter().map(|t| t.into_owned()));
                        }
                    }
                }
            } else {
                for input in unwrapped_reader.iter_lines() {
                    line_count += 1;
                    let has_line_break =
                        input.offset + input.raw.as_ref().len() < unwrapped_reader.size();
                    let input_line = SliceInput {
                        raw: input.raw,
                        offset: chunk.offset + input.offset,
                        line_number: chunk.line_number + input.line_number - 1,
                        file_metadata: chunk.file_metadata,
                    };
                    let mut produced_slices = Vec::new();
                    self.slicer
                        .push_slices_by_mode(&input_line, &mut produced_slices);

                    for slice in produced_slices {
                        let dispatch_started = Instant::now();
                        let compress_result = self.compress_one_slice(&slice, &arena);
                        let dispatch_duration = dispatch_started.elapsed();
                        Self::collect_dispatch_metrics_delta(
                            &mut metrics_delta,
                            &compress_result,
                            dispatch_duration,
                            Some(slice.id),
                        );
                        tokens.extend(compress_result.tokens.into_iter().map(|t| t.into_owned()));
                    }

                    if line_mode && has_line_break {
                        tokens.push(Token::Text(Cow::Borrowed("\n")));
                    }
                }
            }
        }

        if self.config.reorder_config.enabled {
            let final_flushed = self.log_reorderer.flush();
            for line in final_flushed {
                processed_lines += 1;
                let input_line = SliceInput {
                    raw: Cow::Owned(line),
                    offset: 0,
                    line_number: processed_lines,
                    file_metadata: None,
                };

                let mut produced_slices = Vec::new();
                self.slicer
                    .push_slices_by_mode(&input_line, &mut produced_slices);

                for slice in produced_slices {
                    let dispatch_started = Instant::now();
                    let compress_result = self.compress_one_slice(&slice, &arena);
                    let dispatch_duration = dispatch_started.elapsed();
                    Self::collect_dispatch_metrics_delta(
                        &mut metrics_delta,
                        &compress_result,
                        dispatch_duration,
                        Some(slice.id),
                    );
                    tokens.extend(compress_result.tokens.into_iter().map(|t| t.into_owned()));
                }
            }
        }

        let final_slices = self.slicer.flush();
        for slice in final_slices {
            let dispatch_started = Instant::now();
            let res = self.compress_one_slice(&slice, &arena);
            let dispatch_duration = dispatch_started.elapsed();
            Self::collect_dispatch_metrics_delta(
                &mut metrics_delta,
                &res,
                dispatch_duration,
                Some(slice.id),
            );
            tokens.extend(res.tokens.into_iter().map(|t| t.into_owned()));
        }
        self.metrics.end_module("plugin_dispatcher");
        self.apply_dispatch_metrics_delta(metrics_delta);

        let final_tokens = Self::merge_adjacent_tokens_static(tokens);
        let output_size = final_tokens.iter().map(|t| t.estimated_size()).sum();
        self.metrics.set_output_size(output_size);
        self.metrics.add_slice_count(line_count);

        Ok(CompressionOutput {
            tokens: final_tokens,
            dictionary: self.dict_engine.snapshot(),
            metadata: CompressionMetadata {
                original_size: reader.size(),
                slice_count: line_count,
                processing_time_ms: start_time.elapsed().as_millis(),
                base_timestamp: self
                    .processing_context
                    .base_timestamp()
                    .map(|ts| ts.to_rfc3339()),
                ..Default::default()
            },
        })
    }

    fn compress_stream_parallel<'a>(
        &mut self,
        reader: &StreamReader<'a>,
    ) -> Result<CompressionOutput, PipelineError> {
        let start_time = Instant::now();
        let line_mode = matches!(self.config.slicer_config.mode, SliceMode::Line);
        let worker_count = rayon::current_num_threads().max(1);
        let chunks = reader.split_for_parallel(worker_count);

        if chunks.is_empty() {
            return Ok(CompressionOutput {
                tokens: Vec::new(),
                dictionary: self.dict_engine.snapshot(),
                metadata: CompressionMetadata {
                    original_size: reader.size(),
                    slice_count: 0,
                    processing_time_ms: start_time.elapsed().as_millis(),
                    ..Default::default()
                },
            });
        }

        let shared_dedup = &self.dedup_engine;
        let dispatcher = &self.dispatcher;
        let dict_manager = &self.dict_manager;
        let dedup_config = &self.config.dedup_config;
        self.metrics.start_module("plugin_dispatcher");

        let results: Vec<(
            Vec<Token<'static>>,
            Option<chrono::DateTime<chrono::Utc>>,
            DispatchMetricsDelta,
        )> = chunks
            .into_par_iter()
            .map(|chunk_input| {
                let arena = Bump::new();
                let mut batch_tokens: Vec<Token> = Vec::with_capacity(16384);
                let mut local_dict = DictionaryEngine::with_manager(dict_manager.clone());
                let mut local_dedup = DedupEngine::new(dedup_config.clone());
                let mut local_dedup_cache = HashMap::with_capacity(16384);
                let mut local_slicer = TextSlicer::with_dict_manager(
                    self.config.slicer_config.clone(),
                    dict_manager.clone(),
                );
                let local_analyzer = ContentAnalyzer::new(self.config.analyzer_config.clone())
                    .expect(E_PIPELINE_ANALYZER_INIT);
                let mut sticky_plugin = None;
                let mut local_metrics_delta = DispatchMetricsDelta::default();

                let mut local_context = crate::core::compression_context::CompressionContext::new();
                let chunk_start_line = chunk_input.line_number;
                let chunk_base_offset = chunk_input.offset;

                // Unwrap the entire chunk first
                let unwrapped_chunk = dispatcher.unwrap_recursive(chunk_input.raw.as_ref());

                for (line_idx, line_with_break) in unwrapped_chunk.split_inclusive('\n').enumerate()
                {
                    let has_line_break = line_with_break.ends_with('\n');
                    let line = if has_line_break {
                        &line_with_break[..line_with_break.len().saturating_sub(1)]
                    } else {
                        line_with_break
                    };
                    let line = line.strip_suffix('\r').unwrap_or(line);

                    let stable_line = &*arena.alloc_str(line);
                    let text_to_check = if has_line_break {
                        bumpalo::format!(in &arena, "{}\n", stable_line).into_bump_str()
                    } else {
                        stable_line
                    };

                    if let Some(dedup) = shared_dedup.dedup_cross_slice_with_local(
                        text_to_check,
                        &mut local_dict,
                        &arena,
                        &mut local_dedup_cache,
                    ) {
                        batch_tokens.extend(dedup.tokens);
                        continue;
                    }

                    let input_line = SliceInput {
                        raw: Cow::Borrowed(stable_line),
                        offset: chunk_base_offset,
                        line_number: chunk_start_line + line_idx,
                        file_metadata: chunk_input.file_metadata,
                    };

                    let mut produced_slices = Vec::new();
                    local_slicer.push_slices_by_mode(&input_line, &mut produced_slices);

                    for slice in produced_slices {
                        let stable_slice = arena.alloc(slice);
                        let _analysis = local_analyzer.analyze(stable_slice);
                        let dispatch_started = Instant::now();
                        let res = dispatcher.dispatch_slice_sticky(
                            stable_slice,
                            &mut local_dict,
                            &mut local_dedup,
                            &arena,
                            &mut local_context,
                            &mut sticky_plugin,
                        );
                        let dispatch_duration = dispatch_started.elapsed();
                        Self::collect_dispatch_metrics_delta(
                            &mut local_metrics_delta,
                            &res,
                            dispatch_duration,
                            Some(stable_slice.id),
                        );

                        for token in res.tokens {
                            batch_tokens.push(token);
                        }
                    }

                    if line_mode && has_line_break {
                        batch_tokens.push(Token::Text(Cow::Borrowed("\n")));
                    }
                }

                for slice in local_slicer.flush() {
                    let stable_slice = arena.alloc(slice);
                    let _analysis = local_analyzer.analyze(stable_slice);
                    let dispatch_started = Instant::now();
                    let res = dispatcher.dispatch_slice_sticky(
                        stable_slice,
                        &mut local_dict,
                        &mut local_dedup,
                        &arena,
                        &mut local_context,
                        &mut sticky_plugin,
                    );
                    let dispatch_duration = dispatch_started.elapsed();
                    Self::collect_dispatch_metrics_delta(
                        &mut local_metrics_delta,
                        &res,
                        dispatch_duration,
                        Some(stable_slice.id),
                    );
                    for token in res.tokens {
                        batch_tokens.push(token);
                    }
                }

                let local_fused = Self::fuse_tokens_local(batch_tokens);
                (
                    local_fused.into_iter().map(|t| t.into_owned()).collect(),
                    local_context.base_timestamp(),
                    local_metrics_delta,
                )
            })
            .collect();

        let global_base_ts = results.iter().find_map(|(_, base_ts, _)| *base_ts);
        for (_, _, delta) in &results {
            self.apply_dispatch_metrics_delta_ref(delta);
        }
        self.metrics.end_module("plugin_dispatcher");

        let final_tokens = Self::merge_adjacent_tokens_static(
            results
                .into_iter()
                .flat_map(|(tokens, _, _)| tokens)
                .collect(),
        );
        let output_size = final_tokens.iter().map(|t| t.estimated_size()).sum();
        self.metrics.set_output_size(output_size);

        Ok(CompressionOutput {
            tokens: final_tokens,
            dictionary: self.dict_engine.snapshot(),
            metadata: CompressionMetadata {
                original_size: reader.size(),
                slice_count: 0,
                processing_time_ms: start_time.elapsed().as_millis(),
                base_timestamp: global_base_ts.map(|ts| ts.to_rfc3339()),
                ..Default::default()
            },
        })
    }

    fn fuse_tokens_local<'a>(tokens: Vec<Token<'a>>) -> Vec<Token<'a>> {
        if tokens.is_empty() {
            return vec![];
        }
        let mut fused = Vec::with_capacity(128);
        let mut current_text = String::with_capacity(64_000);

        for token in tokens {
            match token {
                Token::Text(s) => current_text.push_str(s.as_ref()),
                _ => {
                    if !current_text.is_empty() {
                        fused.push(Token::Text(Cow::Owned(std::mem::take(&mut current_text))));
                        current_text.reserve(64_000);
                    }
                    fused.push(token);
                }
            }
        }
        if !current_text.is_empty() {
            fused.push(Token::Text(Cow::Owned(current_text)));
        }
        fused
    }

    fn merge_adjacent_tokens_static(tokens: Vec<Token<'static>>) -> Vec<Token<'static>> {
        if tokens.is_empty() {
            return vec![];
        }
        let mut merged = Vec::with_capacity(tokens.len());
        let mut it = tokens.into_iter();
        if let Some(mut current) = it.next() {
            for next in it {
                if let (Token::Text(ref mut last_s), Token::Text(s)) = (&mut current, &next) {
                    let mut owned = last_s.to_string();
                    owned.push_str(s.as_ref());
                    *last_s = Cow::Owned(owned);
                } else {
                    merged.push(std::mem::replace(&mut current, next));
                }
            }
            merged.push(current);
        }
        merged
    }

    fn compress_one_slice<'a>(
        &mut self,
        slice: &'a Slice<'a>,
        arena: &'a Bump,
    ) -> crate::core::plugin_dispatcher::CompressResult<'a> {
        let mut dummy = None;
        let mut local_dedup = DedupEngine::new(self.config.dedup_config.clone());
        self.dispatcher.dispatch_slice_sticky(
            slice,
            &mut self.dict_engine,
            &mut local_dedup,
            arena,
            &mut self.processing_context,
            &mut dummy,
        )
    }

    pub fn get_metrics(&self) -> &MetricsCollector {
        &self.metrics
    }

    fn collect_dispatch_metrics_delta(
        delta: &mut DispatchMetricsDelta,
        result: &crate::core::plugin_dispatcher::CompressResult<'_>,
        duration: Duration,
        slice_id: Option<u64>,
    ) {
        let plugin_key = result
            .plugin_name
            .map(|n| n.to_string())
            .unwrap_or_else(|| METRICS_DISPATCHER_PASSTHROUGH.to_string());

        if result.plugin_name.is_some() {
            let entry = delta
                .plugin_detect
                .entry(plugin_key.clone())
                .or_insert((0, Duration::ZERO));
            entry.0 += 1;
            entry.1 += duration;
        }

        let entry = delta
            .plugin_compress
            .entry(plugin_key.clone())
            .or_insert((0, Duration::ZERO));
        entry.0 += 1;
        entry.1 += duration;

        let parse_tier = result
            .metadata
            .as_ref()
            .and_then(|m| m.get(META_PARSE_TIER))
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        let parse_reason = result
            .metadata
            .as_ref()
            .and_then(|m| m.get(META_PARSE_REASON))
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        if parse_tier != "full" {
            *delta.plugin_fallback.entry(plugin_key.clone()).or_insert(0) += 1;
        }

        if parse_tier == "degraded" || parse_reason == "plugin_failed" {
            delta.errors.push((
                result.plugin_name.map(|s| s.to_string()),
                "dispatcher_degraded".to_string(),
                format!("parse_tier={parse_tier}, parse_reason={parse_reason}"),
                slice_id,
            ));
        }
    }

    fn apply_dispatch_metrics_delta_ref(&mut self, delta: &DispatchMetricsDelta) {
        for (plugin, (calls, total_duration)) in &delta.plugin_detect {
            self.metrics
                .record_plugin_detect_batch(plugin, *calls, *total_duration);
        }
        for (plugin, (calls, total_duration)) in &delta.plugin_compress {
            self.metrics
                .record_plugin_compress_batch(plugin, *calls, *total_duration);
        }
        for (plugin, fallback_count) in &delta.plugin_fallback {
            self.metrics.inc_plugin_fallback_by(plugin, *fallback_count);
        }
        for (plugin, error_type, message, slice_id) in &delta.errors {
            self.metrics.log_plugin_error(
                "plugin_dispatcher",
                plugin.as_deref(),
                error_type,
                message,
                *slice_id,
            );
        }
    }

    fn apply_dispatch_metrics_delta(&mut self, delta: DispatchMetricsDelta) {
        self.apply_dispatch_metrics_delta_ref(&delta);
    }
}
