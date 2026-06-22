//! gcc log plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::compression_context::CompressionContext;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::path_optimizer::methods::{
    append_optimized_inline_path_dictionary_with_options, PathDictionaryOptions,
};
use crate::core::path_optimizer::token_boundary::replace_path_token_boundary;
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use aho_corasick::AhoCorasick;
use bumpalo::Bump;
use once_cell::sync::Lazy;
use regex::Regex;
use std::any::Any;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

static ERROR_MARKERS: &[&str] = &["error:", "warning:", "note:", "fatal error:"];
static AC_MARKERS: Lazy<AhoCorasick> = Lazy::new(|| AhoCorasick::new(ERROR_MARKERS).unwrap());
static NINJA_PROGRESS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\[(?P<step>\d+/\d+)\]\s+(?P<msg>.+)$").unwrap());
static PATH_TOKEN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\$P\d+").unwrap());
static GCC_DIAGNOSTIC_CONTEXT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(?:\d+\s+\|.*|\|.*)$").unwrap());
static LINKER_SOURCE_REF_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"^(?P<file>.+):\((?P<offset>[^)]+)\): undefined reference to [`'](?P<sym>[^`']+)[`']"#,
    )
    .unwrap()
});

/// 警告折叠阈值：超过此数量的相同警告将被折叠
const WARNING_FOLD_THRESHOLD: usize = 3;

/// 构建统计信息
#[derive(Debug, Default)]
struct BuildStats {
    /// 错误计数
    errors: usize,
    /// 警告计数（按警告类型分组）
    warnings: HashMap<String, Vec<usize>>, // warning_type -> line_numbers
    /// 注释计数
    notes: usize,
    /// 链接器错误计数
    linker_errors: usize,
}

impl BuildStats {
    /// 创建新的统计对象
    #[tracing::instrument(level = "trace", skip_all)]
    fn new() -> Self {
        Self::default()
    }

    /// 分类一行输出
    #[tracing::instrument(level = "trace", skip_all)]
    fn classify(&mut self, line: &str, line_num: usize) {
        // 链接器错误：包含 undefined reference 的行
        if line.contains("undefined reference") {
            self.linker_errors += 1;
            return;
        }

        if line.contains("error:") && !line.contains("warning:") {
            self.errors += 1;
        } else if line.contains(" Error ")
            || line.ends_with(" Error 1")
            || line.ends_with(" Error 2")
        {
            self.errors += 1;
        } else if line.contains("warning:") {
            // 按 warning 类型 + 具体消息聚合，避免少数变量被多数同类 warning 淹没。
            if let Some(warning_type) = gcc_warning_signature(line) {
                self.warnings
                    .entry(warning_type)
                    .or_insert_with(Vec::new)
                    .push(line_num);
            } else {
                // 未知警告类型
                self.warnings
                    .entry("[unknown]".to_string())
                    .or_insert_with(Vec::new)
                    .push(line_num);
            }
        } else if line.contains("note:") {
            self.notes += 1;
        }
    }

    /// 检查是否有问题需要报告
    #[tracing::instrument(level = "trace", skip_all)]
    fn has_issues(&self) -> bool {
        self.errors > 0 || !self.warnings.is_empty() || self.linker_errors > 0
    }

    /// 生成构建摘要
    #[tracing::instrument(level = "trace", skip_all)]
    fn generate_summary(&self) -> Option<String> {
        if !self.has_issues() {
            return None;
        }

        let mut parts = Vec::new();

        let total_errors = self.errors + self.linker_errors;
        if total_errors > 0 {
            parts.push(format!("{} errors", total_errors));
        }

        let total_warnings: usize = self.warnings.values().map(|v| v.len()).sum();
        if total_warnings > 0 {
            parts.push(format!("{} warnings", total_warnings));
        }

        if self.notes > 0 {
            parts.push(format!("{} notes", self.notes));
        }

        if parts.is_empty() {
            None
        } else {
            Some(format!("[SUMMARY] {}", parts.join(", ")))
        }
    }

    /// 检查某个警告是否应该被折叠
    #[tracing::instrument(level = "trace", skip_all)]
    fn should_fold_warning(&self, line_num: usize, warning_type: &str) -> bool {
        if let Some(lines) = self.warnings.get(warning_type) {
            if lines.len() > WARNING_FOLD_THRESHOLD {
                // 只保留前 WARNING_FOLD_THRESHOLD 个
                let pos = lines.iter().position(|&n| n == line_num);
                if let Some(idx) = pos {
                    return idx >= WARNING_FOLD_THRESHOLD;
                }
            }
        }
        false
    }

    /// 生成警告折叠摘要
    #[tracing::instrument(level = "trace", skip_all)]
    fn generate_fold_summary(&self, warning_type: &str) -> Option<String> {
        if let Some(lines) = self.warnings.get(warning_type) {
            let count = lines.len();
            if count > WARNING_FOLD_THRESHOLD {
                let suppressed = count - WARNING_FOLD_THRESHOLD;
                return Some(format!(
                    "[WARNING] Same warning {} repeated {} times (first {} shown, {} suppressed)",
                    warning_type, count, WARNING_FOLD_THRESHOLD, suppressed
                ));
            }
        }
        None
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn finalize_gcc_compaction(
    text: &str,
    compacted: String,
    dict_engine: &DictionaryEngine,
) -> String {
    let path_options = PathDictionaryOptions {
        min_footer_token_uses: 1,
        ..PathDictionaryOptions::default()
    };
    let compacted_with_paths = append_optimized_inline_path_dictionary_with_options(
        &compacted,
        dict_engine,
        &path_options,
    );
    let final_with_paths =
        crate::core::utils::roi::prefer_non_expanding(text, compacted_with_paths);
    if final_with_paths != text || !PATH_TOKEN_RE.is_match(&compacted) {
        return final_with_paths;
    }

    let expanded = expand_gcc_path_tokens(&compacted, dict_engine);
    crate::core::utils::roi::prefer_non_expanding(text, expanded)
}

#[tracing::instrument(level = "debug", skip_all)]
fn expand_gcc_path_tokens(text: &str, dict_engine: &DictionaryEngine) -> String {
    let dict = dict_engine.snapshot();
    let mut mappings = dict
        .paths
        .iter()
        .map(|(token, raw_path)| (token.clone(), dict.resolve_or_self(raw_path)))
        .collect::<Vec<_>>();
    mappings.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    let mut out = text.to_string();
    for (token, raw_path) in mappings {
        out = replace_path_token_boundary(&out, &token, &raw_path);
    }
    out
}

#[tracing::instrument(level = "trace", skip_all)]
fn is_gcc_diagnostic_context_line(line: &str) -> bool {
    GCC_DIAGNOSTIC_CONTEXT_RE.is_match(line)
}

#[tracing::instrument(level = "trace", skip_all)]
fn gcc_warning_signature(line: &str) -> Option<String> {
    let warning_msg = line.split("warning:").nth(1)?.trim();
    let warning_type = if let Some(start) = line.find("[-W") {
        if let Some(end) = line[start..].find(']') {
            &line[start..start + end + 1]
        } else {
            "[unknown]"
        }
    } else {
        "[unknown]"
    };
    let message = warning_msg
        .split("[-W")
        .next()
        .unwrap_or(warning_msg)
        .trim();
    Some(format!("{warning_type} {message}"))
}

impl GccLogPlugin {
    /// 实例化并返回该插件的默认配置对象。
    pub fn new() -> Self {
        let gcc_pattern = Regex::new(r"gcc|g\+\+").unwrap();
        let make_pattern = Regex::new(
            r"(?:^|\]\s*)make\[(?P<lv>\d+)\]: (?:Entering|Leaving) directory '(?P<dir>.*)'",
        )
        .unwrap();
        let cmake_pattern = Regex::new(r"(?:^|\]\s*)\[\s*\d+%\s*\]").unwrap();
        let compile_cmd_pattern = Regex::new(r"(?:^|\]\s*)[ \t]*-c\s+").unwrap();
        let error_pattern = Regex::new(r"(?:^|\]\s*)(?P<file>[^:\n\[\]]+):(?P<line>\d+):(?P<col>\d+):\s*(?P<lvl>error|warning|note|fatal error): (?P<msg>.*)$").unwrap();

        GccLogPlugin {
            name: "gcc_log",
            priority: 150,
            gcc_pattern: Arc::new(gcc_pattern),
            make_pattern: Arc::new(make_pattern),
            cmake_pattern: Arc::new(cmake_pattern),
            compile_cmd_pattern: Arc::new(compile_cmd_pattern),
            error_pattern: Arc::new(error_pattern),
            config: None,
        }
    }

    fn compress_line_standardized<'a>(
        &self,
        line: &'a str,
        dict: &mut DictionaryEngine,
        arena: &'a Bump,
    ) -> Cow<'a, str> {
        if line.trim().is_empty() {
            return Cow::Borrowed("");
        }

        if let Some(compacted) = self.compress_ninja_line(line, dict, arena) {
            return compacted;
        }

        if let Some(compacted) = self.compress_ctest_line(line, arena) {
            return compacted;
        }

        let line_to_check = if line.starts_with('[') {
            if let Some(idx) = line.find(']') {
                line[idx + 1..].trim_start()
            } else {
                line
            }
        } else {
            line
        };

        if let Some(compacted) = self.compress_cmake_line(line_to_check, dict, arena) {
            return compacted;
        }

        // 链接器输出压缩：/usr/bin/ld: file.o: in function 'func': undefined reference to 'symbol'
        if line_to_check.contains("/usr/bin/ld:") && line_to_check.contains("undefined reference") {
            return self.compress_linker_line(line, dict, arena);
        }
        if line_to_check.contains("undefined reference") {
            if let Some(compacted) = self.compress_linker_source_ref(line_to_check, dict, arena) {
                return compacted;
            }
        }

        // Use Aho-Corasick for fast marker detection instead of multiple line.contains()
        if AC_MARKERS.find(line_to_check).is_some() {
            if let Some(caps) = self.error_pattern.captures(line) {
                let file = dict.add_path_layered(&caps["file"]);
                let lvl = &caps["lvl"];
                let msg = &caps["msg"];
                let clean_msg = self.replace_paths_in_text(msg, dict, arena);

                // 提取前缀（仅当行以 '[' 开头时才提取时间戳前缀）
                let prefix = if line.starts_with('[') {
                    if let Some(idx) = line.find(']') {
                        &line[..idx + 1]
                    } else {
                        ""
                    }
                } else {
                    ""
                };

                // 只输出压缩格式，不包含前缀（前缀用于处理带时间戳的日志）
                let formatted = if prefix.is_empty() {
                    bumpalo::format!(in arena, "$GCC {}:{}:{} {} {}", file, &caps["line"], &caps["col"], lvl, clean_msg)
                } else {
                    bumpalo::format!(in arena, "{} $GCC {}:{}:{} {} {}", prefix, file, &caps["line"], &caps["col"], lvl, clean_msg)
                };

                return Cow::Borrowed(formatted.into_bump_str());
            }
        }

        if line_to_check.starts_with("make[") {
            if let Some(caps) = self.make_pattern.captures(line) {
                let lv = &caps["lv"];
                let dir = dict.add_path_layered(&caps["dir"]);
                let msg = if line_to_check.contains("Entering") {
                    "Entering"
                } else {
                    "Leaving"
                };
                let prefix = if let Some(idx) = line.find(']') {
                    &line[..idx + 1]
                } else {
                    ""
                };
                let formatted =
                    bumpalo::format!(in arena, "{} $MAKE {} {} {}", prefix, lv, msg, dir);
                return Cow::Borrowed(formatted.into_bump_str());
            }
        }

        if line_to_check.starts_with("skipping ") {
            let path = &line_to_check[9..];
            let token = dict.add_path_layered(path);
            let prefix = if let Some(idx) = line.find(']') {
                &line[..idx + 1]
            } else {
                ""
            };
            let formatted = bumpalo::format!(in arena, "{} $SKIP {}", prefix, token);
            return Cow::Borrowed(formatted.into_bump_str());
        }

        let p = self.replace_paths_in_text(line, dict, arena);
        let m = self.replace_macros_in_text(&p, dict, arena);

        match m {
            Cow::Borrowed(s) => Cow::Borrowed(arena.alloc_str(s)),
            Cow::Owned(s) => Cow::Borrowed(arena.alloc_str(&s)),
        }
    }

    /// 压缩 CMake configure/generate 阶段输出。
    #[tracing::instrument(level = "trace", skip_all)]
    fn compress_cmake_line<'a>(
        &self,
        line: &'a str,
        dict: &mut DictionaryEngine,
        arena: &'a Bump,
    ) -> Option<Cow<'a, str>> {
        let trimmed = line.trim();
        if !trimmed.starts_with("-- ") {
            return None;
        }
        let msg = trimmed.trim_start_matches("-- ").trim();
        let compact = if msg.contains("compiler identification") {
            let lang = if msg.starts_with("The CXX ") {
                "CXX"
            } else {
                "C"
            };
            let value = msg.split(" is ").nth(1).unwrap_or(msg).trim();
            format!("$CMAKE {lang}={value}")
        } else if msg.starts_with("Detecting ")
            && (msg.ends_with(" - done") || msg.ends_with(" - skipped"))
        {
            let status = if msg.ends_with(" - done") {
                "ok"
            } else {
                "skip"
            };
            let subject = msg
                .trim_start_matches("Detecting ")
                .trim_end_matches(" - done")
                .trim_end_matches(" - skipped");
            format!("$CMAKE detect:{status} {subject}")
        } else if msg.starts_with("Check for working ") && msg.ends_with(" - skipped") {
            "$CMAKE check:skip compiler".to_string()
        } else if msg == "Configuring done" {
            "$CMAKE configured".to_string()
        } else if msg == "Generating done" {
            "$CMAKE generated".to_string()
        } else if let Some(path) = msg.strip_prefix("Build files have been written to: ") {
            format!("$CMAKE build_dir {}", dict.add_path_layered(path))
        } else {
            return None;
        };
        Some(Cow::Borrowed(arena.alloc_str(&compact)))
    }

    /// 压缩 Ninja 进度行，保留进度、动作和目标。
    #[tracing::instrument(level = "trace", skip_all)]
    fn compress_ninja_line<'a>(
        &self,
        line: &'a str,
        dict: &mut DictionaryEngine,
        arena: &'a Bump,
    ) -> Option<Cow<'a, str>> {
        let caps = NINJA_PROGRESS_RE.captures(line.trim())?;
        let step = caps.name("step")?.as_str();
        let msg = caps.name("msg")?.as_str();
        let compact = if let Some(rest) = msg.strip_prefix("Building CXX object ") {
            format!("$NINJA {step} CXX {}", dict.add_path_layered(rest))
        } else if let Some(rest) = msg.strip_prefix("Building C object ") {
            format!("$NINJA {step} CC {}", dict.add_path_layered(rest))
        } else if let Some(rest) = msg.strip_prefix("Linking CXX executable ") {
            format!("$NINJA {step} LINK {}", dict.add_path_layered(rest))
        } else if let Some(rest) = msg.strip_prefix("Running custom command ") {
            format!("$NINJA {step} CUSTOM {}", dict.add_path_layered(rest))
        } else {
            format!("$NINJA {step} {msg}")
        };
        Some(Cow::Borrowed(arena.alloc_str(&compact)))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn compress_ctest_line<'a>(&self, line: &'a str, arena: &'a Bump) -> Option<Cow<'a, str>> {
        let trimmed = line.trim();
        if trimmed == "The following tests FAILED:" {
            return Some(Cow::Borrowed(arena.alloc_str("$CTEST failed:")));
        }
        if trimmed.contains("Errors while running CTest") {
            return Some(Cow::Borrowed(arena.alloc_str("$CTEST error")));
        }

        if let Some((id_part, rest)) = trimmed.split_once(" - ") {
            let id = id_part.trim();
            if !id.is_empty() && id.chars().all(|ch| ch.is_ascii_digit()) {
                let name = rest
                    .split('(')
                    .next()
                    .unwrap_or(rest)
                    .trim()
                    .replace(' ', "_");
                let status = if rest.contains("Timeout") {
                    "timeout"
                } else if rest.contains("Failed") {
                    "fail"
                } else {
                    return None;
                };
                let compact = format!("$CTEST {status} {id} {name}");
                return Some(Cow::Borrowed(arena.alloc_str(&compact)));
            }
        }

        if !(trimmed.contains("Test #")
            || trimmed.contains(" Test ")
            || trimmed.starts_with("Start "))
        {
            return None;
        }

        let status = if trimmed.contains("***Failed") {
            "fail"
        } else if trimmed.contains("***Timeout") || trimmed.contains("Timeout") {
            "timeout"
        } else if trimmed.contains(" Passed ") || trimmed.ends_with(" Passed") {
            "pass"
        } else if trimmed.starts_with("Start ") {
            "start"
        } else {
            return None;
        };
        let compact = if let Some(hash_idx) = trimmed.find("Test #") {
            let rest = &trimmed[hash_idx + "Test #".len()..];
            let id = rest.split(':').next().unwrap_or("").trim();
            let name = rest
                .split(':')
                .nth(1)
                .unwrap_or(rest)
                .split("...")
                .next()
                .unwrap_or(rest)
                .trim()
                .replace(' ', "_");
            format!("$CTEST {status} #{id} {name}")
        } else {
            format!("$CTEST {status} {}", trimmed.replace(' ', "_"))
        };
        Some(Cow::Borrowed(arena.alloc_str(&compact)))
    }

    /// 压缩链接器输出行
    #[tracing::instrument(level = "trace", skip_all)]
    fn compress_linker_line<'a>(
        &self,
        line: &'a str,
        dict: &mut DictionaryEngine,
        arena: &'a Bump,
    ) -> Cow<'a, str> {
        // 匹配：/usr/bin/ld: file.o: in function 'func': undefined reference to 'symbol'
        // 压缩为：$LD file.o:func undefined reference to 'symbol'

        if let Some(ld_pos) = line.find("/usr/bin/ld:") {
            let after_ld = &line[ld_pos + 12..].trim_start();

            // 提取文件名
            if let Some(colon_pos) = after_ld.find(':') {
                let file = &after_ld[..colon_pos].trim();
                let rest = &after_ld[colon_pos + 1..].trim();

                // 提取函数名（如果有）
                let (func, msg) = if rest.starts_with("in function") {
                    if let Some(quote_start) = rest.find('\'') {
                        if let Some(quote_end) = rest[quote_start + 1..].find('\'') {
                            let func_name = &rest[quote_start + 1..quote_start + 1 + quote_end];
                            let after_func = &rest[quote_start + 1 + quote_end + 1..].trim();
                            // 跳过冒号
                            let msg_part = if after_func.starts_with(':') {
                                after_func[1..].trim()
                            } else {
                                after_func
                            };
                            (Some(func_name), msg_part)
                        } else {
                            (None, *rest)
                        }
                    } else {
                        (None, *rest)
                    }
                } else {
                    (None, *rest)
                };

                // 压缩路径
                let file_token = dict.add_path_layered(file);

                // 组装输出
                let formatted = if let Some(f) = func {
                    bumpalo::format!(in arena, "$LD {}:{} {}", file_token, f, msg)
                } else {
                    bumpalo::format!(in arena, "$LD {} {}", file_token, msg)
                };

                return Cow::Borrowed(formatted.into_bump_str());
            }
        }

        // 如果解析失败，返回原行
        Cow::Borrowed(line)
    }

    /// 压缩链接器源码偏移行：src.c:(.text+0x1a): undefined reference to `sym`
    #[tracing::instrument(level = "trace", skip_all)]
    fn compress_linker_source_ref<'a>(
        &self,
        line: &'a str,
        dict: &mut DictionaryEngine,
        arena: &'a Bump,
    ) -> Option<Cow<'a, str>> {
        let caps = LINKER_SOURCE_REF_RE.captures(line)?;
        let file = dict.add_path_layered(caps.name("file")?.as_str());
        let offset = caps.name("offset")?.as_str();
        let sym = caps.name("sym")?.as_str();
        let formatted = bumpalo::format!(in arena, "$LD {} {} undef {}", file, offset, sym);
        Some(Cow::Borrowed(formatted.into_bump_str()))
    }

    fn replace_paths_in_text<'a>(
        &self,
        text: &'a str,
        dict_engine: &mut DictionaryEngine,
        arena: &'a Bump,
    ) -> Cow<'a, str> {
        // Use memchr to find potential path separators efficiently (SIMD)
        if memchr::memchr3(b'/', b'\\', b'-', text.as_bytes()).is_none() {
            return Cow::Borrowed(text);
        }

        use std::cell::RefCell;
        thread_local! {
            static PATH_RE: RefCell<Regex> = RefCell::new(
                Regex::new(r#"(?P<pre>-[IL]|[ \t("'\(])(?P<path>(?:[a-zA-Z]:\\|[/.])[\w\.\-\+_~=@#]+(?:[/\\][\w\.\-\+_~=@#]+)*)"#).unwrap()
            );
        }

        let mut replaced = false;
        let result = PATH_RE.with(|re| {
            let re = re.borrow();
            re.replace_all(text, |caps: &regex::Captures| {
                replaced = true;
                let prefix = caps.name("pre").map(|m| m.as_str()).unwrap_or("");
                let path = caps.name("path").map(|m| m.as_str()).unwrap_or("");
                if path.contains('/')
                    || path.contains('\\')
                    || path.contains(":\\")
                    || matches!(prefix, "-I" | "-L")
                {
                    let token = dict_engine.add_path_layered(path);
                    let skeleton = dict_engine.skeletonize_path(&token);
                    format!("{}{}", prefix, skeleton)
                } else {
                    caps.get(0).unwrap().as_str().to_string()
                }
            })
            .into_owned()
        });

        if replaced {
            Cow::Borrowed(arena.alloc_str(&result))
        } else {
            Cow::Borrowed(text)
        }
    }

    fn replace_macros_in_text<'a>(
        &self,
        text: &'a str,
        _dict_engine: &mut DictionaryEngine,
        arena: &'a Bump,
    ) -> Cow<'a, str> {
        let _ = arena;
        Cow::Borrowed(text)
    }
}

impl Plugin for GccLogPlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        let mut score: f32 = 0.0;
        if self.gcc_pattern.is_match(text) {
            score += 0.8;
        }
        if self.make_pattern.is_match(text) {
            score += 0.8;
        }
        if self.cmake_pattern.is_match(text) {
            score += 0.3;
        }
        if text.contains("Configuring done")
            || text.contains("Generating done")
            || text.contains("Build files have been written to:")
            || text.contains("CMake Error")
            || text.contains("Configuring incomplete")
            || text.contains("CMake Generate step failed")
            || text.contains("The following tests FAILED:")
            || text.contains("Errors while running CTest")
            || text.contains("Test #")
        {
            score += 0.4;
        }
        if NINJA_PROGRESS_RE.is_match(text) {
            score += 0.5;
        }
        if text.contains("] Building CXX object")
            || text.contains("] Building C object")
            || text.contains("] Linking CXX")
        {
            score += 0.5;
        }
        if self.error_pattern.is_match(text) {
            score += 0.8;
        }
        if AC_MARKERS.find(text).is_some() {
            score += 0.5;
        }

        if score > 0.3 {
            Some(score.min(1.0))
        } else {
            None
        }
    }

    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        // 第一遍：收集统计信息
        let mut stats = BuildStats::new();
        let lines: Vec<&str> = text.lines().collect();
        for (line_num, line) in lines.iter().enumerate() {
            stats.classify(line, line_num);
        }

        // 第二遍：生成压缩输出（应用折叠规则）
        let mut tokens: Vec<Token<'a>> = Vec::new();
        let mut folded_warnings: HashMap<String, bool> = HashMap::new(); // 记录已折叠的警告类型

        for (line_num, line) in lines.iter().enumerate() {
            // 检查是否是需要折叠的警告
            let mut should_skip = false;
            if line.contains("warning:") {
                // 提取 warning 签名
                if let Some(warning_type) = gcc_warning_signature(line) {
                    // 检查是否应该折叠
                    if stats.should_fold_warning(line_num, &warning_type) {
                        should_skip = true;

                        // 如果这是该类型第一次被折叠，插入折叠摘要
                        if !folded_warnings.contains_key(&warning_type) {
                            folded_warnings.insert(warning_type.clone(), true);
                            if let Some(summary) = stats.generate_fold_summary(&warning_type) {
                                let summary_with_newline =
                                    bumpalo::format!(in arena, "{}\n", summary);
                                tokens.push(Token::Text(Cow::Borrowed(
                                    summary_with_newline.into_bump_str(),
                                )));
                            }
                        }
                    }
                }
            }

            if !should_skip {
                if is_gcc_diagnostic_context_line(line) {
                    continue;
                }
                let compressed = self.compress_line_standardized(line, dict_engine, arena);
                let with_newline = bumpalo::format!(in arena, "{}\n", compressed);
                tokens.push(Token::Text(Cow::Borrowed(with_newline.into_bump_str())));
            }
        }

        // 添加构建摘要（如果有问题）
        if let Some(summary) = stats.generate_summary() {
            let summary_with_newline = bumpalo::format!(in arena, "{}\n", summary);
            tokens.push(Token::Text(Cow::Borrowed(
                summary_with_newline.into_bump_str(),
            )));
        }

        // 法则 A ROI 门控：`$GCC`/`$MAKE` 等 IR 标签在短样本上会整体扩张；
        // compact 比 raw 大则回退原文。参考 `docs/prompts/non_vcs_classical_prompts.md` § A.2.2。
        let compacted: String = tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.as_ref(),
                _ => "",
            })
            .collect();
        let final_text = finalize_gcc_compaction(text, compacted, dict_engine);
        let final_in_arena = arena.alloc_str(&final_text);

        CompressResult {
            tokens: vec![Token::Text(Cow::Borrowed(final_in_arena))],
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

        // 第一遍：收集统计信息
        let mut stats = BuildStats::new();
        let lines: Vec<&str> = text.lines().collect();
        for (line_num, line) in lines.iter().enumerate() {
            stats.classify(line, line_num);
        }

        // 第二遍：生成压缩输出（应用折叠规则）
        let mut tokens: Vec<Token<'a>> = Vec::new();
        let mut folded_warnings: HashMap<String, bool> = HashMap::new();

        for (line_num, line) in lines.iter().enumerate() {
            let normalized_line = context.convert_line(Cow::Borrowed(line));

            // 检查是否是需要折叠的警告
            let mut should_skip = false;
            if normalized_line.contains("warning:") {
                if let Some(warning_type) = gcc_warning_signature(normalized_line.as_ref()) {
                    if stats.should_fold_warning(line_num, &warning_type) {
                        should_skip = true;

                        if !folded_warnings.contains_key(&warning_type) {
                            folded_warnings.insert(warning_type.clone(), true);
                            if let Some(summary) = stats.generate_fold_summary(&warning_type) {
                                let summary_with_newline =
                                    bumpalo::format!(in arena, "{}\n", summary);
                                tokens.push(Token::Text(Cow::Borrowed(
                                    summary_with_newline.into_bump_str(),
                                )));
                            }
                        }
                    }
                }
            }

            if !should_skip {
                if is_gcc_diagnostic_context_line(normalized_line.as_ref()) {
                    continue;
                }
                let compressed =
                    self.compress_line_standardized(normalized_line.as_ref(), dict_engine, arena);
                let with_newline = bumpalo::format!(in arena, "{}\n", compressed);
                tokens.push(Token::Text(Cow::Borrowed(with_newline.into_bump_str())));
            }
        }

        // 添加构建摘要
        if let Some(summary) = stats.generate_summary() {
            let summary_with_newline = bumpalo::format!(in arena, "{}\n", summary);
            tokens.push(Token::Text(Cow::Borrowed(
                summary_with_newline.into_bump_str(),
            )));
        }

        // 法则 A ROI 门控：同上。
        let compacted: String = tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.as_ref(),
                _ => "",
            })
            .collect();
        let final_text = finalize_gcc_compaction(text, compacted, dict_engine);
        let final_in_arena = arena.alloc_str(&final_text);

        CompressResult {
            tokens: vec![Token::Text(Cow::Borrowed(final_in_arena))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn load_config(&mut self, config: &dyn Any) -> Result<(), String> {
        if let Some(_) =
            config.downcast_ref::<crate::core::plugin_config_loader::CompiledPluginConfig>()
        {
            return Ok(());
        }
        Err("Invalid config type".to_string())
    }
}
