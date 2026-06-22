//! 基础设施插件共享的小型辅助函数。

use crate::core::dictionary_engine::Dictionary;
use regex::Regex;
use std::sync::OnceLock;

pub struct ShowcaseCase {
    pub file_name: &'static str,
    pub title: &'static str,
}

pub(crate) fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

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

pub(crate) fn fallback_if_anchor_only(lines: Vec<String>, text: &str) -> String {
    if lines.len() <= 1 {
        return text.to_string();
    }
    lines.join("\n")
}

pub(crate) fn compact_spaces(text: &str) -> String {
    static SPACE_RE: OnceLock<Regex> = OnceLock::new();
    SPACE_RE
        .get_or_init(|| Regex::new(r"[ \t]{2,}").unwrap())
        .replace_all(text.trim(), " ")
        .into_owned()
}

pub(crate) fn is_error_line(line: &str) -> bool {
    contains_any(
        &line.to_ascii_lowercase(),
        &["error", "fatal", "panic", "failed", "rollback"],
    )
}

#[cfg(test)]
pub fn write_showcase_report<P: crate::core::plugin_dispatcher::Plugin>(
    plugin: &P,
    sample_dir: &str,
    report_name: &str,
    cases: &[ShowcaseCase],
) {
    use crate::core::text_slicer::SliceType;
    use crate::plugins::test_utils::{compress_to_string, read_sample_file};

    let mut report = String::new();
    report.push_str(&"=".repeat(80));
    report.push_str(&format!("\n  {} Compact Showcase\n", plugin.name()));
    report.push_str(&"=".repeat(80));
    report.push_str("\n\n");

    for case in cases {
        let raw = read_sample_file(sample_dir, case.file_name);
        let compacted = compress_to_string(plugin, &raw, SliceType::LogBlock);
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
    }

    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(report_name),
        report,
    )
    .unwrap();
}
