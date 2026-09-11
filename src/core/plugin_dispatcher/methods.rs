//! plugin dispatcher 方法实现

use super::types::*;
use crate::core::compression_context::CompressionContext;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::DictionaryEngine;
use crate::core::dictionary_manager::DictionaryManager;
use crate::core::error_isolation::{ExecutionError, SafeExecutor, SafeExecutorConfig};
use crate::core::text_slicer::Slice;
use aho_corasick::AhoCorasick;
use bumpalo::Bump;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

const PARSE_TIER_KEY: &str = "parse_tier";
const PARSE_REASON_KEY: &str = "parse_reason";

/// ANSI 剥离统一走 [`crate::core::utils::strip_ansi`]（P3-192 C-4：消除双实现漂移，
/// 完整语义含真 ESC 序列 + 裸 CSI 残留 + 合法路径字面量保护）。

/// 剥离文本中的 ANSI 控制码（纯函数，便于单测回归）。
///
/// 统一委托 [`crate::core::utils::strip_ansi`]——真 ANSI 序列（含真实 ESC 字节 `\x1B`）
/// 无条件剥离；裸 CSI 残留（`[Nm`）同样无条件剥离（302 字节 cargo 报错样本）；
/// 裸码后紧跟 `]` 且该 `]` 后是非色码字符时保留（`path/to/[2m]odule.rs` 等合法目录名）。
fn strip_ansi(text: &str) -> String {
    crate::core::utils::strip_ansi(text)
}

/// 剥离切片文本中的 ANSI 控制码。
///
/// **剥离开关**：真 ESC 序列无条件剥离；裸 CSI 残留（`[Nm`）也**不分是否含真实 ESC 一律
/// 剥离**——脱色日志的色码本身就是「去 ESC 后的字面量」，若以真实 ESC 为开关就漏剥。
/// 唯一的保留条件是：裸码后紧跟 `]` **且该 `]` 后不是另一个 `[Nm`**（保护
/// `path/to/[2m]odule.rs`、`test [0m] ok` 这类以 `]` 收尾、其后衔接路径字符的合法目录名，
/// 避免砍坏 smart_path 依赖的路径；而 `[1m[96m][0m` 里的 `][0m` 是相邻色码链，`]` 只是被
/// 着色的字面括号，色码仍须剥，否则 cargo Usage 的 `[96m]` 会泄漏进输出）。
///
/// 命中重建时在 arena 中产生新 `Slice`，其余字段（id、offset、行列范围、文件元数据、flags）原样保留。
fn strip_ansi_slice<'a>(slice: &'a Slice<'a>, arena: &'a Bump) -> &'a Slice<'a> {
    let text = slice.text.as_ref();
    let cleaned = strip_ansi(text);
    // 零剥离 → 原样透传，零分配
    if cleaned.len() == text.len() {
        return slice;
    }
    let cleaned_ref = arena.alloc_str(&cleaned);
    arena.alloc(Slice {
        id: slice.id,
        text: Cow::Borrowed(cleaned_ref),
        slice_type: slice.slice_type,
        offset: slice.offset,
        line_start: slice.line_start,
        line_end: slice.line_end,
        file_metadata: slice.file_metadata,
        flags: slice.flags,
    })
}

#[derive(Clone, Copy)]
enum ParseTier {
    Full,
    Degraded,
    Passthrough,
}

impl ParseTier {
    /// 将解析分级（ParseTier）映射为其稳定字符串表示，用于写入压缩结果的 metadata，
    /// 以便审计与下游判断本次分片最终走了「完整解析 / 降级 / 透传」中的哪条路径。
    fn as_str(self) -> &'static str {
        match self {
            ParseTier::Full => "full",
            ParseTier::Degraded => "degraded",
            ParseTier::Passthrough => "passthrough",
        }
    }
}

/// 给一次分片压缩结果打上「解析分级」与「触发原因」的元数据标签。
/// 取出（或新建）结果中的 metadata，写入 `parse_tier` / `parse_reason` 两个键后放回，
/// 便于后续统计各分片的解析质量分布与降级来源。
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
    /// 构造 `PluginDispatcher`：建立插件名称→下标索引（`plugin_map`）以支持 O(1) 查找，
    /// 预编译关键字 Aho-Corasick 扫描器以加速「无关键字纯文本」分片的快速跳过，
    /// 并以默认 `SafeExecutorConfig` 初始化错误隔离执行器，准备好失败计数与审计聚合容器。
    /// `executor_config` 的 `catch_panic` 语义被保留，`default_timeout` 由
    /// `config.plugin_timeout_ms` 接管（P2-65 接线：调度器超时配置此前被
    /// 无视，统一走 `SafeExecutorConfig::default()`）。同名插件重复注册时
    /// 记录 `warn!` 日志（后注册者覆盖先注册者，索引仍保持 O(1) 查找）。
    pub fn new(
        plugins: Vec<Box<dyn Plugin>>,
        config: DispatcherConfig,
        executor_config: SafeExecutorConfig,
    ) -> Self {
        let mut plugin_map = HashMap::new();
        for (i, p) in plugins.iter().enumerate() {
            if plugin_map.insert(p.name().to_string(), i).is_some() {
                tracing::warn!("duplicate plugin name registered: {}", p.name());
            }
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
            // 配置文件（TOML/INI）特征：k=v 分隔符与本格式自述关键字，保证小尺寸配置文件
            // 也能绕过 quick_skip 进入 toml_ini 插件的行级结构 detect，而非被跳过。
            "=",
            "toml",
        ];
        let keyword_scanner = Arc::new(AhoCorasick::new(keywords).unwrap());

        PluginDispatcher {
            plugins,
            plugin_map,
            executor: SafeExecutor::new(SafeExecutorConfig {
                default_timeout: std::time::Duration::from_millis(config.plugin_timeout_ms),
                ..executor_config
            }),
            config,
            keyword_scanner,
            plugin_failures: std::sync::Mutex::new(HashMap::new()),
            plugin_panic_counts: std::sync::Mutex::new(HashMap::new()),
            audit_effects: std::sync::Mutex::new(Vec::new()),
            audit_trace_active: std::sync::atomic::AtomicBool::new(false),
            ansi_strip_bytes_removed: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// 判断插件是否已被列入「失败黑名单」。
    /// 当某插件累计失败次数 >= 3 时视为不稳定，后续调度直接跳过该插件以避免反复崩溃。
    fn is_plugin_blacklisted(&self, name: &str) -> bool {
        if let Ok(failures) = self.plugin_failures.lock() {
            if let Some(&count) = failures.get(name) {
                return count >= 3;
            }
        }
        false
    }

    /// 记录插件一次失败（含 panic 隔离与错误路径），累入失败黑名单计数。
    /// 达到阈值 3 后 [`Self::is_plugin_blacklisted`] 生效，调度层跳过该插件。
    fn record_plugin_failure(&self, name: &str) {
        if let Ok(mut failures) = self.plugin_failures.lock() {
            *failures.entry(name.to_string()).or_insert(0) += 1;
        }
        log::warn!("plugin '{}' recorded failure (blacklist counting)", name);
    }

    /// 记录插件一次 panic 隔离事件（P1-10）。
    /// 计入 `plugin_panic_counts`，由管线在指标回写点排水进
    /// `MetricsCollector::inc_plugin_panic`（修复 P2-63 panic 指标恒 0）。
    fn record_plugin_panic(&self, name: &str) {
        if let Ok(mut counts) = self.plugin_panic_counts.lock() {
            *counts.entry(name.to_string()).or_insert(0) += 1;
        }
    }

    /// 排水并清空本压缩窗口内的插件 panic 计数（供管线回写全局指标）。
    pub(crate) fn take_plugin_panic_counts(&self) -> HashMap<String, u32> {
        std::mem::take(
            &mut self
                .plugin_panic_counts
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
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

    /// 并行对各插件做 detect，返回「插件引用 + 置信度」列表并按置信度降序排序。
    /// 会跳过被列入黑名单的插件；插件放弃识别（detect 返回 None）时也被过滤掉。
    ///
    /// 置信度平局时按 [`Plugin::priority`] 升序（数值小者优先）破平：这是 priority
    /// 唯一的调度消费点。安全语义依赖——privacy（priority=0）承诺「凭证在进入字典、
    /// 去重等有状态引擎之前先被脱敏」，若平局仅按注册列表顺序回落，json（priority=146，
    /// 注册第 14 位）会先于 privacy（注册第 26 位）抢到 confidence 1.0 的含凭证切片。
    /// 下方候选裁剪排序为稳定排序，平局时继承此处次序，故优先级 tiebreak 只需此一处。
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
        detections.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.priority().cmp(&b.0.priority()))
        });
        detections
    }

    /// 高性能调度：支持“粘性插件”缓存
    ///
    /// - `candidate_plugins`: 贝叶斯分类器给出的「建议候选插件名」；命中时在 Full Detect
    ///   阶段将其提升到候选列表最前优先尝试，避免落到 generic_text 兜底。
    pub fn dispatch_slice_sticky<'a>(
        &self,
        slice: &'a Slice<'a>,
        candidate_plugins: Option<&[&'static str]>,
        dict_engine: &mut DictionaryEngine,
        local_dedup: &mut DedupEngine, // 复用外部传入的去重器，消除分配
        arena: &'a Bump,
        context: &mut CompressionContext,
        sticky_plugin_name: &mut Option<&'static str>,
    ) -> CompressResult<'a> {
        // 0. 统一入口剥离 ANSI：彩色化 cargo/gcc 输出会残留 `\x1b[31m` 等转义码，导致
        // rust_go/smart_path 等插件的行前缀匹配（`error[E`/`-->`/`warning:`）全部失配而落到
        // generic_text 兜底；同时 ansi_cleaner.detect 返回 0.1 会被下方 `conf > 0.1` 过滤掉而
        // 永不执行。这里在 sticky/quick-skip/detect 之前统一清理，让所有插件看到干净文本。
        // 同时累加「删除了多少字节」到 `ansi_strip_bytes_removed`，供审计红灯量化（见
        // [`PluginDispatcher::take_ansi_strip_bytes_removed`]）。
        let original_text_len = slice.text.len();
        let slice = strip_ansi_slice(slice, arena);
        let removed = original_text_len.saturating_sub(slice.text.len());
        if removed > 0 {
            self.ansi_strip_bytes_removed
                .fetch_add(removed, std::sync::atomic::Ordering::Relaxed);
        }

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

        // 分类裁剪：当贝叶斯分类器给出建议候选插件时，将命中者提升到最优先尝试，
        // 使 cagoo/gcc 等输出优先路由到专用插件，而非骤跌到 generic_text 兜底。
        if let Some(plugins) = candidate_plugins {
            if !plugins.is_empty() {
                candidates.sort_by(|a, b| {
                    let a_hit = plugins.contains(&a.0.name());
                    let b_hit = plugins.contains(&b.0.name());
                    b_hit
                        .cmp(&a_hit)
                        .then(b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
                });
            }
        }

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

    /// 执行单个插件的压缩，并按该插件的 `next_plugins` 声明的后继链做「链式递归压缩」。
    /// 产生的每个 `Text` token 会再被后继插件压缩（递归深度上限 5，防止无限嵌套）；
    /// 同时记录本次执行的审计贡献。返回压缩结果，若插件未产出任何 token 则返回 None。
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

        // P1-10 错误隔离：插件压缩调用统一经 SafeExecutor 捕获 panic。
        // `&mut DictionaryEngine` 等借用在 unwind 语义上无法静态证明安全，
        // 用 `AssertUnwindSafe` 显式声明（panic 发生时这些引用的生命周期
        // 仍由外层作用域保证；插件半途修改的字典状态由压缩协议的
        // append-only 约定兜底）。panic 被降级为「本次产出为空」：
        // 计入失败黑名单（达到 3 次后调度层跳过）与 panic 指标，不再击穿进程。
        let result = match self.executor.catch_panic(std::panic::AssertUnwindSafe(|| {
            plugin.compress_with_context(slice, dict_engine, local_dedup, arena, context)
        })) {
            Ok(result) => result,
            Err(ExecutionError::Panic) => {
                self.record_plugin_panic(plugin.name());
                self.record_plugin_failure(plugin.name());
                return None;
            }
            Err(err) => {
                log::warn!("plugin '{}' execution error: {err}", plugin.name());
                self.record_plugin_failure(plugin.name());
                return None;
            }
        };

        self.record_plugin_audit_effect(
            plugin.name(),
            plugin.priority(),
            slice.text.as_ref().len(),
            result
                .tokens
                .iter()
                .map(|token| token.estimated_size())
                .sum(),
        );
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
impl PluginDispatcher {
    /// 开始新的本地审计窗口；仅在用户显式启用 JSONL 审计时由 Pipeline 调用。
    pub(crate) fn begin_audit_trace(&self) {
        self.audit_trace_active
            .store(true, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut effects) = self.audit_effects.lock() {
            effects.clear();
        }
        self.ansi_strip_bytes_removed
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// 返回当前压缩调用中注册的插件顺序与优先级。
    pub(crate) fn audit_plugin_chain(&self) -> Vec<PluginAuditDescriptor> {
        self.plugins
            .iter()
            .map(|plugin| PluginAuditDescriptor {
                plugin_id: plugin.name().to_string(),
                priority: plugin.priority(),
            })
            .collect()
    }

    /// 取走当前压缩调用内聚合的真实插件执行贡献。
    pub(crate) fn take_audit_effects(&self) -> Vec<PluginAuditEffect> {
        self.audit_trace_active
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.audit_effects
            .lock()
            .map(|mut effects| std::mem::take(&mut *effects))
            .unwrap_or_default()
    }

    /// 取走当前压缩窗口内 ANSI 剥离累计删除的字节数，并清零。
    ///
    /// 供审计事件落盘使用；配合 `input_bytes` 可量化「裸码剥离红灯」——
    /// 输入含裸 CSI 而该值为 0 即为确定 bug（剥离结果未回写或未走 dispatcher）。
    pub(crate) fn take_ansi_strip_bytes_removed(&self) -> usize {
        self.ansi_strip_bytes_removed
            .swap(0, std::sync::atomic::Ordering::Relaxed)
    }

    /// 在审计窗口开启时，累加某插件单次执行的贡献（调用次数、输入/输出字节、是否真发生压缩）。
    /// 同一插件的多条记录会按 `plugin_id` 合并；窗口未开启时直接返回，零开销。
    fn record_plugin_audit_effect(
        &self,
        plugin_id: &str,
        priority: u8,
        input_bytes: usize,
        output_estimated_bytes: usize,
    ) {
        if !self
            .audit_trace_active
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }
        let Ok(mut effects) = self.audit_effects.lock() else {
            return;
        };
        let changed = input_bytes != output_estimated_bytes;
        if let Some(existing) = effects
            .iter_mut()
            .find(|effect| effect.plugin_id == plugin_id)
        {
            existing.invocation_count += 1;
            existing.input_bytes += input_bytes;
            existing.output_estimated_bytes += output_estimated_bytes;
            existing.changed |= changed;
            return;
        }
        effects.push(PluginAuditEffect {
            plugin_id: plugin_id.to_string(),
            priority,
            invocation_count: 1,
            input_bytes,
            output_estimated_bytes,
            changed,
        });
    }
}

#[cfg(test)]
mod strip_ansi_tests {
    use super::strip_ansi;

    /// 脱色日志：色码是脱掉 ESC 后的字面裸码，必须无条件剥干净。
    /// 这就是 302 字节 cargo 报错样本（真实输入无任何 `\x1B` 字节）的核心回归。
    #[test]
    fn test_naked_ansi_text_is_stripped() {
        assert_eq!(
            strip_ansi("[1m[91merror:[0m unexpected argument '[1m[93mcontent_analyzer[0m' found"),
            "error: unexpected argument 'content_analyzer' found"
        );
        assert_eq!(
            strip_ansi("[1m[92mUsage:[0m [1m[96mcargo.exe test[0m [36m[OPTIONS][0m ..."),
            "Usage: cargo.exe test [OPTIONS] ..."
        );
        assert_eq!(
            strip_ansi("For more information, try '[1m[96m--help[0m'."),
            "For more information, try '--help'."
        );
        // 真实 cargo 完整 Usage 行：`[-- [ARGS]...[96m]` 里的 `]` 是被着色的字面括号，
        // `[96m` 必须剥（不能因「紧跟 `]`」被保留，否则 `...][96m]` 泄漏进输出）。
        assert_eq!(
            strip_ansi("[1m[92mUsage:[0m [1m[96mcargo.exe test[0m [36m[OPTIONS][0m [36m[TESTNAME][0m [1m[96m[--[0m [36m[ARGS]...[0m[1m[96m][0m"),
            "Usage: cargo.exe test [OPTIONS] [TESTNAME] [-- [ARGS]...]"
        );
    }

    /// 以 `]` 收尾的裸码是合法字面量（路径/上下文），必须原样保留，绝不误伤。
    /// 用户 W-4 报告回归：smart_path 依赖的路径不能被 `[2m` 截断。
    #[test]
    fn test_literal_bracket_residues_preserved() {
        assert_eq!(strip_ansi("path/to/[2m]odule.rs"), "path/to/[2m]odule.rs");
        assert_eq!(strip_ansi("test [0m] ok"), "test [0m] ok");
        // warning 前缀前的字符不能被吃掉，否则 `warning:` 前缀匹配失配
        assert_eq!(
            strip_ansi("level[3m] warning: foo"),
            "level[3m] warning: foo"
        );
        // 目录名左括号 `[2m]` 与路径字符相邻 → 保留；但紧跟另一裸码的 `]` 是被着色的
        // 字面括号（cargo Usage 的 `[1m[96m][0m`），不构成合法目录，须剥。
        assert_eq!(strip_ansi("build/[3m]/lib.rs"), "build/[3m]/lib.rs");
        // 链中 `[1m`/`[2m` 后是另一裸码（着色的字面括号）必须剥，其间的 `]` 作为字面括号留
        // 下；`[3m]` 后是空格（非色码字符），是合法字面量保留——与 `build/[3m]/lib.rs` 一致。
        assert_eq!(
            strip_ansi("progress [1m][2m][3m] done"),
            "progress ]][3m] done"
        );
        // 含字母的方括号文本不会被裸码正则误判为 SGR
        assert_eq!(strip_ansi("[OPTIONS]"), "[OPTIONS]");
    }

    /// 含真实 ESC 字节（曾彩色化）的文本：真 ANSI 序列无条件剥掉，
    /// 脱色后残留的裸 CSI（`[Nm`）也一并清干净。
    #[test]
    fn test_colored_with_residue_is_cleaned() {
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m[1m[96m"), "red");
        assert_eq!(strip_ansi("\u{1b}[1m[96mtext and [0m"), "text and ");
    }

    /// 空串与不含任何色码的纯净文本保持零改动。
    #[test]
    fn test_trivial_input_unchanged() {
        assert_eq!(strip_ansi(""), "");
        assert_eq!(strip_ansi("plain"), "plain");
    }
}
