//! python traceback plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_config_loader::CompiledPluginConfig;
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::any::Any;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

/// 深层堆栈截断阈值：超过此数量的堆栈帧将被折叠
const STACK_FRAME_THRESHOLD: usize = 15;

/// 相似异常去重阈值：超过此数量的相似异常将被折叠
const DUPLICATE_EXCEPTION_THRESHOLD: usize = 2;

impl PythonTracebackPlugin {
    pub fn new() -> Self {
        Self {
            name: "python_traceback",
            priority: 80,
            trace_header_pattern: Arc::new(
                Regex::new(r"Traceback \(most recent call last\):").unwrap(),
            ),
            file_line_pattern: Arc::new(
                Regex::new(r#"  File "([^"]+)", line (\d+), in (.+)"#).unwrap(),
            ),
            exception_pattern: Arc::new(Regex::new(r"^([a-zA-Z0-9_.]+): (.*)$").unwrap()),
            config: None,
        }
    }

    /// 判断 Python 异常类名是否必须以字面量保留。
    ///
    /// 法则 D 防失忆：`AssertionError` / `KeyError` / `ValueError` 等是 LLM 定位
    /// Python 错误类型的关键信号；若被 `dict_engine.add_package` 字典化为 `$PKn`，
    /// LLM 将无从得知异常本体。
    ///
    /// 白名单取自 Python 3 内置异常层级（`Exception` 子类）+ 常见第三方派生：
    /// https://docs.python.org/3/library/exceptions.html#exception-hierarchy
    fn should_preserve_class_name(class_name: &str) -> bool {
        const BUILTIN: &[&str] = &[
            // 基础与系统级
            "BaseException",
            "Exception",
            "SystemExit",
            "KeyboardInterrupt",
            "GeneratorExit",
            // ArithmeticError 族
            "ArithmeticError",
            "FloatingPointError",
            "OverflowError",
            "ZeroDivisionError",
            // LookupError 族
            "LookupError",
            "IndexError",
            "KeyError",
            // OSError 族（及 Python 3.3+ 别名）
            "OSError",
            "IOError",
            "EnvironmentError",
            "BlockingIOError",
            "ChildProcessError",
            "ConnectionError",
            "BrokenPipeError",
            "ConnectionAbortedError",
            "ConnectionRefusedError",
            "ConnectionResetError",
            "FileExistsError",
            "FileNotFoundError",
            "InterruptedError",
            "IsADirectoryError",
            "NotADirectoryError",
            "PermissionError",
            "ProcessLookupError",
            "TimeoutError",
            // ImportError 族
            "ImportError",
            "ModuleNotFoundError",
            // NameError 族
            "NameError",
            "UnboundLocalError",
            // SyntaxError 族
            "SyntaxError",
            "IndentationError",
            "TabError",
            // 其他常见异常
            "AssertionError",
            "AttributeError",
            "BufferError",
            "EOFError",
            "MemoryError",
            "NotImplementedError",
            "RecursionError",
            "ReferenceError",
            "RuntimeError",
            "StopIteration",
            "StopAsyncIteration",
            "SystemError",
            "TypeError",
            "ValueError",
            "UnicodeError",
            "UnicodeDecodeError",
            "UnicodeEncodeError",
            "UnicodeTranslateError",
            // Warning 族
            "Warning",
            "DeprecationWarning",
            "PendingDeprecationWarning",
            "UserWarning",
            "SyntaxWarning",
            "RuntimeWarning",
            "FutureWarning",
            "ImportWarning",
            "UnicodeWarning",
            "BytesWarning",
            "ResourceWarning",
        ];
        if BUILTIN.contains(&class_name) {
            return true;
        }

        // 命名模式兜底：第三方库 / 业务自定义的异常子类
        // 如 `DjangoValidationError` / `HTTPException` / `PyTestRunError`
        class_name.ends_with("Error")
            || class_name.ends_with("Exception")
            || class_name.ends_with("Warning")
    }

    /// 提取异常类型（简单类名）从异常行
    /// 例如：`KeyError: 'missing_key'` -> `KeyError`
    #[tracing::instrument(level = "debug", skip_all)]
    fn extract_exception_type(line: &str) -> Option<String> {
        if let Some(colon_pos) = line.find(':') {
            let exc_type = line[..colon_pos].trim();
            // 只接受真实异常类名，避免把 chained traceback 的说明行当成异常类型。
            if Self::should_preserve_class_name(exc_type) {
                return Some(exc_type.to_string());
            }
        }
        None
    }

    /// 去重相似异常：检测并折叠重复的异常类型
    #[tracing::instrument(level = "debug", skip_all)]
    fn dedupe_similar_exceptions(text: &str) -> String {
        let mut result = String::new();
        let mut exception_counts: HashMap<String, usize> = HashMap::new();
        let mut current_traceback = String::new();
        let mut in_traceback = false;
        let mut current_exception_type = String::new();
        let mut current_exception_line = String::new();

        for line in text.lines() {
            if line.contains("Traceback (most recent call last):") {
                // 新 traceback 开始
                if !current_traceback.is_empty() && !current_exception_type.is_empty() {
                    let count = exception_counts
                        .entry(current_exception_type.clone())
                        .or_insert(0);
                    *count += 1;

                    if *count == 1 {
                        result.push_str(&current_traceback);
                    } else if *count >= DUPLICATE_EXCEPTION_THRESHOLD {
                        let signature = if current_exception_line.is_empty() {
                            current_exception_type.as_str()
                        } else {
                            current_exception_line.as_str()
                        };
                        result.push_str(&format!(
                            "[DUPLICATE] Similar {} #{}: {}\n",
                            current_exception_type, count, signature
                        ));
                    }
                }
                current_traceback.clear();
                current_traceback.push_str(line);
                current_traceback.push('\n');
                in_traceback = true;
                current_exception_type.clear();
                current_exception_line.clear();
            } else if in_traceback {
                current_traceback.push_str(line);
                current_traceback.push('\n');

                // 提取异常类型
                if let Some(exc_type) = Self::extract_exception_type(line) {
                    current_exception_type = exc_type;
                    current_exception_line = line.to_string();
                    in_traceback = false;
                }
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        // 处理最后一个 traceback
        if !current_traceback.is_empty() && !current_exception_type.is_empty() {
            let count = exception_counts
                .entry(current_exception_type.clone())
                .or_insert(0);
            *count += 1;
            if *count == 1 {
                result.push_str(&current_traceback);
            } else if *count >= DUPLICATE_EXCEPTION_THRESHOLD {
                let signature = if current_exception_line.is_empty() {
                    current_exception_type.as_str()
                } else {
                    current_exception_line.as_str()
                };
                result.push_str(&format!(
                    "[DUPLICATE] Similar {} #{}: {}\n",
                    current_exception_type, count, signature
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
        let mut in_traceback = false;

        for line in text.lines() {
            if line.contains("Traceback (most recent call last):") {
                result.push_str(line);
                result.push('\n');
                in_traceback = true;
                frame_count = 0;
            } else if in_traceback && line.contains("  File ") {
                frame_count += 1;
                if frame_count <= STACK_FRAME_THRESHOLD {
                    result.push_str(line);
                    result.push('\n');
                } else if frame_count == STACK_FRAME_THRESHOLD + 1 {
                    // 第一次超过阈值时，添加摘要
                    let total_frames = text.lines().filter(|l| l.contains("  File ")).count();
                    result.push_str(&format!(
                        "[STACK] {} frames (first {} shown, {} omitted)\n",
                        total_frames,
                        STACK_FRAME_THRESHOLD,
                        total_frames - STACK_FRAME_THRESHOLD
                    ));
                }
            } else if in_traceback && (line.contains(":") && !line.starts_with("  ")) {
                // 异常行
                in_traceback = false;
                result.push_str(line);
                result.push('\n');
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        result
    }

    /// 压缩 Chained 异常：折叠多个链式异常
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_chained_exceptions(text: &str) -> String {
        let mut result = String::new();
        let mut chained_count = 0;

        for line in text.lines() {
            if line.contains("During handling of the above exception") {
                chained_count += 1;
                if chained_count == 1 {
                    result.push_str(line);
                    result.push('\n');
                } else if chained_count == 2 {
                    result.push_str(&format!(
                        "[CHAINED] {} exceptions (see details below)\n",
                        chained_count
                    ));
                }
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        result
    }
}

impl Plugin for PythonTracebackPlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        if self.trace_header_pattern.is_match(text) || text.contains(".py\", line ") {
            return Some(0.9);
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

        // 只在有多个 traceback 或深层堆栈时应用新功能
        let traceback_count = text.matches("Traceback (most recent call last):").count();
        let frame_count = text.matches("  File ").count();

        let processed_text = if traceback_count >= 2 {
            // 多个 traceback：应用去重和摘要
            // 先在原文中统计异常类型
            let mut exception_counts: HashMap<String, usize> = HashMap::new();
            for line in text.lines() {
                if let Some(exc_type) = Self::extract_exception_type(line) {
                    *exception_counts.entry(exc_type).or_insert(0) += 1;
                }
            }

            // 然后去重
            let text_after_dedup = Self::dedupe_similar_exceptions(text);

            // 最后添加摘要（基于原文统计）
            let mut result = text_after_dedup;
            if !exception_counts.is_empty() {
                let total = exception_counts.values().sum::<usize>();
                let mut summary = format!("[SUMMARY] {} exceptions: ", total);
                let mut parts: Vec<_> = exception_counts.iter().collect();
                parts.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

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
        } else if frame_count > 15 {
            // 深层堆栈：应用截断
            Self::truncate_deep_stack(text)
        } else if text.contains("During handling of the above exception") {
            // 链式异常：应用链式压缩
            Self::compress_chained_exceptions(text)
        } else {
            // 单个简单 traceback：不应用新功能
            text.to_string()
        };

        let mut tokens: Vec<Token<'a>> = Vec::new();
        let mut in_traceback = false;

        for line in processed_text.lines() {
            if self.trace_header_pattern.is_match(line) {
                tokens.push(Token::Text(Cow::Borrowed("$PY|TB\n")));
                in_traceback = true;
                continue;
            }

            if in_traceback {
                if let Some(caps) = self.file_line_pattern.captures(line) {
                    let file_path = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                    let line_num = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                    let func_name = caps.get(3).map(|m| m.as_str()).unwrap_or("");

                    let encoded_path = crate::core::path_compressor::methods::replace_paths_in_text(
                        file_path,
                        dict_engine,
                    );
                    tokens.push(Token::Text(Cow::Owned(format!(
                        "$PY|FL|{}|{}|{}\n",
                        line_num, func_name, encoded_path
                    ))));
                } else if let Some(caps) = self.exception_pattern.captures(line) {
                    let class_name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                    let msg = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                    // 法则 D 防失忆：Python 内置异常类名 + 模式兜底必须保留字面量，
                    // 非白名单的自定义类名才允许字典化。
                    let class_token = if Self::should_preserve_class_name(class_name) {
                        class_name.to_string()
                    } else {
                        dict_engine.add_package(class_name)
                    };
                    tokens.push(Token::Text(Cow::Owned(format!(
                        "$PY|EX|{}|{}\n",
                        class_token, msg
                    ))));
                    in_traceback = false;
                } else {
                    tokens.push(Token::Text(Cow::Owned(format!("{}\n", line))));
                }
            } else {
                tokens.push(Token::Text(Cow::Owned(format!("{}\n", line))));
            }
        }

        // 法则 A ROI 门控：若压缩后体积反而变大（短 traceback / 单异常场景），
        // 回退原文整段直接透传。参考 `docs/prompts/non_vcs_classical_prompts.md` § 1.3。
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

    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        let mut result = String::new();
        for line in compressed.lines() {
            if line.starts_with("$PY|TB") {
                result.push_str("Traceback (most recent call last):\n");
            } else if line.starts_with("$PY|FL|") {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 5 {
                    let line_num = parts[2];
                    let func_name = parts[3];
                    let file_path = dict.resolve_or_self(parts[4].trim_end());
                    result.push_str(&format!(
                        "  File \"{}\", line {}, in {}\n",
                        file_path, line_num, func_name
                    ));
                }
            } else if line.starts_with("$PY|EX|") {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 4 {
                    let class_name = dict.resolve_or_self(parts[2]);
                    let msg = parts[3];
                    result.push_str(&format!("{}: {}\n", class_name, msg));
                }
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }
        result
    }

    fn load_config(&mut self, config: &dyn Any) -> Result<(), String> {
        if let Some(c) = config.downcast_ref::<CompiledPluginConfig>() {
            self.config = Some(c.clone());
            return Ok(());
        }
        Err("Invalid config".to_string())
    }
}

impl Clone for PythonTracebackPlugin {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            priority: self.priority,
            trace_header_pattern: self.trace_header_pattern.clone(),
            file_line_pattern: self.file_line_pattern.clone(),
            exception_pattern: self.exception_pattern.clone(),
            config: self.config.clone(),
        }
    }
}
