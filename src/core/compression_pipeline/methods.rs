//! compression pipeline 方法实现

use super::types::*;
use crate::core::compression::{CompressionMetadata, CompressionOutput, Token};
use crate::core::content_analyzer::ContentAnalyzer;
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

const META_PARSE_TIER: &str = "parse_tier";
const META_PARSE_REASON: &str = "parse_reason";
const METRICS_DISPATCHER_PASSTHROUGH: &str = "dispatcher_passthrough";

/// 小输入整块处理的字节上限。交互式命令输出（git status、cargo 报错、ls 等）通常远小于
/// 该值：段落模式在空行处把 error/Usage/help 切碎成单行切片，detect 采样不到跨行综合信号
/// 而落到 generic_text 兜底。低于该上限的输入改走整块一次分发（见 [`CompressionPipeline::compress_whole_input`]）。
const SMALL_INPUT_WHOLE_THRESHOLD: usize = 2048;

#[derive(Default)]
struct DispatchMetricsDelta {
    plugin_detect: HashMap<String, (usize, Duration)>,
    plugin_compress: HashMap<String, (usize, Duration)>,
    plugin_fallback: HashMap<String, usize>,
    errors: Vec<(Option<String>, String, String, Option<u64>)>,
}

impl CompressionPipeline {
    /// 构造压缩流水线实例，组装并持有运行压缩所需的全部子系统。
    ///
    /// 依次构造插件分发器（PluginDispatcher）、内容分析器（ContentAnalyzer）、
    /// 文本切片器（TextSlicer，挂载共享字典管理器）、字典引擎（DictionaryEngine）、
    /// 去重引擎（SharedDedupEngine）、压缩上下文、字典管理器与日志重排器（LogReorderer），
    /// 各子系统的配置分别取 `config` 中对应的子配置项。
    ///
    /// # 参数
    /// - `config`：流水线总配置，含分发器、切片器、去重、重排等子配置。
    /// - `plugins`：插件实现列表，交由分发器按需调度。
    /// - `metrics`：调用方传入的指标采集器，用于记录输入/输出规模与耗时。
    ///
    /// # 注意
    /// 内容分析器为无状态结构，以零配置构造（见 [`ContentAnalyzer`]）。
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
        );

        Self {
            config: config.clone(),
            dispatcher,
            analyzer: ContentAnalyzer::new(),
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

    /// 压缩一段内存中的字符串文本。
    ///
    /// 将输入文本交由 `StreamReader::from_str_owned` 包装后，调用
    /// [`compress_stream`](Self::compress_stream) 完成实际压缩。若配置了
    /// `debug_audit_jsonl`，会先开启审计追踪，压缩结束后把本次事件（输入文本、
    /// 输出、插件链、审计副作用）写入指定审计文件。
    ///
    /// # 参数
    /// - `text`：待压缩的原始文本。
    ///
    /// # 返回
    /// 成功返回 `CompressionOutput`；审计写入或底层压缩失败返回 `PipelineError`。
    pub fn compress_str(&mut self, text: &str) -> Result<CompressionOutput, PipelineError> {
        let reader = StreamReader::from_str_owned(text.to_string());
        if self.config.debug_audit_jsonl.is_some() {
            self.dispatcher.begin_audit_trace();
        }
        let output = self.compress_stream(&reader)?;
        if let Some(audit_path) = self.config.debug_audit_jsonl.as_deref() {
            crate::core::debug_audit::write_compression_event(
                audit_path,
                crate::core::debug_audit::AuditSource::Text,
                text,
                &output,
                &self.config.debug_audit_attribution,
                self.dispatcher.audit_plugin_chain(),
                self.dispatcher.take_audit_effects(),
                self.metrics.plugin_fallback_counts(),
                self.dispatcher.take_ansi_strip_bytes_removed(),
            )?;
        }
        Ok(output)
    }
    /// 压缩磁盘上的一个文件。
    ///
    /// 与 [`compress_str`](Self::compress_str) 类似，输入源改为文件：通过
    /// `StreamReader::from_file` 读取并按需映射。启用审计时会先完整读取文件内容
    /// 作为审计输入，再执行压缩，最后写出审计事件（`AuditSource::File` 带路径）。
    ///
    /// # 参数
    /// - `path`：待压缩文件的路径。
    ///
    /// # 返回
    /// 成功返回 `CompressionOutput`；文件读取、审计写入或底层压缩失败返回 `PipelineError`。
    pub fn compress_file(
        &mut self,
        path: &std::path::Path,
    ) -> Result<CompressionOutput, PipelineError> {
        let audit_input = if self.config.debug_audit_jsonl.is_some() {
            Some(std::fs::read_to_string(path)?)
        } else {
            None
        };
        let reader = StreamReader::from_file(path)?;
        if self.config.debug_audit_jsonl.is_some() {
            self.dispatcher.begin_audit_trace();
        }
        let output = self.compress_stream(&reader)?;
        if let (Some(audit_path), Some(input)) = (
            self.config.debug_audit_jsonl.as_deref(),
            audit_input.as_deref(),
        ) {
            crate::core::debug_audit::write_compression_event(
                audit_path,
                crate::core::debug_audit::AuditSource::File(path),
                input,
                &output,
                &self.config.debug_audit_attribution,
                self.dispatcher.audit_plugin_chain(),
                self.dispatcher.take_audit_effects(),
                self.metrics.plugin_fallback_counts(),
                self.dispatcher.take_ansi_strip_bytes_removed(),
            )?;
        }
        Ok(output)
    }
    /// 压缩一个 `StreamReader` 输入流，是字符串/文件入口的统一调度点。
    ///
    /// 记录输入规模并启动 `compression_pipeline` 模块计时；随后按输入大小与配置
    /// 选择执行路径：当输入字节数达到 `parallel_threshold` 且未启用日志重排时走
    /// 并行路径 [`compress_stream_parallel`](Self::compress_stream_parallel)，
    /// 否则走串行路径 [`compress_stream_serial`](Self::compress_stream_serial)。
    /// 结束模块计时后返回结果。
    ///
    /// # 参数
    /// - `reader`：已构造好的输入流读取器。
    ///
    /// # 返回
    /// 压缩结果 `CompressionOutput` 或 `PipelineError`。
    pub fn compress_stream<'a>(
        &mut self,
        reader: &StreamReader<'a>,
    ) -> Result<CompressionOutput, PipelineError> {
        let original_size = reader.size();
        self.metrics.set_input_size(original_size);
        self.metrics.start_module("compression_pipeline");

        let parallel_threshold = self.config.parallel_threshold.max(1);
        // 阶段 3（文档级定向）：仅在大输入路径（serial/parallel）计算文档级 sticky 种子，
        // 作为逐块分发的「内层识别→定向」先验；小输入路径已有整块识别/剥皮（document_skin /
        // 整块贝叶斯软提升），不再重复 classify。
        let doc_seed = if original_size >= SMALL_INPUT_WHOLE_THRESHOLD {
            self.document_dispatch_seed(reader)
        } else {
            None
        };
        let result = if original_size > 0 && original_size < SMALL_INPUT_WHOLE_THRESHOLD {
            // 整块一次分发（见 compress_whole_input），保留跨行综合信号
            self.compress_whole_input(reader)
        } else if original_size >= parallel_threshold && !self.config.reorder_config.enabled {
            self.compress_stream_parallel(reader, doc_seed)
        } else {
            self.compress_stream_serial(reader, doc_seed)
        };

        self.metrics.end_module("compression_pipeline");
        result
    }

    /// 文档级定向种子：对整段输入做一次贝叶斯分类，取文档级候选插件首位作为
    /// 大输入路径逐块分发的 sticky 初始种子，让无皮文档也走「识别→定向」而非纯段落竞争。
    ///
    /// 超大输入（> `DOC_SAMPLE_BYTES`）仅取头部样本定性，避免全量 tokenize 代价；
    /// 头部通常是命令头/起始段，足以表达文档整体语义倾向。无明确语义（GenericText）
    /// 或置信不足时返回 `None`，交由现状全插件竞争兜底。sticky 种子不硬路由：内容转向时
    /// 该插件对后续切片 `detect` 不达标即失效重置（见 [`PluginDispatcher::dispatch_slice_sticky`]）。
    #[tracing::instrument(level = "debug", skip_all)]
    fn document_dispatch_seed<'a>(&self, reader: &StreamReader<'a>) -> Option<&'static str> {
        const DOC_SAMPLE_BYTES: usize = 64 * 1024;
        let data = reader.get_data();
        let sample_len = data.len().min(DOC_SAMPLE_BYTES);
        let lossy = String::from_utf8_lossy(&data[..sample_len]);
        self.analyzer
            .document_category(&lossy)
            .and_then(|cat| cat.candidate_plugins().first().copied())
    }

    /// 小输入整块路径：整个输入作为一个文档切片一次性分发。
    ///
    /// 交互式命令输出（git status、cargo 报错、ls 等）通常远小于 `SMALL_INPUT_WHOLE_THRESHOLD`
    /// 且每行信号稀疏。段落模式在空行处把 cargo 报错的 `error:` / `Usage:` / `help` 各自切成一
    /// 个单行切片，导致 `detect` 采不到「cargo test + error: + Usage:` 的跨行综合信号，插件置信度
    /// 上不去而骤跌到 generic_text 兜底。这里对小输入不做按行/段落切片，整块递归展开后作为
    /// 单个文档切片交给 [`compress_one_slice`](Self::compress_one_slice) 压缩，保留全文上下文；
    /// 同时天然不破坏 diff 等跨行语义块（整块本就是一体，不存在的切片切断问题）。
    fn compress_whole_input<'a>(
        &mut self,
        reader: &StreamReader<'a>,
    ) -> Result<CompressionOutput, PipelineError> {
        let start_time = Instant::now();
        self.processing_context.reset_timestamp();
        let arena = Bump::new();
        let mut tokens = Vec::new();
        let mut metrics_delta = DispatchMetricsDelta::default();
        // P1-04：整块路径持有一个跨切片复用的去重引擎（原每切片重建）。
        let mut local_dedup = DedupEngine::new(self.config.dedup_config.clone());

        self.metrics.start_module("plugin_dispatcher");

        // 整块内容（小输入在内存中完整可得）
        let lossy = String::from_utf8_lossy(reader.get_data());
        let raw = lossy.as_ref();

        // 两层化文档级 Pass：整段定性 → 有皮剥皮 → 内层切片定向。
        // 命中带皮类别且剥皮后产物不劣于原文时采用两层 IR，否则回退现状单切片路径。
        if let Some(two_tier_tokens) = self.try_two_tier_compress(raw, &mut metrics_delta) {
            tokens.extend(two_tier_tokens);
        } else {
            // 现状路径：递归展开链路解码后，把整块内容聚合成一个文档切片
            let unwrapped = self.dispatcher.unwrap_recursive(raw).into_owned();
            let held = arena.alloc_str(&unwrapped);
            let slice = self.slicer.slice_line(&SliceInput {
                raw: Cow::Borrowed(held),
                offset: 0,
                line_number: 1,
                file_metadata: None,
            });

            let dispatch_started = Instant::now();
            // 小输入整块路径单切片分发：无文档级 sticky（整块已有贝叶斯软提升定向）。
            let mut sticky_plugin = None;
            let res = self.compress_one_slice(&slice, &arena, &mut local_dedup, &mut sticky_plugin);
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
        self.metrics.add_slice_count(1);

        // 统计压缩元数据：`original_tokens = reader.size() / 4` 是按"每 4 字节≈1 token"的粗估
        // （P2-13，中文/二进制场景偏差 ~25%），三路径同款；`slice_count: 1`（P2-12 的口径 A
        // = 整段即 1 个块，非行数、非切片数）。
        let original_tokens = reader.size() / 4;
        let compressed_tokens: usize = final_tokens.iter().map(|t| t.estimated_tokens()).sum();

        Ok(CompressionOutput {
            tokens: final_tokens,
            dictionary: self.dict_engine.snapshot(),
            metadata: CompressionMetadata {
                original_size: reader.size(),
                compressed_size: output_size,
                original_tokens,
                compressed_tokens,
                token_savings: original_tokens.saturating_sub(compressed_tokens),
                compression_ratio: if reader.size() == 0 {
                    1.0
                } else {
                    output_size as f32 / reader.size() as f32
                },
                token_ratio: if original_tokens == 0 {
                    1.0
                } else {
                    compressed_tokens as f32 / original_tokens as f32
                },
                slice_count: 1,
                processing_time_ms: start_time.elapsed().as_millis(),
                order_info: None,
                base_timestamp: self
                    .processing_context
                    .base_timestamp()
                    .map(|ts| ts.to_rfc3339()),
                source_encoding: None,
            },
        })
    }

    /// 两层化文档级压缩：整段识别带皮类别 → 剥皮 → 内层正文重新走管线 → 组合两层 IR。
    ///
    /// 按「① 文档级识别 ② 有皮剥皮 ③ 内层切片定向 ④ 组合 ⑤ ROI 门控」执行：
    /// 1. 整段 `classify`，仅命中剥皮类别（syslog/ci/cloud）且置信达标才继续；
    /// 2. 交由对应剥皮插件把外壳骨架与内层正文分离；
    /// 3. 内层正文经 [`compress_inner_body`](Self::compress_inner_body) 重新切片定向；
    /// 4. 组合「外壳摘要 + 内层 IR」，命令锚点保持在首行（法则 0）；外壳无可留信号时
    ///    仅保留内层 IR + 锚点单层（决策 2 两种形态并存）；内层 token 全变体透传
    ///    （P1-05，不再只保留 Text），ROI 门控按同口径比较（P2-40）；
    /// 5. 两层产物不劣于原文才采纳，否则返回 `None` 交由调用方回退现状单切片路径。
    ///
    /// 仅在小输入整块路径（[`compress_whole_input`](Self::compress_whole_input)）内生效，
    /// 不改变大输入并行/串行路径；指标增量（内层各切片）回写到调用方批次。
    #[tracing::instrument(level = "debug", skip_all)]
    fn try_two_tier_compress<'a>(
        &mut self,
        text: &str,
        metrics_delta: &mut DispatchMetricsDelta,
    ) -> Option<Vec<Token<'static>>> {
        // ① 文档级识别：整段 classify，命中剥皮类别才继续（无皮直接回退）
        let skin = self.analyzer.document_skin(text)?;
        // ② 文档级剥皮：由对应剥皮插件把外壳骨架与内层正文分离（无皮可剥回退）
        let plugin_name = skin.candidate_plugins().first().copied()?;
        let idx = *self.dispatcher.plugin_map.get(plugin_name)?;
        let peeled = match self.dispatcher.plugins[idx].peel_document(text) {
            Some(peeled) => peeled,
            None => {
                // P1-06 可观测性：剥皮失败（peel_miss）显式记录，不再无声 `?` 回退——
                // 否则同类静默失效（如 trait 默认实现返回 None）将来仍不可见。
                tracing::debug!("two-tier peel_miss: plugin '{plugin_name}' returned None");
                return None;
            }
        };
        // ③ 内层正文重新走管线（unwrapped → 二次切片 → 定向分发）
        let inner_tokens = self.compress_inner_body(&peeled.inner_body, metrics_delta);
        // ④ 组合外壳摘要 + 内层 IR，锚点保持在首行
        let mut combined = String::with_capacity(text.len() / 2 + 64);
        if peeled.summary.is_empty() {
            // 外壳无可留信号（决策 2：纯噪声皮）→ 仅内层 IR + 命令锚点单层
            if let Some(anchor) = text.lines().next() {
                combined.push_str(anchor);
                combined.push('\n');
            }
        } else {
            combined.push_str(&peeled.summary);
            if !combined.ends_with('\n') {
                combined.push('\n');
            }
        }
        // ⑤ ROI 门控（P2-40 同口径比较）：估算产物 = 外壳 header + 内层全部
        // token 的 estimated_size。内层不再被降级拼接（P1-05），非 Text 变体
        // 全部保留，门控不再出现「丢弃越多 → combined 越短 → 越易采纳」的反向激励。
        let inner_estimated: usize = inner_tokens.iter().map(|t| t.estimated_size()).sum();
        if combined.len() + inner_estimated >= text.len() {
            return None;
        }
        // ⑥ P1-05：内层 token 原样透传（DictRef/Repeat/Marker/Diff 交由解压侧
        // 正常还原，不再静默丢弃），外壳摘要/锚点作为首个 Text token 前置。
        let mut result = Vec::with_capacity(inner_tokens.len() + 1);
        result.push(Token::Text(Cow::Owned(combined)));
        result.extend(inner_tokens);
        Some(result)
    }

    /// 内层正文重新走管线：递归展开后按切片模式二次切片，逐块定向分发。
    ///
    /// 复用流水线持有的切片器（`self.slicer`）与字典引擎，每个内层切片经
    /// [`compress_one_slice`](Self::compress_one_slice) 压缩（内部注入贝叶斯候选插件
    /// 做定向提升），指标增量回写到调用方批次；用局部 arena 承载切片生命周期。
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_inner_body<'a>(
        &mut self,
        inner_body: &str,
        metrics_delta: &mut DispatchMetricsDelta,
    ) -> Vec<Token<'static>> {
        let unwrapped = self.dispatcher.unwrap_recursive(inner_body).into_owned();
        let inner_reader = StreamReader::from_str_owned(unwrapped);
        let arena = Bump::new();
        let mut tokens: Vec<Token<'static>> = Vec::new();
        let mut produced: Vec<Slice<'_>> = Vec::new();
        // 内层正文逐块独立定向：每个切片经 candidate_plugins_for_slice 贝叶斯软提升，
        // 无需文档级 sticky 种子（正文已是剥皮后的干净工具输出）。
        let mut sticky_plugin = None;
        // P1-04：内层路径持有跨切片复用的去重引擎（原每切片重建）。
        let mut local_dedup = DedupEngine::new(self.config.dedup_config.clone());

        for input in inner_reader.iter_lines() {
            self.slicer.push_slices_by_mode(&input, &mut produced);
            for slice in produced.drain(..) {
                let dispatch_started = Instant::now();
                let res =
                    self.compress_one_slice(&slice, &arena, &mut local_dedup, &mut sticky_plugin);
                let dispatch_duration = dispatch_started.elapsed();
                Self::collect_dispatch_metrics_delta(
                    metrics_delta,
                    &res,
                    dispatch_duration,
                    Some(slice.id),
                );
                tokens.extend(res.tokens.into_iter().map(|t| t.into_owned()));
            }
        }
        for slice in self.slicer.flush() {
            let dispatch_started = Instant::now();
            let res = self.compress_one_slice(&slice, &arena, &mut local_dedup, &mut sticky_plugin);
            let dispatch_duration = dispatch_started.elapsed();
            Self::collect_dispatch_metrics_delta(
                metrics_delta,
                &res,
                dispatch_duration,
                Some(slice.id),
            );
            tokens.extend(res.tokens.into_iter().map(|t| t.into_owned()));
        }
        tokens
    }

    /// 串行压缩路径：逐行/逐块读取、切片、分发并收集令牌。
    ///
    /// 以单 worker 调用 `split_for_parallel` 切分（每块上限约 5MB 且按语义边界断开）；
    /// 对每块先经分发器递归展开（`unwrap_recursive`），再按行送入切片器生成切片；
    /// 每个切片交给 [`compress_one_slice`](Self::compress_one_slice) 压缩并收集令牌与分发指标。
    /// 启用日志重排（`reorder_config.enabled`）时先经 `LogReorderer` 缓冲并按行 flush，
    /// 否则按原顺序逐行处理；行模式下在行尾补回换行令牌。最后 flush 切片器剩余切片，
    /// 用 [`merge_adjacent_tokens_static`](Self::merge_adjacent_tokens_static) 合并相邻令牌，
    /// 统计输出规模并返回 `CompressionOutput`（含切片数、耗时、基准时间戳等元数据）。
    ///
    /// # 参数
    /// - `reader`：输入流读取器。
    /// - `doc_seed`：文档级定向种子（阶段 3），作为逐块 sticky 初始值；内容转向时自动失效重置。
    ///
    /// # 返回
    /// 串行压缩结果 `CompressionOutput` 或 `PipelineError`。
    fn compress_stream_serial<'a>(
        &mut self,
        reader: &StreamReader<'a>,
        doc_seed: Option<&'static str>,
    ) -> Result<CompressionOutput, PipelineError> {
        let start_time = Instant::now();
        let line_mode = matches!(self.config.slicer_config.mode, SliceMode::Line);
        self.processing_context.reset_timestamp();
        let arena = Bump::new();
        let mut tokens = Vec::new();
        let mut line_count = 0usize;
        let mut metrics_delta = DispatchMetricsDelta::default();
        // 阶段 3：文档级定向种子作为 sticky 初始值，跨 chunk/切片保持，摆脱纯段落竞争。
        let mut sticky_plugin = doc_seed;
        // P1-04：串行路径接入与并行路径相同的跨切片去重（SharedDedupEngine）。
        // 旧实现完全没有共享去重，跨切片重复行在串行路径永远命不中（与并行
        // 路径行为分叉）；同时把 DedupEngine 提升到此处跨切片复用。
        let mut local_dedup = DedupEngine::new(self.config.dedup_config.clone());
        let mut dedup_cache: HashMap<u64, String> = HashMap::with_capacity(16384);

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
                        // P1-04：先过共享跨切片去重（与并行路径同一引擎/口径）。
                        if let Some(dedup) = self.dedup_engine.dedup_cross_slice_with_local(
                            &line,
                            &mut self.dict_engine,
                            &arena,
                            &mut dedup_cache,
                        ) {
                            tokens.extend(dedup.tokens.into_iter().map(|t| t.into_owned()));
                            continue;
                        }
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
                            let compress_result = self.compress_one_slice(
                                &slice,
                                &arena,
                                &mut local_dedup,
                                &mut sticky_plugin,
                            );
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
                    // P1-04：先过共享跨切片去重（text_to_check 口径与并行路径一致）。
                    let text_to_check = if has_line_break {
                        format!("{}\n", input.raw.as_ref())
                    } else {
                        input.raw.as_ref().to_string()
                    };
                    if let Some(dedup) = self.dedup_engine.dedup_cross_slice_with_local(
                        &text_to_check,
                        &mut self.dict_engine,
                        &arena,
                        &mut dedup_cache,
                    ) {
                        tokens.extend(dedup.tokens.into_iter().map(|t| t.into_owned()));
                        continue;
                    }
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
                        let compress_result = self.compress_one_slice(
                            &slice,
                            &arena,
                            &mut local_dedup,
                            &mut sticky_plugin,
                        );
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
                // P1-04：重排冲刷行同样先过共享跨切片去重。
                if let Some(dedup) = self.dedup_engine.dedup_cross_slice_with_local(
                    &line,
                    &mut self.dict_engine,
                    &arena,
                    &mut dedup_cache,
                ) {
                    tokens.extend(dedup.tokens.into_iter().map(|t| t.into_owned()));
                    continue;
                }
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
                    let compress_result = self.compress_one_slice(
                        &slice,
                        &arena,
                        &mut local_dedup,
                        &mut sticky_plugin,
                    );
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
            let res = self.compress_one_slice(&slice, &arena, &mut local_dedup, &mut sticky_plugin);
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

        // 统计压缩元数据：`original_tokens = reader.size() / 4` 粗估（P2-13，同三路径）；
        // `slice_count` 在这里取的是**输入行数** `line_count`（P2-12 的口径 B），
        // 与 whole 的 1、parallel 的 0 语义不同——消费方不能用它当切片数。
        let original_tokens = reader.size() / 4;
        let compressed_tokens: usize = final_tokens.iter().map(|t| t.estimated_tokens()).sum();

        Ok(CompressionOutput {
            tokens: final_tokens,
            dictionary: self.dict_engine.snapshot(),
            metadata: CompressionMetadata {
                original_size: reader.size(),
                compressed_size: output_size,
                original_tokens,
                compressed_tokens,
                token_savings: original_tokens.saturating_sub(compressed_tokens),
                compression_ratio: if reader.size() == 0 {
                    1.0
                } else {
                    output_size as f32 / reader.size() as f32
                },
                token_ratio: if original_tokens == 0 {
                    1.0
                } else {
                    compressed_tokens as f32 / original_tokens as f32
                },
                slice_count: line_count,
                processing_time_ms: start_time.elapsed().as_millis(),
                order_info: None,
                base_timestamp: self
                    .processing_context
                    .base_timestamp()
                    .map(|ts| ts.to_rfc3339()),
                source_encoding: None,
            },
        })
    }

    /// 并行压缩路径：基于 rayon 将数据块分发到多线程并发压缩。
    ///
    /// 仅在输入规模达到 `parallel_threshold` 且未启用日志重排时由
    /// [`compress_stream`](Self::compress_stream) 调用。每个数据块在独立线程中构造
    /// 本地的切片器、分析器、字典引擎与去重引擎，递归展开后逐行切片并调用
    /// `dispatch_slice_sticky` 压缩；跨切片去重复用共享 `dedup_engine`。
    /// 块内先用 [`fuse_tokens_local`](Self::fuse_tokens_local) 融合令牌，最终跨块用
    /// [`merge_adjacent_tokens_static`](Self::merge_adjacent_tokens_static) 合并，
    /// 并以第一个非空基准时间戳作为全局基准时间戳。
    /// 注意：并行路径下 `slice_count` 元数据固定为 0（未按行统计）。
    ///
    /// # 参数
    /// - `reader`：输入流读取器。
    /// - `doc_seed`：文档级定向种子（阶段 3），作为各 worker 内 sticky 初始值；
    ///   内容转向时自动失效重置。
    ///
    /// # 返回
    /// 并行压缩结果 `CompressionOutput` 或 `PipelineError`。
    fn compress_stream_parallel<'a>(
        &mut self,
        reader: &StreamReader<'a>,
        doc_seed: Option<&'static str>,
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
                    compressed_size: 0,
                    // P2-13：`reader.size()/4` 粗估（同三路径）。
                    original_tokens: reader.size() / 4,
                    compressed_tokens: 0,
                    token_savings: reader.size() / 4,
                    compression_ratio: 1.0,
                    token_ratio: 0.0,
                    // P2-12：并行路径 `slice_count` 固定 0（空输入分支亦同，口径 C）。
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
                let local_analyzer = ContentAnalyzer::new();
                let mut sticky_plugin = doc_seed;
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
                        // 接线分类器：把贝叶斯分类器建议的候选插件注入调度，提升专用插件优先级。
                        let candidate_plugins =
                            local_analyzer.candidate_plugins_for_slice(stable_slice);
                        let dispatch_started = Instant::now();
                        let res = dispatcher.dispatch_slice_sticky(
                            stable_slice,
                            Some(candidate_plugins),
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
                    // 接线分类器：flush 段同样注入贝叶斯分类器建议的候选插件。
                    let candidate_plugins =
                        local_analyzer.candidate_plugins_for_slice(stable_slice);
                    let dispatch_started = Instant::now();
                    let res = dispatcher.dispatch_slice_sticky(
                        stable_slice,
                        Some(candidate_plugins),
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

        // 统计压缩元数据：`original_tokens = reader.size() / 4` 粗估（P2-13，同三路径）；
        // `slice_count` 在并行路径固定为 0（P2-12 的口径 C，未按行统计，见函数级文档）。
        let original_tokens = reader.size() / 4;
        let compressed_tokens: usize = final_tokens.iter().map(|t| t.estimated_tokens()).sum();

        Ok(CompressionOutput {
            tokens: final_tokens,
            dictionary: self.dict_engine.snapshot(),
            metadata: CompressionMetadata {
                original_size: reader.size(),
                compressed_size: output_size,
                original_tokens,
                compressed_tokens,
                token_savings: original_tokens.saturating_sub(compressed_tokens),
                compression_ratio: if reader.size() == 0 {
                    1.0
                } else {
                    output_size as f32 / reader.size() as f32
                },
                token_ratio: if original_tokens == 0 {
                    1.0
                } else {
                    compressed_tokens as f32 / original_tokens as f32
                },
                slice_count: 0,
                processing_time_ms: start_time.elapsed().as_millis(),
                order_info: None,
                base_timestamp: global_base_ts.map(|ts| ts.to_rfc3339()),
                source_encoding: None,
            },
        })
    }

    /// 将连续相邻的文本令牌就地融合为更少的令牌，降低令牌数量。
    ///
    /// 遍历输入令牌：连续的 `Token::Text` 被累加进一个可增长字符串缓冲；一旦遇到
    /// 非文本令牌，先把已累积文本作为一个 `Token::Text` 落库，再原样压入该非文本令牌。
    /// 末尾若仍有累积文本也一并落库。用于并行 worker 内部先做一次局部融合，
    /// 减小后续跨块合并与下游处理的令牌规模。
    ///
    /// # 参数
    /// - `tokens`：单块压缩产出的令牌序列。
    ///
    /// # 返回
    /// 融合后的令牌序列（文本令牌更少、更紧凑）。
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

    /// 合并跨切片/跨块的相邻 `Token::Text`，是最终输出的令牌归并步骤。
    ///
    /// 与 `fuse_tokens_local` 不同，本函数接受 `'static` 生命周期令牌，直接把相邻的
    /// 两个 `Token::Text` 内容拼接进前者（原地增长 `Cow`），非文本令牌则原样移入结果序列。
    /// 用于串行/并行路径收尾，将分散的文本碎片合并为更少文本令牌，提升下游序列化与传输效率。
    ///
    /// # 参数
    /// - `tokens`：待归并的 `'static` 令牌序列。
    ///
    /// # 返回
    /// 归并后的令牌序列。
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

    /// 对单个切片执行一次分发压缩，是串行路径的最小压缩单元。
    ///
    /// 复用流水线持有的字典引擎（`self.dict_engine`）与压缩上下文
    /// （`self.processing_context`），并为本次调用新建一个局部去重引擎，
    /// 以“粘性插件”（sticky）方式调用分发器的 `dispatch_slice_sticky`。
    /// `sticky_plugin` 由调用方持有并在路径内跨切片保持：串行路径初始化为文档级
    /// 定向种子（阶段 3），小输入整块/内层正文路径传局部 `None` 保持现状。
    ///
    /// # 参数
    /// - `slice`：待压缩切片（生命周期与 `arena` 绑定）。
    /// - `arena`：用于分配压缩过程中临时令牌的 bump 分配器。
    /// - `sticky_plugin`：粘性插件状态（可能为文档级定向种子），分发成功后回写。
    ///
    /// # 返回
    /// 该切片的压缩结果（含令牌、命中插件名与解析层级元数据）。
    fn compress_one_slice<'a>(
        &mut self,
        slice: &'a Slice<'a>,
        arena: &'a Bump,
        local_dedup: &mut DedupEngine,
        sticky_plugin: &mut Option<&'static str>,
    ) -> crate::core::plugin_dispatcher::CompressResult<'a> {
        // P1-04：DedupEngine 由调用方持有并跨切片复用（每切片重建会使
        // seen_hashes 清空，跨切片去重永远命不中）。
        // 接线分类器：把贝叶斯分类器建议的候选插件注入调度，提升专用插件优先级。
        let candidate_plugins = self.analyzer.candidate_plugins_for_slice(slice);
        self.dispatcher.dispatch_slice_sticky(
            slice,
            Some(candidate_plugins),
            &mut self.dict_engine,
            local_dedup,
            arena,
            &mut self.processing_context,
            sticky_plugin,
        )
    }

    /// 返回流水线持有的指标采集器引用。
    ///
    /// 供调用方在压缩前后读取累计的输入/输出规模、各模块耗时、插件级检测/压缩
    /// 调用次数与降级错误等运行时指标。
    ///
    /// # 返回
    /// 指向内部 `MetricsCollector` 的不可变引用。
    pub fn get_metrics(&self) -> &MetricsCollector {
        &self.metrics
    }

    /// P2-08：记录输入解码发生 lossy 替换（无可用编码候选、必产 U+FFFD）的可观测信号。
    ///
    /// 写入 metric 的 errors 槽，供 `snapshot().errors` 在 TS_MON_VERBOSE 等路径暴露；
    /// 不中断压缩，仅让「数据被不可逆降级」这件事对调用方/指标可见，替代原始静默 U+FFFD。
    #[tracing::instrument(level = "warn", skip_all)]
    pub fn record_encoding_lossy(&mut self, message: &str) {
        self.metrics.log_plugin_error(
            "encoding_fallback",
            None,
            "UTF8_LOSSY_REPLACEMENT",
            message,
            None,
        );
    }

    /// 把单次切片分发结果累加到本批指标增量（`DispatchMetricsDelta`）中。
    ///
    /// 以命中的插件名（未命中时记为 `dispatcher_passthrough`）为键，分别累加检测与压缩的
    /// 调用次数与耗时；当解析层级（`parse_tier`）非 `full` 时计入回退次数；当层级为
    /// `degraded` 或原因为 `plugin_failed` 时记录一条错误项（含插件名、错误类型、原因与切片 ID）。
    ///
    /// # 参数
    /// - `delta`：本批指标增量结构，就地累加。
    /// - `result`：单次分发压缩结果。
    /// - `duration`：本次分发耗时。
    /// - `slice_id`：触发本次分发的切片 ID（用于错误定位）。
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

        // P2-42：fallback 判据与 errors 判据对齐——仅「降级」或「插件失败」才算回退。
        // 旧判据 `parse_tier != "full"` 把直通（quick_skip/无候选，plugin_name=None）与
        // 成功接管但 metadata 缺失（tier=unknown）的切片全部计入回退，导致指标系统性虚高；
        // 这两类都不是真实回退，直通由 plugin_compress 的 `dispatcher_passthrough` 单独口径统计。
        let is_fallback = parse_tier == "degraded" || parse_reason == "plugin_failed";
        if is_fallback {
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

    /// 将一批指标增量回写到流水线全局指标采集器。
    ///
    /// 依次把 `plugin_detect`、`plugin_compress` 的批量调用计数与耗时、`plugin_fallback`
    /// 的回退次数，以及 `errors` 中的插件错误项记录到 `self.metrics`。串行路径在收尾时
    /// 调用一次；并行路径对每个 worker 的增量各调用一次（以 `&` 引用，避免转移所有权）。
    ///
    /// 同时排水调度器窗口内累计的插件 panic 计数（P1-10 隔离点写入），
    /// 使 `inc_plugin_panic` 指标在真实 panic 场景下非 0（修复 P2-63）。
    ///
    /// # 参数
    /// - `delta`：待回写的指标增量（不可变引用）。
    fn apply_dispatch_metrics_delta_ref(&mut self, delta: &DispatchMetricsDelta) {
        for (plugin, count) in self.dispatcher.take_plugin_panic_counts() {
            for _ in 0..count {
                self.metrics.inc_plugin_panic(&plugin);
            }
        }
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

    /// 指标增量回写的便捷包装：取得所有权后转发给
    /// [`apply_dispatch_metrics_delta_ref`](Self::apply_dispatch_metrics_delta_ref)。
    ///
    /// 串行路径收集完本批 `DispatchMetricsDelta` 后调用本函数，由其将增量一次性写入全局
    /// 指标采集器。与引用版本的区别仅在于是否转移增量所有权。
    ///
    /// # 参数
    /// - `delta`：本批指标增量（按值传入）。
    fn apply_dispatch_metrics_delta(&mut self, delta: DispatchMetricsDelta) {
        self.apply_dispatch_metrics_delta_ref(&delta);
    }
}
