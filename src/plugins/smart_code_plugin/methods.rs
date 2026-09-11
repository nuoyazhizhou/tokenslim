//! smart code plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::{Slice, SliceType};
use bumpalo::Bump;
use regex::Regex;
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
    /// SmartCodePlugin 默认实现：等价于 new()。
    fn default() -> Self {
        Self::new()
    }
}

impl SmartCodePlugin {
    /// 创建 SmartCodePlugin 实例（名称 smart_code，优先级 200），预编译标识符与空格正则。
    pub fn new() -> Self {
        Self {
            name: "smart_code",
            priority: 200,
            identifier_pattern: Arc::new(Regex::new(r"\b[a-zA-Z_]\w*\b").unwrap()),
            spaces_pattern: Arc::new(Regex::new(r" {2,}").unwrap()),
        }
    }
}

impl Plugin for SmartCodePlugin {
    /// 返回插件名称 "smart_code"。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级 200。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：代码块类 slice 类型或含 class/function/const/import 特征得 0.8。
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

    /// 压缩切片：连续空格压缩为 $S|N 标记，>8 字符标识符字典化（保留关键字与异常类），ROI 门控。
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
            // P2-78：字面 `$` 统一转义为 `$$`，防止源码中形如 `$S|5`/`$PK1` 的字面量
            // 被解压端误当作本插件标记还原（round-trip 失真）。转义先行于标记生成，
            // 本插件随后生成的 `$S|N`/`$PKn` 均为单 `$`，解压端可无歧义区分。
            let mut processed = line.replace('$', "$$");

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
        // P2-78：门控基线必须用「转义后原文」——兜底回落形态也要能被本插件 decompress
        // 无歧义解码，否则含字面 `$S|N`/`$PKn` 的原文在兜底路径仍会被误还原。
        let escaped_original = text.replace('$', "$$");
        let final_text =
            crate::core::utils::roi::prefer_non_expanding(&escaped_original, compacted);

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(final_text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 解压：将 $S|N 空格标记与 $PK 标识符 token 还原为原文。
    ///
    /// P2-78：以逐字节扫描器替代正则——`$$` 先于标记判定，作为字面 `$` 的转义序列
    /// 还原（与 compress 端 `replace('$', "$$")` 对偶），杜绝原文字面 `$S|N`/`$PKn`
    /// 被误还原；单 `$` 引导的 `$S|N`/`$PKn` 才按标记处理。
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        let mut result = String::new();

        for line in compressed.lines() {
            let restored = restore_line(line, dict);
            result.push_str(&restored);
            result.push('\n');
        }
        result
    }
}

/// 单行还原扫描器：依次判定转义序列 `$$`、空格标记 `$S|N`、标识符 token `$PKn`，
/// 其余内容（含单 `$` 非标记形态）原样透传。字节索引处均为 ASCII，切片边界安全。
///
/// P2-78：以逐字节扫描器替代正则——`$$` 先于标记判定，作为字面 `$` 的转义序列
/// 还原（与 compress 端 `replace('$', "$$")` 对偶），杜绝原文字面 `$S|N`/`$PKn`
/// 被误还原；单 `$` 引导的 `$S|N`/`$PKn` 才按标记处理。
fn restore_line(line: &str, dict: &Dictionary) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0usize;

    while i < line.len() {
        if bytes[i] == b'$' {
            // 1) 转义序列：$$ → 字面 $
            if bytes.get(i + 1) == Some(&b'$') {
                out.push('$');
                i += 2;
                continue;
            }
            // 2) 空格标记：$S|N → N 个空格
            if bytes.get(i + 1) == Some(&b'S') && bytes.get(i + 2) == Some(&b'|') {
                let mut j = i + 3;
                while j < line.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j > i + 3 {
                    let n: usize = line[i + 3..j].parse().unwrap_or(0);
                    out.push_str(&" ".repeat(n));
                    i = j;
                    continue;
                }
            }
            // 3) 标识符 token：$PKn → 词典还原（查不到原样保留，与 resolve_or_self 一致）
            if bytes.get(i + 1) == Some(&b'P') && bytes.get(i + 2) == Some(&b'K') {
                let mut j = i + 3;
                while j < line.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j > i + 3 {
                    let token = &line[i..j];
                    out.push_str(&dict.resolve_or_self(token));
                    i = j;
                    continue;
                }
            }
        }
        // 其余按 UTF-8 字符推进（多字节字符整体拷贝）
        let ch = line[i..].chars().next().unwrap_or('\0');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::text_slicer::{Slice, SliceType};
    use bumpalo::Bump;
    use std::borrow::Cow;

    /// 构造测试切片。
    fn slice_of<'a>(text: &'a str) -> Slice<'a> {
        Slice {
            id: 1,
            text: Cow::Borrowed(text),
            slice_type: SliceType::Unknown,
            offset: 0,
            line_start: 1,
            line_end: text.lines().count().max(1),
            file_metadata: None,
            flags: Default::default(),
        }
    }

    /// 压缩并返回 (compact 文本, 词典快照)。
    fn compress_of(text: &str) -> (String, crate::core::dictionary_engine::Dictionary) {
        let plugin = SmartCodePlugin::new();
        let mut dict_engine = DictionaryEngine::new();
        let mut dedup = DedupEngine::new(DedupConfig::default());
        let arena = Bump::new();
        let slice = slice_of(text);
        let result = plugin.compress(&slice, &mut dict_engine, &mut dedup, &arena);
        let compacted = result
            .tokens
            .iter()
            .filter_map(|t| match t {
                Token::Text(s) => Some(s.as_ref().to_string()),
                _ => None,
            })
            .collect::<String>();
        (compacted, dict_engine.snapshot())
    }

    /// P2-78 负路径回归：原文含标记形态字面量 `$S|5`/`$PK1` 时，compress→decompress
    /// 往返必须逐字还原，不得把字面量误还原为 5 个空格/词典值。
    #[test]
    fn round_trip_preserves_literal_marker_shaped_text() {
        let plugin = SmartCodePlugin::new();
        let source = "const tpl = \"$S|5 and $PK1\";\nlet price$$ = cost$S|2 total;\n";
        let (compacted, dict) = compress_of(source);
        let restored = plugin.decompress(&compacted, &dict);

        // 字面 $S|5 / $PK1 / $$ 均原样保留
        assert!(
            restored.contains("\"$S|5 and $PK1\""),
            "literal marker-shaped text preserved: {restored}"
        );
        assert!(
            restored.contains("price$$ = cost$S|2 total;"),
            "literal $$ and $S|2 preserved: {restored}"
        );
        // 源码行结构还原一致（忽略压缩产生的行尾规范化差异）
        let restored_body = restored.trim_end();
        let source_body = source.trim_end();
        assert_eq!(
            restored_body.split('\n').count(),
            source_body.split('\n').count(),
            "line count preserved: {restored}"
        );
    }

    /// P2-78 回归：普通代码往返仍保持语义（标记正常还原，转义不破坏既有行为）。
    /// 取特殊字符样本（非 ASCII 标识符）验证文件驱动链路不被转义机制破坏。
    #[test]
    fn round_trip_on_special_chars_sample_keeps_identifiers() {
        let plugin = SmartCodePlugin::new();
        let raw = crate::plugins::test_utils::read_sample_log(
            "smart_code_plugin",
            "case_007_special_chars",
        );
        let (compacted, dict) = compress_of(&raw);
        let restored = plugin.decompress(&compacted, &dict);
        for needle in ["function", "const", "console.log", "函数", "变量", "漢字"] {
            assert!(
                restored.contains(needle),
                "round-trip must keep `{needle}`: {restored}"
            );
        }
    }
}
