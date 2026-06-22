//! smart code plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::{Slice, SliceType};
use bumpalo::Bump;
use regex::Regex;
use std::any::Any;
use std::borrow::Cow;
use std::sync::Arc;

static KEYWORDS: &[&str] = &[
    "public",
    "private",
    "protected",
    "class",
    "interface",
    "implements",
    "extends",
    "void",
    "return",
    "if",
    "else",
    "for",
    "while",
    "static",
    "final",
];

/// 法则 D 防失忆红线：异常类 / 错误抛出词 / 堆栈锚点词必须以字面量保留。
///
/// smart_code 是通用源码压缩插件，默认会把任何 >8 字符的标识符字典化为 `$PKn`。
/// 但日志 / 源码里出现的异常类型名（`SyntaxError` / `ZeroDivisionError` 等）
/// 与错误抛出关键字（`throw` / `raise` / `panic!`）是 LLM 识别运行时错误的关键信号，
/// 一旦被字典化后 LLM 无法识别异常本体，违反 Compression Protocol V1 法则 D。
///
/// 白名单覆盖：
/// - Java / JavaScript / Python / Ruby / PHP 常见异常类名
/// - 异常 / 错误抛出 / 堆栈追踪关键词
/// - 后缀兜底：`*Error` / `*Exception` / `*Warning` / `*Fault`
///
/// 注意：长度 <=8 的单词本身就不进入字典化流程，无需在白名单中列出（如 `throw`/`raise`/`panic`）。
/// 白名单只列长度 >8 且语义上属于「不可丢」的关键词。
fn should_preserve_identifier(id: &str) -> bool {
    const KEEP: &[&str] = &[
        // JavaScript / Node.js 内置异常
        "SyntaxError",
        "TypeError",
        "ReferenceError",
        "RangeError",
        "URIError",
        "EvalError",
        "AggregateError",
        "InternalError",
        "UnhandledPromiseRejectionWarning",
        "DeprecationWarning",
        // Python 高频内置异常（长度 >8 才需要列入）
        "AssertionError",
        "AttributeError",
        "ArithmeticError",
        "ZeroDivisionError",
        "FloatingPointError",
        "OverflowError",
        "LookupError",
        "IndexError",
        "ImportError",
        "ModuleNotFoundError",
        "NameError",
        "UnboundLocalError",
        "IndentationError",
        "BufferError",
        "MemoryError",
        "NotImplementedError",
        "RecursionError",
        "RuntimeError",
        "StopIteration",
        "StopAsyncIteration",
        "SystemError",
        "SystemExit",
        "ValueError",
        "UnicodeError",
        "UnicodeDecodeError",
        "UnicodeEncodeError",
        "UnicodeTranslateError",
        "OSError",
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
        "PendingDeprecationWarning",
        "ResourceWarning",
        "UserWarning",
        "SyntaxWarning",
        "RuntimeWarning",
        "FutureWarning",
        "ImportWarning",
        "UnicodeWarning",
        "BytesWarning",
        // Java 高频运行时异常
        "NullPointerException",
        "IllegalArgumentException",
        "IllegalStateException",
        "IndexOutOfBoundsException",
        "ArrayIndexOutOfBoundsException",
        "StringIndexOutOfBoundsException",
        "ClassCastException",
        "ClassNotFoundException",
        "NumberFormatException",
        "UnsupportedOperationException",
        "ArithmeticException",
        "ConcurrentModificationException",
        "NoSuchElementException",
        "NoSuchMethodException",
        "NoSuchFieldException",
        "NoSuchMethodError",
        "StackOverflowError",
        "OutOfMemoryError",
        "NoClassDefFoundError",
        "IncompatibleClassChangeError",
        "VerifyError",
        "ExceptionInInitializerError",
        // 堆栈 / 错误锚点词
        "Traceback",
        "Exception",
        "Throwable",
        "Uncaught",
        "UncaughtException",
        "FatalError",
        "PanicError",
    ];
    if KEEP.contains(&id) {
        return true;
    }
    // 后缀兜底：符合「以 Error / Exception / Warning / Fault 结尾」命名的自定义类名
    id.ends_with("Error")
        || id.ends_with("Exception")
        || id.ends_with("Warning")
        || id.ends_with("Fault")
}

impl Default for SmartCodePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SmartCodePlugin {
    pub fn new() -> Self {
        Self {
            name: "smart_code",
            priority: 200,
            config: SmartCodeConfig::default(),
            identifier_pattern: Arc::new(Regex::new(r"\b[a-zA-Z_]\w*\b").unwrap()),
            spaces_pattern: Arc::new(Regex::new(r" {2,}").unwrap()),
        }
    }
}

impl Plugin for SmartCodePlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        match slice.slice_type {
            SliceType::CodeBlock
            | SliceType::VueComponent
            | SliceType::ReactComponent
            | SliceType::AngularComponent
            | SliceType::SvelteComponent => return Some(0.8),
            _ => {}
        }
        let text = slice.text.as_ref();
        if text.contains("public class")
            || text.contains("function ")
            || text.contains("const ")
            || text.contains("import ")
        {
            return Some(0.8);
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
        let mut tokens = Vec::new();

        for line in text.lines() {
            let mut processed = line.to_string();

            // 1. 压缩空格
            processed = self
                .spaces_pattern
                .replace_all(&processed, |caps: &regex::Captures| {
                    format!("$S|{}", caps.get(0).unwrap().as_str().len())
                })
                .into_owned();

            // 2. 压缩标识符
            processed = self
                .identifier_pattern
                .replace_all(&processed, |caps: &regex::Captures| {
                    let id = caps.get(0).unwrap().as_str();
                    if id.len() > 8 && !KEYWORDS.contains(&id) && !should_preserve_identifier(id) {
                        dict_engine.add_package(id)
                    } else {
                        id.to_string()
                    }
                })
                .into_owned();

            tokens.push(Token::Text(Cow::Owned(format!("{}\n", processed))));
        }

        // 法则 A ROI 门控：对整段做 `prefer_non_expanding` 兜底，避免短样本 / 标识符少的
        // 样本因 `$S|N`、`$PKn` 元字符反而扩张。参考 non_vcs_classical_prompts.md § 1.3。
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
        let space_re = Regex::new(r"\$S\|(\d+)").unwrap();
        let token_re = Regex::new(r"(\$PK\d+)").unwrap();

        for line in compressed.lines() {
            let mut restored = line.to_string();

            // 还原标识符
            restored = token_re
                .replace_all(&restored, |caps: &regex::Captures| {
                    let token = caps.get(1).unwrap().as_str();
                    dict.resolve_or_self(token)
                })
                .into_owned();

            // 还原空格
            restored = space_re
                .replace_all(&restored, |caps: &regex::Captures| {
                    let len: usize = caps.get(1).unwrap().as_str().parse().unwrap_or(0);
                    " ".repeat(len)
                })
                .into_owned();

            result.push_str(&restored);
            result.push('\n');
        }
        result
    }

    fn load_config(&mut self, config: &dyn Any) -> Result<(), String> {
        if let Some(c) = config.downcast_ref::<SmartCodeConfig>() {
            self.config = c.clone();
            return Ok(());
        }
        Err("Invalid config type".to_string())
    }
}
