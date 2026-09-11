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
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

static ERROR_MARKERS: &[&str] = &["error:", "warning:", "note:", "fatal error:"];
static AC_MARKERS: Lazy<AhoCorasick> = Lazy::new(|| AhoCorasick::new(ERROR_MARKERS).unwrap());
static NINJA_PROGRESS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\[(?P<step>\d+/\d+)\]\s+(?P<msg>.+)$").unwrap());
static PATH_TOKEN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\$P\d+").unwrap());
/// nm 符号表行：地址列（十六进制 8-16 位，或未定义符号的空白占位）+
/// 单一类型字母 + 符号名。强特征门槛用 ≥3 行连续命中来避免误判普通文本。
static NM_SYMBOL_LINE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        // `[\w.$@]*` 的 `@` 允许 nm -D 动态符号名带版本尾（`name@@GLIBC_2.2.5` / `name@GLIBC_2.2`）。
        r"^(?:[0-9a-f]{8,16}| {16})\s+[AaBbCcDdGgIiNnRrSsTtUuVvWw?]\s+[A-Za-z_$][\w.$@]*",
    )
    .unwrap()
});
/// size 输出表头：text data bss dec hex filename。
static SIZE_HEADER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*text\s+data\s+bss\s+dec\s+hex\s+(?:filename|file)\s*$").unwrap());
/// size 输出数据行：5 个十进制数值列 + 文件名（列宽可含前导空格对齐）。
static SIZE_ROW_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+([0-9a-fA-F]+)\s+(.+)$").unwrap()
});
/// objdump/readelf 节表特征行。
static BINUTILS_SECTION_HEADER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:Sections:|Idx\s+Name\s+Size|Symbol table|[Tt]he \S+ sections are)").unwrap()
});
/// readelf -S 独有的节表头（`Section Headers:` 及 `[Nr] Name Type` 列标题），
/// 与 objdump `Idx Name Size` 区分开。
static READELF_SECTION_HEADER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(?:Section Headers:|\[Nr\]\s+Name\s+Type)").unwrap());
/// readelf -S 数据行：`[ 1] .interp PROGBITS <Address> <Off> <Size> <ES> ...`。
/// 用 `split_whitespace` 后 Size = parts[5]（Address=3, Off=4, Size=5），
/// 非其余列。空节 `NULL` 行会被捕获为 name="NULL"，由调用方跳过。
static READELF_SECTION_ROW_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\[\s*\d+\]\s+(\S+)\s+\S+\s+[0-9a-f]+\s+[0-9a-f]+\s+([0-9a-fA-F]+)").unwrap()
});
/// ar 归档成员名：字母/数字/下划线开头，可带 `/` 路径分隔，通常以 `.o`/`.obj`/`.a`/`.c` 等结尾。
static AR_MEMBER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[A-Za-z0-9_$][A-Za-z0-9_.$@/-]*\.(?:o|obj|a|c|cc|cpp|s|lib)$").unwrap()
});
/// objdump -d 反汇编节头：`Disassembly of section .text:`。
static OBJDUMP_DISASM_HEADER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^Disassembly of section ").unwrap());
/// objdump -d 函数边界标签：`0000000000000000 <main>:`（hex 地址 + `<符号名>:`）。
static OBJDUMP_FUNC_LABEL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[0-9a-f]{4,16}\s+<(\S+)>:$").unwrap());
/// objdump -d 指令行：前导空格 + 短 hex 偏移（相对函数，1-8 位）以 `:` 结尾。
static OBJDUMP_INST_OFFSET_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s+[0-9a-f]{1,8}:[ \t]").unwrap());
/// objdump -r 重定位表节头：现代 binutils 的 `RELOCATION RECORDS FOR [.text]:`，
/// 以及旧格式的 `Relocation section '.rela.text' at offset 0x40 contains N entries:`。
/// 重定位表是链接缺口判定（PC32/PLT32 等类型 + 符号名）的关键，区别于反汇编/符号/节表。
static OBJDUMP_RELOC_HEADER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:RELOCATION RECORDS FOR \[\S+\]:|Relocation section '\S+' at offset)").unwrap()
});
/// ar rcs 创建归档 verbose 的操作行前缀：`a - <member>`（GNU/BSD 添加成员记录），
/// 保留添加的成员路径，区别于 ar -t 的只读清单。
static AR_CREATE_MEMBER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^a - ").unwrap());
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

/// 收尾压缩：先附加内联路径字典，做 ROI 门控；若结果含未展开路径 token 则展开后再次门控。
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

/// 将压缩文本中的路径 token（$P\d+）按 token 长度降序替换回原始路径（边界感知）。
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

/// 判断行是否为 GCC 诊断上下文行（源码片段行，如 "12 | int x = 1;"），此类行直接丢弃。
#[tracing::instrument(level = "trace", skip_all)]
fn is_gcc_diagnostic_context_line(line: &str) -> bool {
    GCC_DIAGNOSTIC_CONTEXT_RE.is_match(line)
}

/// 判断行是否为 GCC/Clang 诊断头（P2-19）：
/// `file:line:col: error:/warning:/note:/fatal error:` 或 cargo 风格 `-->` 提示行。
#[tracing::instrument(level = "trace", skip_all)]
fn is_gcc_diagnostic_header_line(line: &str) -> bool {
    static HEADER_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^\s*(?:[^\s:]+:\d+:\d+:|\d+:\d+:)?\s*(?:fatal error|error|warning|note):|^\s*-->")
            .unwrap()
    });
    HEADER_RE.is_match(line)
}

/// 提取警告签名：warning: 后的消息 + [-Wxxx] 警告类型，用于同类警告折叠分组。
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

/// 计算文本中「连续命中 nm 符号表行」的最大窗口长度。
///
/// 返回连续命中 ≥2 行的最大连续数（`None` 表示全程无连续命中）。
/// 作为 detect 的强特征门槛：整块 nm 符号表（数十行连续）返回大窗口，
/// 而普通日志里零星出现的「十六进制+类型字母」单行不会触发。
#[tracing::instrument(level = "trace", skip_all)]
fn count_consecutive_nm_symbol_lines(text: &str) -> Option<usize> {
    let mut max_run = 0usize;
    let mut cur = 0usize;
    for line in text.lines() {
        if NM_SYMBOL_LINE_RE.is_match(line) {
            cur += 1;
            if cur > max_run {
                max_run = cur;
            }
        } else {
            cur = 0;
        }
    }
    if max_run >= 2 {
        Some(max_run)
    } else {
        None
    }
}

/// 解析一行 objdump -t 符号行，返回 `Some((flag, type, name))`；type 列可能缺失（返回空串）。
///
/// objdump -t 行形如 `{addr} {l|g|w} [{type}] {section} {size} {name}`，各列空白分隔：
/// - 满 6 token（含 type）时 type = parts[2]
/// - 缺 type 的 5 token（如 `g .bss size __bss_start`）时 type = ""
/// section 可为 `.text`/`*ABS*`/`*UND*` 等无空格 token，故 name 恒为末 token。
fn parse_objdump_symbol_row(line: &str) -> Option<(&str, &str, &str)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let n = parts.len();
    if n < 5 {
        return None;
    }
    let addr = parts[0];
    if addr.len() < 4 || addr.len() > 16 || !addr.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    if !matches!(parts[1], "l" | "g" | "w") {
        return None;
    }
    let size = parts[n - 2];
    if size.len() < 2 || size.len() > 16 || !size.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let name = parts[n - 1];
    if !name
        .chars()
        .next()
        .map_or(false, |c| c.is_ascii_alphabetic() || c == '_' || c == '$')
    {
        return None;
    }
    let typ = if n >= 6 { parts[2] } else { "" };
    Some((parts[1], typ, name))
}

/// 计算文本中「连续命中 objdump -t 符号行」的最大窗口长度，作为 detect 强特征门槛。
#[tracing::instrument(level = "trace", skip_all)]
fn count_consecutive_objdump_symbol_rows(text: &str) -> Option<usize> {
    let mut max_run = 0usize;
    let mut cur = 0usize;
    for line in text.lines() {
        if parse_objdump_symbol_row(line).is_some() {
            cur += 1;
            if cur > max_run {
                max_run = cur;
            }
        } else {
            cur = 0;
        }
    }
    if max_run >= 2 {
        Some(max_run)
    } else {
        None
    }
}

/// 判断是否为 2-8 位偶长度纯 hex 机器码字节 token（如 `48`/`89e5`），
/// 用于在反汇编指令行里从偏移后的字节流中定位助记符起点。
#[tracing::instrument(level = "trace", skip_all)]
fn is_machine_byte_token(s: &str) -> bool {
    (2..=8).contains(&s.len()) && s.len() % 2 == 0 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// 解析一行 objdump -d 指令行，折叠行首偏移与机器码字节，返回 `助记符 [操作数]`。
/// 反汇编短 hex 偏移（`0:`/`a:`）不同于符号表长地址，此处仅处理以 `:` 结尾的行。
#[tracing::instrument(level = "trace", skip_all)]
fn parse_objdump_disasm_instruction(line: &str) -> Option<String> {
    if !OBJDUMP_INST_OFFSET_RE.is_match(line) {
        return None;
    }
    let toks: Vec<&str> = line.split_whitespace().collect();
    // toks[0] 形如 `0:` / `4004a0:`（偏移+冒号），随后为机器码字节、再助记符与操作数。
    let offset_with_colon = toks.first()?;
    if !offset_with_colon.ends_with(':') {
        return None;
    }
    let mut idx = 1usize;
    while idx < toks.len() && is_machine_byte_token(toks[idx]) {
        idx += 1;
    }
    if idx >= toks.len() {
        return None; // 纯机器码续行（长指令换行），无独立助记符，跳过
    }
    let mnemonic = toks[idx];
    let operands = toks[idx + 1..].join(" ");
    if operands.is_empty() {
        Some(mnemonic.to_string())
    } else {
        Some(format!("{mnemonic} {operands}"))
    }
}

/// 检测 objdump -d 反汇编块：存在节头，或 ≥2 行函数边界标签，或 ≥3 行指令行。
/// 强特征门槛：反汇编的 `<func>:` 标签与 hex+冒号偏移行组合不会出现在普通日志。
#[tracing::instrument(level = "trace", skip_all)]
fn has_objdump_disasm_block(text: &str) -> bool {
    let mut funcs = 0usize;
    let mut insts = 0usize;
    let mut has_header = false;
    for line in text.lines() {
        let t = line.trim();
        if OBJDUMP_DISASM_HEADER_RE.is_match(t) {
            has_header = true;
        }
        if OBJDUMP_FUNC_LABEL_RE.is_match(t) {
            funcs += 1;
        } else if parse_objdump_disasm_instruction(line).is_some() {
            insts += 1;
        }
    }
    has_header || funcs >= 2 || insts >= 3
}

/// 折叠 objdump -d 反汇编：保留函数边界标签 `$FUNC <name>` 与指令助记符 `$ASM <mnemonic> <ops>`，
/// 折叠每行的相对偏移与机器码字节列（对阅读非必要，跳转目标以 `<func+0x>` 呈现自含）。
#[tracing::instrument(level = "trace", skip_all)]
fn compact_objdump_disasm(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty()
            || OBJDUMP_DISASM_HEADER_RE.is_match(t)
            || t.contains("file format")
            || t.ends_with("$(END)")
        {
            continue;
        }
        // 函数边界标签：去地址，保留符号名。
        if let Some(caps) = OBJDUMP_FUNC_LABEL_RE.captures(t) {
            out.push_str("$FUNC ");
            out.push_str(&caps[1]);
            out.push('\n');
            continue;
        }
        if let Some(instr) = parse_objdump_disasm_instruction(line) {
            out.push_str("$ASM ");
            out.push_str(&instr);
            out.push('\n');
        }
    }
    out
}

/// 解析一行 objdump -r 重定位入口，返回 `Some((type, symbol))`。
///
/// 重定位行有两种列布局：
/// - 现代 binutils：`{off16} {R_TYPE} {symbol-0x...addend}`（3 token）
/// - 旧格式：`{off} {info} {R_TYPE} {sym.value} {symbol} [+|- N]`（≥5 token）
/// 统一按「首个以 `R_` 开头的 token 作为类型」定位，符号取其后的首个非纯 hex token，
/// 并剥离 addend 后缀（`-0x`/`+0x` 或尾随 `- N`）。符号名可含版本尾（`sym@@GLIBC_2.2`）。
#[tracing::instrument(level = "trace", skip_all)]
fn parse_objdump_reloc_row(line: &str) -> Option<(String, String)> {
    let toks: Vec<&str> = line.split_whitespace().collect();
    let ti = toks.iter().position(|t| t.starts_with("R_"))?;
    if ti == 0 {
        return None;
    }
    // offset 前导列须为十六进制（新旧格式均满足）。
    let addr = toks[ti - 1];
    if addr.len() < 8 || addr.len() > 16 || !addr.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let typ = toks[ti].to_string();
    let raw_symbol = toks[ti + 1..]
        .iter()
        .find(|t| !t.chars().all(|c| c.is_ascii_hexdigit()))?;
    let mut symbol = (*raw_symbol).to_string();
    // 剥离 addend：新格式符号内嵌 `-0x`/`+0x` 后缀。
    if let Some(idx) = symbol.find("-0x") {
        symbol.truncate(idx);
    } else if let Some(idx) = symbol.find("+0x") {
        symbol.truncate(idx);
    }
    if symbol.is_empty() {
        return None;
    }
    Some((typ, symbol))
}

/// 检测 objdump -r 重定位块：存在节头且至少 1 条重定位入口行。
/// 强特征门槛：`R_*` 类型 + 前置 hex 地址列组合不会出现在普通编译日志。
#[tracing::instrument(level = "trace", skip_all)]
fn has_objdump_reloc_block(text: &str) -> bool {
    let mut has_header = false;
    let mut rows = 0usize;
    for line in text.lines() {
        let t = line.trim();
        if OBJDUMP_RELOC_HEADER_RE.is_match(t) {
            has_header = true;
        } else if parse_objdump_reloc_row(line).is_some() {
            rows += 1;
        }
    }
    has_header && rows >= 1
}

/// 折叠 objdump -r 重定位表：保留 `$RELOC {type} {symbol}`，折叠 Offset/Info/Sym.Value/Addend。
/// 类型（PC32/PLT32 等）与符号名是链接解析与缺口判定的关键语义，地址与应用值冗余。
#[tracing::instrument(level = "trace", skip_all)]
fn compact_objdump_relocations(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty()
            || OBJDUMP_RELOC_HEADER_RE.is_match(t)
            || t.contains("file format")
            || t.starts_with("OFFSET")
            || t.contains("Sym. Name")
        {
            continue;
        }
        if let Some((typ, symbol)) = parse_objdump_reloc_row(line) {
            out.push_str("$RELOC ");
            out.push_str(&typ);
            out.push(' ');
            out.push_str(&symbol);
            out.push('\n');
        }
    }
    out
}

/// 检测 ar rcs 创建归档 verbose 块：存在 `a - <member>` 操作行 ≥1，
/// 且出现 `ar rcs`/`ar r` 创建命令（含 -v）或 `ar: creating <archive>` 提示。
/// 区别于 ar -t 只读清单（`a - ` 前缀 + 创建提示是写操作特征）。
#[tracing::instrument(level = "trace", skip_all)]
fn has_ar_create_block(text: &str) -> Option<usize> {
    let mut members = 0usize;
    let mut has_create_signal = false;
    for line in text.lines() {
        let t = line.trim();
        if AR_CREATE_MEMBER_RE.is_match(t) {
            members += 1;
        } else if t.starts_with("ar ") && t.contains("rcs") {
            has_create_signal = true;
        } else if t.starts_with("ar: creating ") {
            has_create_signal = true;
        }
    }
    if members >= 1 && has_create_signal {
        Some(members)
    } else {
        None
    }
}

/// 折叠 ar rcs 创建归档 verbose：保留添加成员 `$AR_CREATE {member}` 与归档创建提示
/// `$AR_CREATE archive {name}`，折叠创建命令行。保留语义：列出了创建了哪些成员。
#[tracing::instrument(level = "trace", skip_all)]
fn compact_ar_create(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("a - ") {
            out.push_str("$AR_CREATE ");
            out.push_str(rest.trim());
            out.push('\n');
        } else if let Some(rest) = t.strip_prefix("ar: creating ") {
            out.push_str("$AR_CREATE archive ");
            out.push_str(rest.trim());
            out.push('\n');
        }
    }
    out
}

/// 检测 ar -t 归档成员清单块：成员行 ≥3 且非成员行（命令/表头/空行）≤2 行。
/// 强门槛保证「纯成员名列表」才触发，避免把普通文件名日志误判为 ar 输出。
#[tracing::instrument(level = "trace", skip_all)]
fn find_ar_member_block(text: &str) -> Option<usize> {
    let non_empty: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if non_empty.len() < 3 {
        return None;
    }
    let hits = non_empty
        .iter()
        .filter(|l| AR_MEMBER_RE.is_match(l))
        .count();
    if hits >= 3 && non_empty.len() - hits <= 2 {
        Some(hits)
    } else {
        None
    }
}

/// 识别并整块压缩 binutils 分析输出（nm 符号表 / size 节大小 / objdump·readelf 节表）。
///
/// 返回压缩文本；若切片不属于任一已知格式返回 `None`（交由 gcc/make 等常规分支处理）。
/// 可读性约定：全局符号（大写类型）逐条保留 `$NM {type} {name}`，
/// 仅对小写局部符号按类型聚合为 `$NM local: t=N, r=N` 摘要，保证下游 LLM 仍能读符号名。
#[tracing::instrument(level = "trace", skip_all)]
fn compress_binutils_block(text: &str) -> Option<String> {
    // objdump -d 反汇编（`<func>:` 标签 + hex:偏移指令行）特征独立，优先判定。
    if has_objdump_disasm_block(text) {
        return Some(compact_objdump_disasm(text));
    }
    // objdump -r 重定位表（`RELOCATION RECORDS FOR`/`Relocation section` 头 + R_* 行）与
    // 反汇编/符号表/节表都不共享行特征，须在 objdump 分支后独立判定。
    if has_objdump_reloc_block(text) {
        return Some(compact_objdump_relocations(text));
    }
    // objdump -t 为 6 列符号表，其行也满足 nm 的行级特征，必须先判定 objdump
    // 再走 nm，否则 objdump 会被 nm 分支误读。
    if count_consecutive_objdump_symbol_rows(text).is_some() {
        return Some(compact_objdump_symbols(text));
    }
    if count_consecutive_nm_symbol_lines(text).is_some() {
        return Some(compact_nm_symbols(text));
    }
    if text.lines().any(|l| SIZE_HEADER_RE.is_match(l)) {
        return Some(compact_size_table(text));
    }
    if text
        .lines()
        .any(|l| BINUTILS_SECTION_HEADER_RE.is_match(l) || READELF_SECTION_HEADER_RE.is_match(l))
    {
        return Some(compact_binutils_sections(text));
    }
    // ar rcs 创建 verbose 的 `a - <member>` 行经 trim 后也满足 AR_MEMBER_RE，
    // 必须先判定创建场景（写操作）再走 ar -t 只读清单。
    if has_ar_create_block(text).is_some() {
        return Some(compact_ar_create(text));
    }
    if find_ar_member_block(text).is_some() {
        return Some(compact_ar_members(text));
    }
    None
}

/// 折叠 nm 符号表：全局符号（大写类型）逐条保留地址已剥除的 `$NM type name`，
/// 小写局部符号按类型聚合统计。
#[tracing::instrument(level = "trace", skip_all)]
fn compact_nm_symbols(text: &str) -> String {
    let mut out = String::new();
    let mut local_counts: HashMap<String, usize> = HashMap::new();
    let mut local_total = 0usize;

    for line in text.lines() {
        if !NM_SYMBOL_LINE_RE.is_match(line) {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        // nm 三列格式：{addr} {type} {name}；未定义符号行为「空白占位 {type} {name}」→ 两列。
        let (typ, name) = match parts.as_slice() {
            [_addr, b, c] => (*b, *c),
            [a, b] if is_nm_type_token(a) => (*a, *b),
            _ => continue,
        };
        let first = typ.chars().next().unwrap_or('?');
        if first.is_ascii_uppercase() || first == '?' {
            // 全局符号：逐条保留（含 U 未定义，指向链接缺口），保留 v/P 等弱符号语义
            out.push_str("$NM ");
            out.push_str(typ);
            out.push(' ');
            out.push_str(name);
            out.push('\n');
        } else {
            let key = typ.to_string();
            *local_counts.entry(key).or_insert(0) += 1;
            local_total += 1;
        }
    }

    if local_total > 0 {
        out.push_str("$NM local:");
        let mut keys: Vec<_> = local_counts.iter().collect();
        keys.sort_by_key(|(k, _)| *k);
        for (k, v) in keys {
            out.push(' ');
            out.push_str(k);
            out.push('=');
            out.push_str(&v.to_string());
        }
        out.push('\n');
    }
    out
}

/// 判断是否为 nm 类型字母（单字母或 '?'），用于解析两列格式的未定义符号行。
#[tracing::instrument(level = "trace", skip_all)]
fn is_nm_type_token(s: &str) -> bool {
    let mut ch = s.chars();
    (s.len() == 1) && matches!(ch.next(), Some(c) if c.is_ascii_alphabetic() || c == '?')
}

/// 折叠 size 输出：表头 + 每数据行去冗余 hex 列，保留 text/data/bss/dec 与文件名。
#[tracing::instrument(level = "trace", skip_all)]
fn compact_size_table(text: &str) -> String {
    let mut out = String::from("$SIZE\n");
    for line in text.lines() {
        if SIZE_HEADER_RE.is_match(line) {
            continue; // 表头本身已由 $SIZE 标记表达
        }
        if let Some(caps) = SIZE_ROW_RE.captures(line) {
            let t = &caps[1];
            let d = &caps[2];
            let b = &caps[3];
            let dec = &caps[4];
            let file = &caps[6];
            out.push_str("$SIZE ");
            out.push_str(file.trim());
            out.push_str(" text=");
            out.push_str(t);
            out.push_str(" data=");
            out.push_str(d);
            out.push_str(" bss=");
            out.push_str(b);
            out.push_str(" dec=");
            out.push_str(dec);
            out.push('\n');
        }
    }
    out
}

/// 折叠 objdump/readelf 节表：保留节名与 Size，折叠 VMA/File off 等冗余列。
#[tracing::instrument(level = "trace", skip_all)]
fn compact_binutils_sections(text: &str) -> String {
    let mut out = String::new();
    // 节表数据行启发式：Idx 数字 + 节名 + 十六进制 Size 等列。
    // 用宽松正则提取「节名 + 首个十六进制 Size 值」，其余列折叠。
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty()
            || BINUTILS_SECTION_HEADER_RE.is_match(t)
            || READELF_SECTION_HEADER_RE.is_match(t)
        {
            continue;
        }
        // readelf -S 行：`[ 1] .interp PROGBITS <addr> <off> <size> ...`，size 在第 5 列 parts[5]。
        if let Some(caps) = READELF_SECTION_ROW_RE.captures(t) {
            let name = &caps[1];
            // 空节（`NULL`）行名会被捕获为 NULL，无节信息可折叠，跳过。
            if name == "NULL" {
                continue;
            }
            let size = &caps[2];
            out.push_str("$SECTION ");
            out.push_str(name);
            if !size.is_empty() {
                out.push_str(" size=");
                out.push_str(size);
            }
            out.push('\n');
            continue;
        }
        let parts: Vec<&str> = t.split_whitespace().collect();
        // objdump -h 形如：Idx Name Size VMA ... Algn；节名通常含 '.' 或为首个非数字 token。
        if parts.len() >= 3 && parts[0].parse::<usize>().is_ok() {
            let name = parts[1];
            let size = parts.get(2).copied().unwrap_or("");
            out.push_str("$SECTION ");
            out.push_str(name);
            if !size.is_empty() {
                out.push_str(" size=");
                out.push_str(size);
            }
            out.push('\n');
        }
    }
    out
}

/// 折叠 objdump -t 符号表：全局/弱符号（flag g/w）逐条保留 `$OBJ {type} {name}`，
/// 局部符号（flag l）按类型聚合为 `$OBJ local: type=N` 摘要。type 与 nm 语义一致（F/O/d/...），
/// 缺失 type 列（如 `__bss_start`）以 `?` 占位；objdump 地址/Size 列全部折叠。
#[tracing::instrument(level = "trace", skip_all)]
fn compact_objdump_symbols(text: &str) -> String {
    let mut out = String::new();
    let mut local_counts: HashMap<String, usize> = HashMap::new();
    let mut local_total = 0usize;

    for line in text.lines() {
        let Some((flag, typ, name)) = parse_objdump_symbol_row(line) else {
            continue;
        };
        let t = if typ.is_empty() { "?" } else { typ };
        if flag == "l" {
            *local_counts.entry(t.to_string()).or_insert(0) += 1;
            local_total += 1;
        } else {
            // 全局（g）/ 弱（w）符号逐条保留，便于下游 LLM 读取符号语义。
            out.push_str("$OBJ ");
            out.push_str(t);
            out.push(' ');
            out.push_str(name);
            out.push('\n');
        }
    }

    if local_total > 0 {
        out.push_str("$OBJ local:");
        let mut keys: Vec<_> = local_counts.iter().collect();
        keys.sort_by_key(|(k, _)| *k);
        for (k, v) in keys {
            out.push(' ');
            out.push_str(k);
            out.push('=');
            out.push_str(&v.to_string());
        }
        out.push('\n');
    }
    out
}

/// 折叠 ar -t 归档成员清单：逐行细化成员名会叠加前缀/分隔符导致净扩张，
/// 因此折叠为计数 + 按扩展名分组的摘要 `$AR archive N members (.o=12, .c=4)`，
/// 与 nm 局部符号按类型聚合的方式一致：LLM 可读归档规模与组成，字节量必然收缩。
#[tracing::instrument(level = "trace", skip_all)]
fn compact_ar_members(text: &str) -> String {
    let members: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && AR_MEMBER_RE.is_match(l))
        .collect();
    let n = members.len();
    let mut ext_counts: HashMap<String, usize> = HashMap::new();
    for m in &members {
        let ext = std::path::Path::new(m)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        *ext_counts.entry(ext).or_insert(0) += 1;
    }
    let mut out = format!("$AR archive {n} members");
    if !ext_counts.is_empty() {
        out.push_str(" (");
        let mut keys: Vec<_> = ext_counts.iter().collect();
        keys.sort_by_key(|(k, _)| *k);
        let parts: Vec<String> = keys.iter().map(|(k, v)| format!("{k}={v}")).collect();
        out.push_str(&parts.join(", "));
        out.push(')');
    }
    out
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
        // P2-73：file 列兼容 Windows 盘符前缀（`C:\src\main.c` / `C:/src/main.c`）。
        // 可选驱动前缀 + 反斜杠，其余仍不允许 `:`，避免吞掉 `:\d+:\d+:` 定位分隔符。
        let error_pattern = Regex::new(r"(?:^|\]\s*)(?P<file>(?:[A-Za-z]:\\?)?[^:\n\[\]]+):(?P<line>\d+):(?P<col>\d+):\s*(?P<lvl>error|warning|note|fatal error): (?P<msg>.*)$").unwrap();

        GccLogPlugin {
            name: "gcc_log",
            priority: 150,
            gcc_pattern: Arc::new(gcc_pattern),
            make_pattern: Arc::new(make_pattern),
            cmake_pattern: Arc::new(cmake_pattern),
            error_pattern: Arc::new(error_pattern),
        }
    }

    /// 标准化压缩单行：按 Ninja/CTest/CMake/链接器/诊断错误/make/跳过 等模式依次匹配压缩。
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

    /// 压缩 CTest 输出行：失败/超时/通过的测试条目压缩为 $CTEST 行。
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

    /// 将文本中的路径替换为字典 token（skeleton），仅替换含路径分隔符或 -I/-L 前缀的候选。
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

    /// P2-20（C-5）：CRLF → LF 归一化，供逐行压缩前清洗。
    ///
    /// Windows 生成的 gcc 日志以 `\r\n` 结尾，`str::lines()` 只按 `\n` 切分、把 `\r`
    /// 留在行尾——行尾 `warning:`/`error:` 等前缀匹配与输出都会混入 `\r`。仅在文本含
    /// `\r` 时分配归一；无 `\r` 直接借原串零拷贝。
    fn normalize_crlf<'a>(&self, text: &'a str) -> Cow<'a, str> {
        if !text.as_bytes().contains(&b'\r') {
            return Cow::Borrowed(text);
        }
        // `\r\n` → `\n`，孤立 `\r` 一并清除
        Cow::Owned(text.replace("\r\n", "\n").replace('\r', ""))
    }

    /// 将文本中的宏替换为字典 token（当前实现为空操作，原样返回）。
    fn replace_macros_in_text<'a>(
        &self,
        text: &'a str,
        _dict_engine: &mut DictionaryEngine,
        arena: &'a Bump,
    ) -> Cow<'a, str> {
        let _ = arena;
        Cow::Borrowed(text)
    }

    /// 合并后的 gcc 压缩核心：binutils 短路 → 两遍（统计 + 逐行折叠生成）→ 摘要 → ROI 门控。
    /// 第二遍逐行归一化由 `context` 可选控制（compress 走 identity，with_context 走 convert_line）。
    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_text<'a>(
        &self,
        text: &str,
        dict_engine: &mut DictionaryEngine,
        arena: &'a Bump,
        mut context: Option<&mut CompressionContext>,
    ) -> CompressResult<'a> {
        // binutils 分析输出（nm/size/objdump·readelf 节表）整块短路，不做逐行 gcc 诊断处理。
        if let Some(binutils_text) = compress_binutils_block(text) {
            let final_text = crate::core::utils::roi::prefer_non_expanding(text, binutils_text);
            let final_in_arena = arena.alloc_str(&final_text);
            return CompressResult {
                tokens: vec![Token::Text(Cow::Borrowed(final_in_arena))],
                metadata: None,
                plugin_name: Some(self.name()),
            };
        }

        // 第一遍：收集统计信息（始终基于原始行分类）
        let mut stats = BuildStats::new();
        let lines: Vec<&str> = text.lines().collect();
        for (line_num, line) in lines.iter().enumerate() {
            stats.classify(line, line_num);
        }

        // 第二遍：生成压缩输出（应用折叠规则，逐行可选归一化）
        let mut tokens: Vec<Token<'a>> = Vec::new();
        let mut folded_warnings: HashMap<String, bool> = HashMap::new(); // 记录已折叠的警告类型
        // P2-19：诊断上下文行折叠改为带状态——仅在诊断头（error/warning/note/-->）
        // 之后的块内折叠 `N | ...` / `| ...` 行，块外原样保留；折叠内容以一行摘要
        // 占位（对齐其他插件 `[STACK] N frames` 口径），消除对 Gradle 依赖树
        // `|    +--- ...`、竖线表格/CSV 等形状相似行的无状态误伤。
        let mut diag_block = false;
        let mut diag_omitted: usize = 0;

        for (line_num, line) in lines.iter().enumerate() {
            // 归一化临时 Cow，保证 cur 的借用在整个迭代内有效
            let normalized_cow;
            let cur: &str = if let Some(ctx) = context.as_deref_mut() {
                normalized_cow = ctx.convert_line(Cow::Borrowed(line));
                normalized_cow.as_ref()
            } else {
                line
            };

            // 检查是否是需要折叠的警告
            let mut should_skip = false;
            if cur.contains("warning:") {
                if let Some(warning_type) = gcc_warning_signature(cur) {
                    if stats.should_fold_warning(line_num, &warning_type) {
                        should_skip = true;
                        if let std::collections::hash_map::Entry::Vacant(e) =
                            folded_warnings.entry(warning_type)
                        {
                            // P2-21：Entry API 单次哈希查找替代 contains_key+insert 双查，
                            // 且经 e.key() 借用免除 warning_type.clone() 分配。
                            let summary = stats.generate_fold_summary(e.key());
                            e.insert(true);
                            if let Some(summary) = summary {
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
            if should_skip {
                // warning 头本身开启诊断块，其后源码引用行进入折叠范围（P2-19）
                diag_block = true;
            }

            if !should_skip {
                if is_gcc_diagnostic_context_line(cur) {
                    if diag_block {
                        // 诊断块内：折叠计数，不产生 token
                        diag_omitted += 1;
                        continue;
                    }
                    // 诊断块外形状相似的行：保留（P2-19 误伤防护），落到下方正常压缩
                } else {
                    // 非上下文行宣告当前诊断块结束：先冲刷折叠摘要
                    if diag_omitted > 0 {
                        let flush = bumpalo::format!(
                            in arena,
                            "[DIAG] {} context lines omitted
",
                            diag_omitted
                        );
                        tokens.push(Token::Text(Cow::Borrowed(flush.into_bump_str())));
                        diag_omitted = 0;
                    }
                    diag_block = is_gcc_diagnostic_header_line(cur);
                }
                let compressed = self.compress_line_standardized(cur, dict_engine, arena);
                let with_newline = bumpalo::format!(in arena, "{}\n", compressed);
                tokens.push(Token::Text(Cow::Borrowed(with_newline.into_bump_str())));
            }
        }

        // 冲刷文本末尾仍悬挂的诊断折叠摘要
        if diag_omitted > 0 {
            let flush = bumpalo::format!(
                in arena,
                "[DIAG] {} context lines omitted
",
                diag_omitted
            );
            tokens.push(Token::Text(Cow::Borrowed(flush.into_bump_str())));
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
}

impl Plugin for GccLogPlugin {
    /// 返回插件名称 "gcc_log"。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级 150。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：对 gcc/make/cmake/ninja/error 标记等特征累加置信度分，>0.3 时命中。
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
        // binutils 分析工具：nm 符号表需 ≥3 行连续命中才累加强置信度（强特征门槛，
        // 避免单行十六进制+类型字母的普通日志误判）；size 需表头特征；objdump/readelf 识别节表头。
        if let Some(hits) = count_consecutive_nm_symbol_lines(text) {
            if hits >= 3 {
                score += 1.0;
            } else if hits >= 1 {
                score += 0.35;
            }
        }
        if let Some(hits) = count_consecutive_objdump_symbol_rows(text) {
            if hits >= 3 {
                score += 1.0;
            } else {
                score += 0.4;
            }
        }
        // objdump -d 反汇编：节头/函数标签/指令行为强特征。
        if has_objdump_disasm_block(text) {
            score += 0.9;
        }
        // objdump -r 重定位表：节头 + R_* 入口行是链接缺口判定关键，强特征。
        if has_objdump_reloc_block(text) {
            score += 0.9;
        }
        // objdump -t/ar 表头也是 binutils 强特征。
        if text.contains("SYMBOL TABLE:") {
            score += 0.4;
        }
        // ar -t 归档成员清单：纯成员名块具备强特征门槛，加保守置信度。
        if find_ar_member_block(text).is_some() {
            score += 0.7;
        }
        // ar rcs 创建归档 verbose：`a - <member>` + 创建信号是写操作特征。
        if has_ar_create_block(text).is_some() {
            score += 0.7;
        }
        for line in text.lines() {
            if SIZE_HEADER_RE.is_match(line)
                || BINUTILS_SECTION_HEADER_RE.is_match(line)
                || READELF_SECTION_HEADER_RE.is_match(line)
            {
                score += 0.9;
            }
            if SIZE_ROW_RE.is_match(line) && SIZE_HEADER_RE.is_match(text) {
                score += 0.4;
            }
        }
        if score > 0.3 {
            Some(score.min(1.0))
        } else {
            None
        }
    }

    /// 压缩切片：复用核心逻辑（不做归一化）。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a> {
        // P2-20（C-5）：Windows CRLF 日志先归一为 LF，避免 `\r` 残留行尾破坏前缀匹配。
        let text = self.normalize_crlf(slice.text.as_ref());
        self.compress_text(text.as_ref(), dict_engine, arena, None)
    }

    /// 带上下文压缩：复用核心逻辑，逐行经 CompressionContext 归一化。
    fn compress_with_context<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
        context: &mut CompressionContext,
    ) -> CompressResult<'a> {
        // P2-20（C-5）：Windows CRLF 日志先归一为 LF，避免 `\r` 残留行尾破坏前缀匹配。
        let text = self.normalize_crlf(slice.text.as_ref());
        self.compress_text(text.as_ref(), dict_engine, arena, Some(context))
    }

    /// 解压：原文透传（gcc 压缩标记无需字典还原）。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }
}
