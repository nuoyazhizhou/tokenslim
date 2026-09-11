//! 基础设施插件共享的小型辅助函数。

use crate::core::dictionary_engine::Dictionary;
use regex::Regex;
use std::sync::OnceLock;

pub struct ShowcaseCase {
    pub file_name: &'static str,
    pub title: &'static str,
}

/// 判断文本是否包含任一 needle 子串（needs 为空时返回 false）。
pub(crate) fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

/// 将压缩文本中的字典 token（形如 $P123）用词典解析替换还原；未命中的 token 原样保留。
pub(crate) fn decompress_with_dict(compressed: &str, dict: &Dictionary) -> String {
    static TOKEN_RE: OnceLock<Regex> = OnceLock::new();
    TOKEN_RE
        .get_or_init(|| Regex::new(r"(\$[A-Z]*\d+)").unwrap())
        .replace_all(compressed, |caps: &regex::Captures| {
            let token = caps.get(1).unwrap().as_str();
            dict.resolve(token).unwrap_or_else(|| token.to_string())
        })
        .into_owned()
}

/// 压缩结果保留错误信号：若原文含 error/fatal/panic 而压缩结果丢失了这些关键字，
/// 则把原文中首个含错误关键字的行追加到压缩结果末尾。
pub(crate) fn keep_error_signal(raw: &str, mut compacted: String) -> String {
    let raw_lower = raw.to_ascii_lowercase();
    if !contains_any(&raw_lower, &["error", "fatal", "panic"]) {
        return compacted;
    }
    let compact_lower = compacted.to_ascii_lowercase();
    if contains_any(&compact_lower, &["error", "fatal", "panic"]) {
        return compacted;
    }
    if let Some(line) = raw
        .lines()
        .find(|line| contains_any(&line.to_ascii_lowercase(), &["error", "fatal", "panic"]))
    {
        compacted.push('\n');
        compacted.push_str(line.trim());
    }
    compacted
}

/// 将文本中首个非空行（trim 后）作为锚点行压入输出行列表；文本为空或全空白时不添加。
pub(crate) fn push_anchor(lines: &mut Vec<String>, text: &str) {
    if let Some(anchor) = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
    {
        lines.push(anchor);
    }
}

/// 若锚点列表不超过 1 行，说明压缩无收益，回退返回原始文本；否则用换行拼接锚点列表。
pub(crate) fn fallback_if_anchor_only(lines: Vec<String>, text: &str) -> String {
    if lines.len() <= 1 {
        return text.to_string();
    }
    lines.join("\n")
}

/// 将文本中连续 2 个及以上的空格/制表符折叠为单个空格，并去除首尾空白。
pub(crate) fn compact_spaces(text: &str) -> String {
    static SPACE_RE: OnceLock<Regex> = OnceLock::new();
    SPACE_RE
        .get_or_init(|| Regex::new(r"[ \t]{2,}").unwrap())
        .replace_all(text.trim(), " ")
        .into_owned()
}

/// 判断一行是否含错误信号关键字（error/fatal/panic/failed/rollback，大小写不敏感）。
pub(crate) fn is_error_line(line: &str) -> bool {
    contains_any(
        &line.to_ascii_lowercase(),
        &["error", "fatal", "panic", "failed", "rollback"],
    )
}

/// 测试辅助：对每个样例文件执行压缩，生成包含原始与压缩对比的 showcase 报告，
/// 写入 target 目录下的指定报告文件。
#[cfg(test)]
pub fn write_showcase_report<P: crate::core::plugin_dispatcher::Plugin>(
    plugin: &P,
    sample_dir: &str,
    report_name: &str,
    cases: &[ShowcaseCase],
) {
    use crate::core::text_slicer::SliceType;
    use crate::plugins::test_utils::{compress_with_dict, full_dict_json, read_sample_file};

    let mut report = String::new();
    report.push_str(&"=".repeat(80));
    report.push_str(&format!("\n  {} Compact Showcase\n", plugin.name()));
    report.push_str(&"=".repeat(80));
    report.push_str("\n\n");

    for case in cases {
        let raw = read_sample_file(sample_dir, case.file_name);
        let (compacted, dict) = compress_with_dict(plugin, &raw, SliceType::LogBlock);
        let case_id = std::path::Path::new(case.file_name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(case.file_name);
        let original_lines = raw.lines().count();
        let original_bytes = raw.len();
        let compact_lines = if compacted.is_empty() {
            0
        } else {
            compacted.lines().count()
        };
        let compact_bytes = compacted.len();
        let compression = if original_bytes > 0 {
            (1.0 - compact_bytes as f64 / original_bytes as f64) * 100.0
        } else {
            0.0
        };

        report.push_str(&"-".repeat(80));
        report.push_str(&format!(
            "\nCase {} - {} ({})\n",
            case_id, case.title, case.file_name
        ));
        report.push_str(&"-".repeat(80));
        report.push_str(&format!(
            "\nOriginal: {} lines, {} bytes | Compact: {} lines, {} bytes | Compression: {:.1}%\n",
            original_lines, original_bytes, compact_lines, compact_bytes, compression
        ));
        report.push_str("-- Case text --\n");
        report.push_str(&"-".repeat(80));
        report.push('\n');
        report.push_str(&raw);
        if !report.ends_with('\n') {
            report.push('\n');
        }
        report.push_str("-- Compact Output (full) --\n");
        report.push_str(&"-".repeat(80));
        report.push('\n');
        report.push_str(&compacted);
        if !report.ends_with('\n') {
            report.push('\n');
        }
        // 字典侧通道（dictside）：仅在压缩产生可逆 token 时携带 `token->原文` 映射，
        // 供审计产物隔离到 compact.txt 后仍可逆解析，不改变上方 compact 段内容（哈希不变）。
        let dict_json = full_dict_json(&dict, &compacted);
        if !dict_json.is_empty() {
            report.push_str("-- Dictionary (full) --\n");
            report.push_str(&"-".repeat(80));
            report.push('\n');
            report.push_str(&dict_json);
            report.push('\n');
        }
    }

    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(report_name),
        report,
    )
    .unwrap();
}
