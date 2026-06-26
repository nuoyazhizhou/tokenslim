//! plugin dispatcher 方法实现

use super::types::*;
use crate::core::compression_context::CompressionContext;
use crate::core::content_analyzer::AnalysisResult;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::DictionaryEngine;
use crate::core::dictionary_manager::DictionaryManager;
use crate::core::error_isolation::{SafeExecutor, SafeExecutorConfig};
use crate::core::text_slicer::Slice;
use aho_corasick::AhoCorasick;
use bumpalo::Bump;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

const PARSE_TIER_KEY: &str = "parse_tier";
const PARSE_REASON_KEY: &str = "parse_reason";

#[derive(Clone, Copy)]
enum ParseTier {
    Full,
    Degraded,
    Passthrough,
}

impl ParseTier {
    fn as_str(self) -> &'static str {
        match self {
            ParseTier::Full => "full",
            ParseTier::Degraded => "degraded",
            ParseTier::Passthrough => "passthrough",
        }
    }
}

fn with_parse_tier<'a>(
    mut result: CompressResult<'a>,
    tier: ParseTier,
    reason: &'static str,
) -> CompressResult<'a> {
    let mut metadata = result.metadata.take().unwrap_or_default();
    metadata.insert(PARSE_TIER_KEY.to_string(), tier.as_str().to_string());
    metadata.insert(PARSE_REASON_KEY.to_string(), reason.to_string());
    result.metadata = Some(metadata);
    result
}

impl PluginDispatcher {
    pub fn new(
        plugins: Vec<Box<dyn Plugin>>,
        config: DispatcherConfig,
        _executor_config: SafeExecutorConfig,
        dict_manager: Arc<DictionaryManager>,
    ) -> Self {
        let mut plugin_map = HashMap::new();
        for (i, p) in plugins.iter().enumerate() {
            plugin_map.insert(p.name().to_string(), i);
        }

        let keywords = vec![
            "gcc",
            "g++",
            "make[",
            "error:",
            "warning:",
            "note:",
            "at ",
            "Traceback",
            "Exception",
            "Caused by:",
            "{",
            "[",
            "http",
            "https",
            "diff --git",
            "--- ",
            "+++ ",
            "SELECT",
            "INSERT",
            "UPDATE",
            "DELETE",
            "/",
            "\\",
            ".",
            "-",
            "_",
            ":",
        ];
        let keyword_scanner = Arc::new(AhoCorasick::new(keywords).unwrap());

        PluginDispatcher {
            plugins,
            plugin_map,
            config,
            executor: SafeExecutor::new(SafeExecutorConfig::default()),
            dict_manager,
            keyword_scanner,
            plugin_failures: std::sync::Mutex::new(HashMap::new()),
        }
    }

    fn is_plugin_blacklisted(&self, name: &str) -> bool {
        if let Ok(failures) = self.plugin_failures.lock() {
            if let Some(&count) = failures.get(name) {
                return count >= 3;
            }
        }
        false
    }

    /// 执行全局脱壳流水线（Unwrapper Pipeline）。
    /// 将文本递归送入各个插件的 unwrap 方法，直到没有任何插件可以继续脱壳，或达到最大深度（5次）。
    pub(crate) fn unwrap_recursive<'a>(&self, text: &'a str) -> Cow<'a, str> {
        let mut current_text = Cow::Borrowed(text);
        let max_depth = 5;

        for _ in 0..max_depth {
            let mut unwrapped = false;
            for plugin in &self.plugins {
                if let Some(new_text) = plugin.unwrap(current_text.as_ref()) {
                    current_text = Cow::Owned(new_text);
                    unwrapped = true;
                    break; // 重头开始匹配（防止多层不同外壳）
                }
            }
            if !unwrapped {
                break;
            }
        }

        current_text
    }

    pub(crate) fn detect_parallel<'a>(&self, slice: &'a Slice<'a>) -> Vec<(&dyn Plugin, f32)> {
        let mut detections: Vec<_> = self
            .plugins
            .iter()
            .filter(|p| !self.is_plugin_blacklisted(p.name()))
            .filter_map(|plugin| {
                plugin
                    .detect(slice)
                    .map(|confidence| (plugin.as_ref(), confidence))
            })
            .collect();
        detections.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        detections
    }

    /// 高性能调度：支持“粘性插件”缓存
    pub fn dispatch_slice_sticky<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        local_dedup: &mut DedupEngine, // 复用外部传入的去重器，消除分配
        arena: &'a Bump,
        context: &mut CompressionContext,
        sticky_plugin_name: &mut Option<&'static str>,
    ) -> CompressResult<'a> {
        // 1. Sticky Path
        if let Some(name) = *sticky_plugin_name {
            if let Some(&idx) = self.plugin_map.get(name) {
                let plugin = self.plugins[idx].as_ref();
                if let Some(conf) = plugin.detect(slice) {
                    if conf > 0.5 {
                        if let Some(res) = self.execute_plugin_chain_fast(
                            plugin,
                            slice,
                            dict_engine,
                            local_dedup,
                            arena,
                            context,
                            0,
                        ) {
                            return with_parse_tier(res, ParseTier::Full, "sticky_plugin");
                        }
                    }
                }
            }
        }

        // 2. Quick Skip (无关键字且不太长，直接返回 Text)
        let text = slice.text.as_ref();
        if text.len() < 1000 && self.keyword_scanner.find(text).is_none() {
            return with_parse_tier(
                CompressResult {
                    tokens: vec![crate::core::compression::Token::Text(Cow::Borrowed(text))],
                    metadata: None,
                    plugin_name: None,
                },
                ParseTier::Passthrough,
                "quick_skip_no_keyword",
            );
        }

        // 3. Full Detect
        let mut candidates = self.detect_parallel(slice);
        candidates.retain(|(_, conf)| *conf > 0.1);

        if !candidates.is_empty() {
            for (plugin, _conf) in candidates {
                if let Some(res) = self.execute_plugin_chain_fast(
                    plugin,
                    slice,
                    dict_engine,
                    local_dedup,
                    arena,
                    context,
                    0,
                ) {
                    *sticky_plugin_name = Some(plugin.name());
                    return with_parse_tier(res, ParseTier::Full, "plugin_match");
                }
            }

            return with_parse_tier(
                CompressResult {
                    tokens: vec![crate::core::compression::Token::Text(Cow::Borrowed(
                        slice.text.as_ref(),
                    ))],
                    metadata: None,
                    plugin_name: None,
                },
                ParseTier::Degraded,
                "plugin_failed",
            );
        }

        with_parse_tier(
            CompressResult {
                tokens: vec![crate::core::compression::Token::Text(Cow::Borrowed(
                    slice.text.as_ref(),
                ))],
                metadata: None,
                plugin_name: None,
            },
            ParseTier::Passthrough,
            "no_plugin_candidate",
        )
    }

    pub fn dispatch_slice<'a>(
        &self,
        slice: &'a Slice<'a>,
        _result: &AnalysisResult,
        _candidate_plugins: Option<&[&str]>,
        dict_engine: &mut DictionaryEngine,
        dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
        context: &mut CompressionContext,
    ) -> CompressResult<'a> {
        let mut dummy = None;
        self.dispatch_slice_sticky(slice, dict_engine, dedup_engine, arena, context, &mut dummy)
    }

    fn execute_plugin_chain_fast<'a>(
        &self,
        plugin: &dyn Plugin,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        local_dedup: &mut DedupEngine,
        arena: &'a Bump,
        context: &mut CompressionContext,
        depth: usize,
    ) -> Option<CompressResult<'a>> {
        if depth > 5 {
            return None;
        }

        let result = plugin.compress_with_context(slice, dict_engine, local_dedup, arena, context);

        let next_plugins = plugin.next_plugins();
        if next_plugins.is_empty() {
            return Some(result);
        }

        let mut final_result = result;
        for next_name in next_plugins {
            if let Some(&idx) = self.plugin_map.get(next_name) {
                let next_plugin = self.plugins[idx].as_ref();
                let mut chained_tokens = Vec::new();
                let mut chained = false;

                let current_tokens = std::mem::take(&mut final_result.tokens);
                for token in current_tokens {
                    if let crate::core::compression::Token::Text(text) = &token {
                        let temp_text = arena.alloc_str(text.as_ref());
                        let temp_slice = arena.alloc(Slice {
                            id: 0,
                            text: Cow::Borrowed(temp_text),
                            slice_type: slice.slice_type.clone(),
                            offset: slice.offset,
                            line_start: slice.line_start,
                            line_end: slice.line_end,
                            file_metadata: slice.file_metadata.clone(),
                            flags: slice.flags.clone(),
                        });

                        if let Some(next_res) = self.execute_plugin_chain_fast(
                            next_plugin,
                            temp_slice,
                            dict_engine,
                            local_dedup,
                            arena,
                            context,
                            depth + 1,
                        ) {
                            chained_tokens.extend(next_res.tokens);
                            chained = true;
                        } else {
                            chained_tokens.push(token);
                        }
                    } else {
                        chained_tokens.push(token);
                    }
                }
                final_result.tokens = chained_tokens;
                if !chained {
                    break;
                }
            }
        }

        Some(final_result)
    }
}
