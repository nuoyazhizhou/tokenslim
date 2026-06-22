#![allow(dead_code)]
//! CVS 压缩方法 — Compression Protocol V1
use super::parser::*;
use crate::core::plugin_config_loader::parse_vcs_command_words_from_line;
use crate::core::utils::roi::prefer_non_expanding;

// ============================================================================
// 遗留 parser 集成
// ============================================================================
#[tracing::instrument(level = "debug", skip_all)]
pub fn process_parser(parser: &dyn VcsParser, raw: &str) -> String {
    if let Some(doc) = parser.parse(raw) {
        let out: String = doc.records.iter().map(|r| format!("{}\n", r)).collect();
        let t = out.trim();
        if t.is_empty() {
            raw.to_string()
        } else {
            t.to_string()
        }
    } else {
        raw.to_string()
    }
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_cvs_status_block(text: &str) -> bool {
    cvs_subcommand_is(text, &["status", "update", "edit"])
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_cvs_log_block(text: &str) -> bool {
    cvs_subcommand_is(text, &["log", "commit", "history", "tag", "annotate"])
}

// ============================================================================
// 公开 API — 全部 anchor_guard
// ============================================================================
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_cvs_status_for_ai(raw: &str) -> String {
    prefer_non_expanding(raw, anchor_guard(raw, || compact_cvs_dispatch(raw)))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_cvs_diff_for_ai(raw: &str) -> String {
    prefer_non_expanding(raw, anchor_guard(raw, || compact_cvs_diff_cmd(raw)))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_cvs_log_for_ai(raw: &str) -> String {
    prefer_non_expanding(raw, anchor_guard(raw, || compact_cvs_dispatch(raw)))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_cvs_other_for_ai(raw: &str) -> String {
    prefer_non_expanding(raw, compact_cvs_dispatch(raw))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_cvs_log_family_for_ai(raw: &str) -> String {
    compact_cvs_log_for_ai(raw)
}

fn anchor_guard(raw: &str, f: impl FnOnce() -> String) -> String {
    let anchor = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");
    let body = f();
    if body.starts_with(anchor) {
        return body;
    }
    if body.is_empty() || body == raw {
        return body;
    }
    format!("{}\n{}", anchor, body.trim_start())
}

// ============================================================================
// 调度器
// ============================================================================
fn compact_cvs_dispatch(raw: &str) -> String {
    if raw.len() < 30 {
        return raw.to_string();
    }
    let first = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();

    if let Some((tool, words)) = parse_vcs_command_words_from_line(first) {
        if tool == "cvs" {
            match words.first().map(String::as_str) {
                Some("update") | Some("checkout") => return compact_cvs_update(raw),
                Some("status") | Some("edit") => return compact_cvs_status(raw),
                Some("commit") => return compact_cvs_commit(raw),
                Some("tag") => return compact_cvs_tag(raw),
                Some("unedit") => return compact_cvs_unedit(raw),
                Some("log") => return compact_cvs_log_cmd(raw),
                _ => {}
            }
        }
    }

    compact_cvs_generic(raw)
}

/// 使用统一 argv 解析识别 cvs 子命令，避免正文词汇触发误判。
fn cvs_subcommand_is(raw: &str, expected: &[&str]) -> bool {
    let first = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");
    let Some((tool, words)) = parse_vcs_command_words_from_line(first) else {
        return false;
    };
    if tool != "cvs" {
        return false;
    }
    let Some(sub) = words.first().map(String::as_str) else {
        return false;
    };
    expected.iter().any(|cmd| *cmd == sub)
}

// ============================================================================
// Case 101/314: update/checkout — 保留锚点，状态码映射 U/A/R/?:
// ============================================================================
fn compact_cvs_update(raw: &str) -> String {
    let mut out = Vec::new();
    let mut got_anchor = false;

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if !got_anchor && t.starts_with("cvs ") {
            out.push(t.to_string());
            got_anchor = true;
            continue;
        }
        if t.starts_with("cvs ") {
            continue;
        }

        // 剔除叙述噪音
        if is_cvs_noise(t) {
            continue;
        }

        // 状态码映射: U → U:, A → A:, R → R:, ? → ?:, M → M:
        if let Some(mapped) = map_cvs_status(t) {
            out.push(mapped);
            continue;
        }

        out.push(t.to_string());
    }
    out.join("\n")
}

fn map_cvs_status(line: &str) -> Option<String> {
    let t = line.trim();
    if t.len() < 2 {
        return None;
    }
    let first = t.chars().next()?;
    let code = match first {
        'U' | 'M' | 'A' | 'R' | 'C' | '?' | 'P' => first,
        _ => return None,
    };
    // 第一个字符是状态码，且接下来是空格
    if t.as_bytes().get(1) != Some(&b' ') {
        return None;
    }
    let path = t[2..].trim();
    if path.is_empty() {
        return None;
    }
    Some(format!("ST:{} {}", code, path))
}

// ============================================================================
// Case 315: status -v — 保留锚点，扁平化 KV
// ============================================================================
fn compact_cvs_status(raw: &str) -> String {
    let mut out = Vec::new();
    let mut got_anchor = false;

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if !got_anchor && t.starts_with("cvs ") {
            out.push(t.to_string());
            got_anchor = true;
            continue;
        }
        if t.starts_with("cvs ") {
            continue;
        }

        // 跳过分隔线
        if t.len() >= 8 && t.chars().all(|c| c == '=') {
            continue;
        }

        if is_cvs_noise(t) {
            continue;
        }

        // 状态码映射
        if let Some(mapped) = map_cvs_status(t) {
            out.push(mapped);
            continue;
        }

        // 扁平化 KV: "File: main.rs           Status: Locally Modified"
        if t.contains(':') {
            let compressed = compress_cvs_kv(&collapse_ws(t));
            out.push(compressed);
            continue;
        }

        // 标签行: "v2_1_0 (revision: 1.5)"
        if t.starts_with("v") || t.starts_with("HEAD") || t.starts_with("release") {
            out.push(collapse_ws(t));
            continue;
        }

        out.push(t.to_string());
    }
    out.join("\n")
}

// ============================================================================
// Case 102: commit — 保留锚点，CM: 映射
// ============================================================================
fn compact_cvs_commit(raw: &str) -> String {
    let mut out = Vec::new();
    let mut got_anchor = false;
    let mut current_file: Option<String> = None;
    let mut current_rev: Option<String> = None;

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if !got_anchor && t.starts_with("cvs ") {
            out.push(t.to_string());
            got_anchor = true;
            continue;
        }
        if t.starts_with("cvs ") {
            continue;
        }

        // "Checking in src/main.rs;" — 提取文件名（必须在噪音检查之前）
        if let Some(rest) = t.strip_prefix("Checking in ") {
            current_file = Some(rest.trim().trim_end_matches(';').to_string());
            continue;
        }

        if is_cvs_noise(t) {
            continue;
        }

        // 版本号: "1.2"
        if t.chars().all(|c| c.is_ascii_digit() || c == '.') && t.len() >= 3 {
            current_rev = Some(t.to_string());
            continue;
        }

        // 提交信息 → CM:file@rev message
        if let Some(file) = &current_file {
            let rev = current_rev.as_deref().unwrap_or("?");
            out.push(format!("CM:{}@{} {}", file, rev, t));
            current_file = None;
            current_rev = None;
            continue;
        }
    }
    out.join("\n")
}

// ============================================================================
// Case 146: tag — 保留锚点，T: 映射
// ============================================================================
fn compact_cvs_tag(raw: &str) -> String {
    let mut out = Vec::new();
    let mut got_anchor = false;

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if !got_anchor && t.starts_with("cvs ") {
            out.push(t.to_string());
            got_anchor = true;
            continue;
        }
        if t.starts_with("cvs ") {
            continue;
        }

        if is_cvs_noise(t) {
            continue;
        }

        // "T src/main.java" → "ST:T src/main.java"
        if t.starts_with("T ") {
            out.push(format!("ST:T {}", t[2..].trim()));
            continue;
        }

        if let Some(mapped) = map_cvs_status(t) {
            out.push(mapped);
            continue;
        }
    }
    out.join("\n")
}

// ============================================================================
// Case 27: log — 保留锚点，CP V1 符号化
// ============================================================================
fn compact_cvs_log_cmd(raw: &str) -> String {
    let mut out = Vec::new();
    let mut got_anchor = false;

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if !got_anchor && t.starts_with("cvs ") {
            out.push(t.to_string());
            got_anchor = true;
            continue;
        }
        if t.starts_with("cvs ") {
            continue;
        }

        if t.len() >= 8 && t.chars().all(|c| c == '-') {
            continue;
        }
        if is_cvs_noise(t) {
            continue;
        }

        // 状态码映射
        if let Some(mapped) = map_cvs_status(t) {
            out.push(mapped);
            continue;
        }

        // RCS file / commit / revision 标记 — 保留
        if t.starts_with("RCS file:")
            || t.starts_with("commit ")
            || t.starts_with("revision ")
            || t.starts_with("date:")
            || t.starts_with("author:")
            || t.starts_with("state:")
            || t.starts_with("lines:")
            || t.starts_with("branches:")
        {
            out.push(collapse_ws(t));
            continue;
        }

        if is_cvs_log_boilerplate(t) {
            continue;
        }

        out.push(t.to_string());
    }
    out.join("\n")
}

fn is_cvs_log_boilerplate(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.starts_with("head:")
        || l.starts_with("branch:")
        || l.starts_with("locks:")
        || l.starts_with("access list:")
        || l.starts_with("symbolic names:")
        || l.starts_with("keyword substitution:")
        || l.starts_with("total revisions:")
        || l.starts_with("description:")
        || l.starts_with("working file:")
}

// ============================================================================
// 通用 fallback
// ============================================================================
fn compact_cvs_generic(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("cvs ") && first {
            out.push(t.to_string());
            first = false;
            continue;
        }
        if is_cvs_noise(t) {
            continue;
        }
        if t.len() >= 8 && t.chars().all(|c| c == '=') {
            continue;
        }
        if let Some(mapped) = map_cvs_status(t) {
            out.push(mapped);
            continue;
        }
        if is_cvs_log_boilerplate(t) {
            continue;
        }
        if let Some(alert) = map_cvs_alert(t) {
            out.push(alert);
            continue;
        }
        out.push(collapse_ws(t));
    }
    out.join("\n")
}

// ============================================================================
// 辅助函数
// ============================================================================
fn is_cvs_noise(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.contains("updating ")
        || l.starts_with("checking in")
        || l.starts_with("cvs tag: tagging")
        || l.ends_with("files updated")
        || l.starts_with("new directory")
        || l.starts_with("annotations for")
        || l == "done"
        || l.starts_with("you have no outstanding edits")
}

fn collapse_ws(s: &str) -> String {
    let mut r = String::with_capacity(s.len());
    let mut last_ws = false;
    for c in s.replace('\t', " ").chars() {
        if c == ' ' {
            if !last_ws {
                r.push(' ');
                last_ws = true;
            }
        } else {
            r.push(c);
            last_ws = false;
        }
    }
    r.trim().to_string()
}

/// 压缩 status -v 的 KV: Working revision: → WR:, Repository revision: → RR:, etc.
fn compress_cvs_kv(s: &str) -> String {
    s.replace("Working revision:", "WR:")
        .replace("Repository revision:", "RR:")
        .replace("Sticky Tag:", "Tag:")
        .replace("Sticky Date:", "Date:")
        .replace("Sticky Options:", "Opts:")
        .replace("Locally Modified", "M")
        .replace("Locally Modified", "M")
        .replace("Up-to-date", "OK")
        .replace("Needs Patch", "NP")
}

/// Case 37/313: diff — 保留锚点，避免 ---/+++ 重复
fn compact_cvs_diff_cmd(raw: &str) -> String {
    let mut out = Vec::new();
    let mut got_anchor = false;
    let mut saw_index = false;

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if !got_anchor && t.starts_with("cvs ") {
            out.push(t.to_string());
            got_anchor = true;
            continue;
        }
        if t.starts_with("cvs ") {
            continue;
        }

        // 分隔线
        if t.len() >= 8 && t.chars().all(|c| c == '=') {
            continue;
        }

        // Index 行
        if t.starts_with("Index: ") {
            out.push(t.to_string());
            saw_index = true;
            continue;
        }

        // --- and +++ 行：仅在跟随 Index 时才跳过（parser 已产出了 DiffFile）
        if (t.starts_with("--- ") || t.starts_with("+++ ")) && saw_index {
            saw_index = false;
            continue;
        }

        out.push(t.to_string());
    }
    out.join("\n")
}

/// 处理 unedit 噪声: "You have no outstanding edits to X" → "No edits: X"
fn compact_cvs_unedit(raw: &str) -> String {
    let mut out = Vec::new();
    let mut got_anchor = false;
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if !got_anchor && t.starts_with("cvs ") {
            out.push(t.to_string());
            got_anchor = true;
            continue;
        }
        if is_cvs_noise(t) {
            // "You have no outstanding edits to src/main.java" → "No edits: src/main.java"
            if let Some(rest) = t.strip_prefix("You have no outstanding edits to ") {
                out.push(format!("No edits: {}", rest.trim()));
            }
            continue;
        }
        out.push(t.to_string());
    }
    out.join("\n")
}

pub(super) fn map_cvs_alert(line: &str) -> Option<String> {
    let l = line.to_ascii_lowercase();
    if ["conflict", "error:", "failed", "rejected"]
        .iter()
        .any(|t| l.contains(t))
    {
        let c = line.trim_start();
        Some(if c.starts_with('!') {
            c.to_string()
        } else {
            format!("!{}", c)
        })
    } else {
        None
    }
}

// legacy compatibility — called from vcs_plugin
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_cvs_history_lines(input: &str) -> String {
    input.to_string()
}
