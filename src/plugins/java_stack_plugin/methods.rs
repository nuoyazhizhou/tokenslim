//! java stack plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::compression_context::CompressionContext;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use once_cell::sync::Lazy;
use regex::Regex;
use std::any::Any;
use std::borrow::Cow;
use std::collections::HashMap;

static EXCEPTION_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"Exception in thread").unwrap());
static STACK_FRAME_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*at ").unwrap());
static CAUSED_BY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\t?Caused by:").unwrap());
static SUPPRESSED_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*Suppressed:").unwrap());

/// 深层堆栈截断阈值：超过此数量的堆栈帧将被折叠
const STACK_FRAME_THRESHOLD: usize = 20;

/// 相同堆栈去重阈值：超过此数量的相同堆栈将被折叠
const DUPLICATE_STACK_THRESHOLD: usize = 2;

/// 法则 D 防失忆红线：Java 异常的「简单类名」必须以字面量保留。
///
/// 全限定名 `java.lang.StackOverflowError` 中，「包前缀」（`java.lang.`）高频重复，
/// 字典化获益大；但「简单类名」（`StackOverflowError`）是 LLM 识别异常类型的关键信号，
/// 若被整体字典化为 `$PKn`，LLM 无法得知异常本体。
///
/// 本函数把 FQN 拆成 `(pkg_prefix, simple_class)`；若 `simple_class` 属于需保留字面量
/// 的 Java 异常类（或以 `Error`/`Exception` 结尾），则包前缀可字典化、类名保留。
///
/// 返回值：`Some((pkg_prefix, simple_class))`。调用方应分别处理两部分。
/// 若类名不应拆分保留，返回 `None`，调用方按原策略完整字典化。
fn split_java_exception_fqn(class_name: &str) -> Option<(&str, &str)> {
    let dot_pos = class_name.rfind('.')?;
    let simple = &class_name[dot_pos + 1..];
    if should_preserve_class_name(simple) {
        Some((&class_name[..dot_pos], simple))
    } else {
        None
    }
}

/// Java / Kotlin / Android 运行时常见异常类名白名单。
/// 以 `Error` / `Exception` / `Throwable` 结尾的类名默认都保留。
fn should_preserve_class_name(simple: &str) -> bool {
    const KEEP: &[&str] = &[
        // java.lang 高频
        "Throwable",
        "Exception",
        "Error",
        "RuntimeException",
        "NullPointerException",
        "IllegalArgumentException",
        "IllegalStateException",
        "IllegalAccessException",
        "IllegalMonitorStateException",
        "IllegalThreadStateException",
        "IndexOutOfBoundsException",
        "ArrayIndexOutOfBoundsException",
        "StringIndexOutOfBoundsException",
        "ClassCastException",
        "ClassNotFoundException",
        "ClassFormatError",
        "NumberFormatException",
        "UnsupportedOperationException",
        "ArithmeticException",
        "ConcurrentModificationException",
        "NoSuchElementException",
        "NoSuchMethodException",
        "NoSuchFieldException",
        "NoSuchMethodError",
        "NoSuchFieldError",
        "StackOverflowError",
        "OutOfMemoryError",
        "NoClassDefFoundError",
        "IncompatibleClassChangeError",
        "VerifyError",
        "ExceptionInInitializerError",
        "AbstractMethodError",
        "AssertionError",
        "BootstrapMethodError",
        "CloneNotSupportedException",
        "InstantiationException",
        "InstantiationError",
        "InternalError",
        "InterruptedException",
        "NegativeArraySizeException",
        "LinkageError",
        "SecurityException",
        "TypeNotPresentException",
        "UnknownError",
        "UnsatisfiedLinkError",
        "UnsupportedClassVersionError",
        // java.io
        "IOException",
        "EOFException",
        "FileNotFoundException",
        "UncheckedIOException",
        "InvalidObjectException",
        "NotSerializableException",
        "StreamCorruptedException",
        // java.sql
        "SQLException",
        "SQLTimeoutException",
        "SQLSyntaxErrorException",
        // 并发
        "CancellationException",
        "CompletionException",
        "ExecutionException",
        "RejectedExecutionException",
        "TimeoutException",
    ];
    if KEEP.contains(&simple) {
        return true;
    }
    // 后缀兜底：*Error / *Exception / *Throwable 命名均保留字面量
    simple.ends_with("Error") || simple.ends_with("Exception") || simple.ends_with("Throwable")
}

/// 编码异常类 FQN 为 compact token：若简单类名在白名单，则「包名字典化 + 类名字面量」；
/// 否则整体字典化保持原行为。
/// 返回的字符串永远不会吞掉用户可读的简单类名。
fn encode_exception_class_name(class_name: &str, dict: &mut DictionaryEngine) -> String {
    if let Some((pkg_prefix, simple)) = split_java_exception_fqn(class_name) {
        if pkg_prefix.is_empty() {
            simple.to_string()
        } else {
            let pkg_token = dict.add_package(pkg_prefix);
            format!("{}.{}", pkg_token, simple)
        }
    } else {
        dict.add_package(class_name)
    }
}

/// 提取异常类型（简单类名）从异常行
/// 例如：`java.lang.NullPointerException: message` -> `NullPointerException`
#[tracing::instrument(level = "debug", skip_all)]
fn extract_exception_type(line: &str) -> Option<String> {
    if let Some(pos) = line.find("java.") {
        let class_part = &line[pos..];
        let end_pos = class_part.find(':').unwrap_or(class_part.len());
        let class_name = &class_part[..end_pos];
        if let Some(dot_pos) = class_name.rfind('.') {
            return Some(class_name[dot_pos + 1..].to_string());
        }
    }
    None
}

/// 去重相同堆栈：检测并折叠重复的异常堆栈
#[tracing::instrument(level = "debug", skip_all)]
fn dedupe_same_stack(text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut result = String::new();
    let mut stack_hashes: HashMap<u64, usize> = HashMap::new();
    let mut current_stack = String::new();
    let mut in_stack = false;

    for line in text.lines() {
        if EXCEPTION_RE.is_match(line) {
            // 新异常开始，保存前一个堆栈
            if !current_stack.is_empty() {
                let mut hasher = DefaultHasher::new();
                current_stack.hash(&mut hasher);
                let hash = hasher.finish();
                let count = stack_hashes.entry(hash).or_insert(0);
                *count += 1;

                if *count == 1 {
                    result.push_str(&current_stack);
                } else if *count == DUPLICATE_STACK_THRESHOLD {
                    // 第二次出现时，添加去重标记
                    result.push_str(&format!(
                        "[DUPLICATE] Same exception occurred {} times (first shown)\n",
                        count
                    ));
                }
            }
            current_stack.clear();
            current_stack.push_str(line);
            current_stack.push('\n');
            in_stack = true;
        } else if in_stack && (STACK_FRAME_RE.is_match(line) || CAUSED_BY_RE.is_match(line)) {
            current_stack.push_str(line);
            current_stack.push('\n');
        } else if in_stack && line.trim().is_empty() {
            // 堆栈结束
            in_stack = false;
        } else if in_stack {
            current_stack.push_str(line);
            current_stack.push('\n');
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    // 处理最后一个堆栈
    if !current_stack.is_empty() {
        let mut hasher = DefaultHasher::new();
        current_stack.hash(&mut hasher);
        let hash = hasher.finish();
        let count = stack_hashes.entry(hash).or_insert(0);
        *count += 1;
        if *count == 1 {
            result.push_str(&current_stack);
        } else if *count == DUPLICATE_STACK_THRESHOLD {
            result.push_str(&format!(
                "[DUPLICATE] Same exception occurred {} times (first shown)\n",
                count
            ));
        }
    }

    result
}

/// 截断深层堆栈：若堆栈帧超过阈值，只保留前 N 帧并添加摘要
#[tracing::instrument(level = "debug", skip_all)]
fn truncate_deep_stack(text: &str) -> String {
    let mut result = String::new();
    let mut frame_count = 0;
    let mut in_stack = false;

    for line in text.lines() {
        if EXCEPTION_RE.is_match(line) {
            result.push_str(line);
            result.push('\n');
            in_stack = true;
            frame_count = 0;
        } else if in_stack && STACK_FRAME_RE.is_match(line) {
            frame_count += 1;
            if frame_count <= STACK_FRAME_THRESHOLD {
                result.push_str(line);
                result.push('\n');
            } else if frame_count == STACK_FRAME_THRESHOLD + 1 {
                // 第一次超过阈值时，添加摘要
                let total_frames = text.lines().filter(|l| STACK_FRAME_RE.is_match(l)).count();
                result.push_str(&format!(
                    "[STACK] {} frames (first {} shown, {} omitted)\n",
                    total_frames,
                    STACK_FRAME_THRESHOLD,
                    total_frames - STACK_FRAME_THRESHOLD
                ));
            }
        } else if in_stack && (CAUSED_BY_RE.is_match(line) || line.trim().is_empty()) {
            in_stack = false;
            result.push_str(line);
            result.push('\n');
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

/// 提取异常摘要：统计所有异常类型并生成摘要
#[tracing::instrument(level = "debug", skip_all)]
fn extract_exception_summary(text: &str) -> String {
    let mut result = String::new();
    let mut exception_counts: HashMap<String, usize> = HashMap::new();

    for line in text.lines() {
        if EXCEPTION_RE.is_match(line) {
            if let Some(exc_type) = extract_exception_type(line) {
                *exception_counts.entry(exc_type).or_insert(0) += 1;
            }
        }
        result.push_str(line);
        result.push('\n');
    }

    // 生成摘要
    if !exception_counts.is_empty() {
        let total = exception_counts.values().sum::<usize>();
        let mut summary = format!("[SUMMARY] {} exceptions: ", total);
        let mut parts: Vec<_> = exception_counts.iter().collect();
        parts.sort_by_key(|&(_, count)| std::cmp::Reverse(*count));

        for (i, (exc_type, count)) in parts.iter().enumerate() {
            if i > 0 {
                summary.push_str(", ");
            }
            summary.push_str(&format!("{} {}", count, exc_type));
        }
        summary.push('\n');
        result.push_str(&summary);
    }

    result
}

/// 压缩 Suppressed 异常：折叠多个 Suppressed 异常
#[tracing::instrument(level = "debug", skip_all)]
fn compress_suppressed_exceptions(text: &str) -> String {
    let mut result = String::new();
    let mut suppressed_count = 0;
    let mut in_suppressed = false;

    for line in text.lines() {
        if SUPPRESSED_RE.is_match(line) {
            if suppressed_count == 0 {
                // 第一个 Suppressed 异常，保留
                result.push_str(line);
                result.push('\n');
            } else if suppressed_count == 1 {
                // 第二个 Suppressed 异常，添加摘要并跳过后续
                result.push_str(&format!(
                    "[SUPPRESSED] {} exceptions (details omitted)\n",
                    suppressed_count + 1
                ));
            }
            suppressed_count += 1;
            in_suppressed = true;
        } else if in_suppressed && (STACK_FRAME_RE.is_match(line) || line.trim().is_empty()) {
            if suppressed_count <= 1 {
                result.push_str(line);
                result.push('\n');
            }
            if line.trim().is_empty() {
                in_suppressed = false;
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

impl JavaStackPlugin {
    pub fn new() -> Self {
        Self {
            name: "java_stack",
            priority: 86,
            config: JavaStackConfig::default(),
        }
    }
}

impl Plugin for JavaStackPlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        if text.contains("at ") && (text.contains(".java:") || text.contains(".kt:")) {
            return Some(0.9);
        }
        if text.contains("Exception in thread") || text.contains("Caused by:") {
            return Some(0.95);
        }
        None
    }

    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        // 应用新的压缩功能
        let text_after_dedup = dedupe_same_stack(text);
        let text_after_truncate = truncate_deep_stack(&text_after_dedup);
        let text_after_suppressed = compress_suppressed_exceptions(&text_after_truncate);
        let text_with_summary = extract_exception_summary(&text_after_suppressed);

        let mut tokens = Vec::new();

        for line in text_with_summary.lines() {
            if EXCEPTION_RE.is_match(line) {
                // Exception in thread "main" java.lang.NullPointerException: ...
                if let Some(pos) = line.find("java.") {
                    let class_part = &line[pos..];
                    let end_pos = class_part.find(':').unwrap_or(class_part.len());
                    let class_name = &class_part[..end_pos];
                    let token = encode_exception_class_name(class_name, dict_engine);
                    tokens.push(Token::Text(Cow::Owned(format!(
                        "$JEX|{}|{}|{}\n",
                        &line[..pos],
                        token,
                        &class_part[end_pos..]
                    ))));
                } else {
                    tokens.push(Token::Text(Cow::Owned(format!("{}\n", line))));
                }
            } else if STACK_FRAME_RE.is_match(line) {
                // at com.pkg.Class.method(File.java:123)
                let trimmed = line.trim_start();
                let content = &trimmed[3..]; // skip "at "
                if let Some(paren_pos) = content.find('(') {
                    let full_method = &content[..paren_pos];
                    if let Some(last_dot) = full_method.rfind('.') {
                        if let Some(second_last_dot) = full_method[..last_dot].rfind('.') {
                            let pkg = &full_method[..second_last_dot];
                            let class_method = &full_method[second_last_dot + 1..];
                            let token = dict_engine.add_package(pkg);
                            tokens.push(Token::Text(Cow::Owned(format!(
                                "$JST|{}|{}|{}\n",
                                token,
                                class_method,
                                &content[paren_pos..]
                            ))));
                        } else {
                            tokens.push(Token::Text(Cow::Owned(format!("{}\n", line))));
                        }
                    } else {
                        tokens.push(Token::Text(Cow::Owned(format!("{}\n", line))));
                    }
                } else {
                    tokens.push(Token::Text(Cow::Owned(format!("{}\n", line))));
                }
            } else if CAUSED_BY_RE.is_match(line) {
                if let Some(pos) = line.find("Caused by: ") {
                    let class_part = &line[pos + 11..];
                    let end_pos = class_part.find(':').unwrap_or(class_part.len());
                    let class_name = &class_part[..end_pos];
                    let token = encode_exception_class_name(class_name, dict_engine);
                    tokens.push(Token::Text(Cow::Owned(format!(
                        "$JCB|{}|{}\n",
                        token,
                        &class_part[end_pos..]
                    ))));
                } else {
                    tokens.push(Token::Text(Cow::Owned(format!("{}\n", line))));
                }
            } else {
                tokens.push(Token::Text(Cow::Owned(format!("{}\n", line))));
            }
        }

        // 法则 A ROI 门控：小样本或无命中行场景下，tokens 尾部 IR 头可能反而扩张，
        // 整段回退原文。参考 `docs/prompts/non_vcs_classical_prompts.md` § 1.3。
        let compacted: String = tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.as_ref(),
                _ => "",
            })
            .collect();
        let final_text = crate::core::utils::roi::prefer_non_expanding(text, compacted);

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(final_text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    fn compress_with_context<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
        context: &mut CompressionContext,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        // 应用新的压缩功能
        let text_after_dedup = dedupe_same_stack(text);
        let text_after_truncate = truncate_deep_stack(&text_after_dedup);
        let text_after_suppressed = compress_suppressed_exceptions(&text_after_truncate);
        let text_with_summary = extract_exception_summary(&text_after_suppressed);

        let mut tokens: Vec<Token<'a>> = Vec::new();

        for raw_line in text_with_summary.lines() {
            let normalized = context.convert_line(Cow::Borrowed(raw_line));
            let line = normalized.as_ref();

            if EXCEPTION_RE.is_match(line) {
                if let Some(pos) = line.find("java.") {
                    let class_part = &line[pos..];
                    let end_pos = class_part.find(':').unwrap_or(class_part.len());
                    let class_name = &class_part[..end_pos];
                    let token = encode_exception_class_name(class_name, dict_engine);
                    let out = bumpalo::format!(
                        in arena,
                        "$JEX|{}|{}|{}\n",
                        &line[..pos],
                        token,
                        &class_part[end_pos..]
                    );
                    tokens.push(Token::Text(Cow::Borrowed(out.into_bump_str())));
                } else {
                    let out = bumpalo::format!(in arena, "{}\n", line);
                    tokens.push(Token::Text(Cow::Borrowed(out.into_bump_str())));
                }
            } else if STACK_FRAME_RE.is_match(line) {
                let trimmed = line.trim_start();
                let content = &trimmed[3..];
                if let Some(paren_pos) = content.find('(') {
                    let full_method = &content[..paren_pos];
                    if let Some(last_dot) = full_method.rfind('.') {
                        if let Some(second_last_dot) = full_method[..last_dot].rfind('.') {
                            let pkg = &full_method[..second_last_dot];
                            let class_method = &full_method[second_last_dot + 1..];
                            let token = dict_engine.add_package(pkg);
                            let out = bumpalo::format!(
                                in arena,
                                "$JST|{}|{}|{}\n",
                                token,
                                class_method,
                                &content[paren_pos..]
                            );
                            tokens.push(Token::Text(Cow::Borrowed(out.into_bump_str())));
                        } else {
                            let out = bumpalo::format!(in arena, "{}\n", line);
                            tokens.push(Token::Text(Cow::Borrowed(out.into_bump_str())));
                        }
                    } else {
                        let out = bumpalo::format!(in arena, "{}\n", line);
                        tokens.push(Token::Text(Cow::Borrowed(out.into_bump_str())));
                    }
                } else {
                    let out = bumpalo::format!(in arena, "{}\n", line);
                    tokens.push(Token::Text(Cow::Borrowed(out.into_bump_str())));
                }
            } else if CAUSED_BY_RE.is_match(line) {
                if let Some(pos) = line.find("Caused by: ") {
                    let class_part = &line[pos + 11..];
                    let end_pos = class_part.find(':').unwrap_or(class_part.len());
                    let class_name = &class_part[..end_pos];
                    let token = encode_exception_class_name(class_name, dict_engine);
                    let out =
                        bumpalo::format!(in arena, "$JCB|{}|{}\n", token, &class_part[end_pos..]);
                    tokens.push(Token::Text(Cow::Borrowed(out.into_bump_str())));
                } else {
                    let out = bumpalo::format!(in arena, "{}\n", line);
                    tokens.push(Token::Text(Cow::Borrowed(out.into_bump_str())));
                }
            } else {
                let out = bumpalo::format!(in arena, "{}\n", line);
                tokens.push(Token::Text(Cow::Borrowed(out.into_bump_str())));
            }
        }

        // 法则 A ROI 门控：参考 `docs/prompts/non_vcs_classical_prompts.md` § 1.3。
        let compacted: String = tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.as_ref(),
                _ => "",
            })
            .collect();
        let final_text = crate::core::utils::roi::prefer_non_expanding(text, compacted);
        let final_in_arena = arena.alloc_str(&final_text);

        CompressResult {
            tokens: vec![Token::Text(Cow::Borrowed(final_in_arena))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        let mut result = String::new();
        for line in compressed.lines() {
            if line.starts_with("$JEX|") {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 4 {
                    let prefix = parts[1];
                    let class = dict.resolve_or_self(parts[2]);
                    let suffix = parts[3];
                    result.push_str(&format!("{}{}{}\n", prefix, class, suffix));
                    continue;
                }
            } else if line.starts_with("$JST|") {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 4 {
                    let pkg = dict.resolve_or_self(parts[1]);
                    let class_method = parts[2];
                    let rest = parts[3];
                    result.push_str(&format!("\tat {}.{}{}\n", pkg, class_method, rest));
                    continue;
                }
            } else if line.starts_with("$JCB|") {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 3 {
                    let class = dict.resolve_or_self(parts[1]);
                    let suffix = parts[2];
                    result.push_str(&format!("Caused by: {}{}\n", class, suffix));
                    continue;
                }
            }
            result.push_str(line);
            result.push('\n');
        }
        result
    }

    fn load_config(&mut self, config: &dyn Any) -> Result<(), String> {
        if let Some(new_config) = config.downcast_ref::<JavaStackConfig>() {
            self.config = new_config.clone();
            return Ok(());
        }
        Err("Invalid config type".to_string())
    }
}

impl Clone for JavaStackPlugin {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            priority: self.priority,
            config: self.config.clone(),
        }
    }
}
