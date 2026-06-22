#![allow(dead_code)]
//! Fossil 压缩方法 — Compression Protocol V1
use super::parser::*;
use crate::core::plugin_config_loader::parse_vcs_command_words_from_line;

// ============================================================================
// 遗留 parser 集成
// ============================================================================
#[tracing::instrument(level = "debug", skip_all)]
pub fn process_parser(parser: &dyn VcsParser, raw: &str) -> String {
    if let Some(doc) = parser.parse(raw) {
        let out: String = doc.records.iter().map(|r| format!("{}\n", r)).collect();
        let trimmed = out.trim();
        if trimmed.is_empty() {
            raw.to_string()
        } else {
            trimmed.to_string()
        }
    } else {
        raw.to_string()
    }
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_fossil_status_block(text: &str) -> bool {
    fossil_subcommand_is(text, &["status", "changes"])
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_fossil_log_block(text: &str) -> bool {
    fossil_subcommand_is(text, &["log", "timeline", "undo", "stash", "merge", "sync"])
}

// ============================================================================
// 公开 API — 全部包裹 anchor_guard
// ============================================================================
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_fossil_status_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_fossil_dispatch(raw))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_fossil_diff_for_ai(raw: &str) -> String {
    // diff --brief 输出与 changes 格式相同（ADDED/MODIFIED/DELETED），走 status 映射
    if raw.contains("ADDED ")
        || raw.contains("MODIFIED ")
        || raw.contains("DELETED ")
        || raw.contains("EDITED ")
    {
        return anchor_guard(raw, || compact_fossil_status_cmd(raw));
    }
    anchor_guard(raw, || process_parser(&FossilDiffParser, raw))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_fossil_log_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_fossil_dispatch(raw))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_fossil_other_for_ai(raw: &str) -> String {
    compact_fossil_dispatch(raw)
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_fossil_log_family_for_ai(raw: &str) -> String {
    compact_fossil_log_for_ai(raw)
}

/// 全局锚点兜底
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
fn compact_fossil_dispatch(raw: &str) -> String {
    if raw.len() < 50 {
        return raw.to_string();
    }
    let first = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();

    if let Some((tool, words)) = parse_vcs_command_words_from_line(first) {
        if tool == "fossil" {
            match words.first().map(String::as_str) {
                Some("status") | Some("changes") => return compact_fossil_status_cmd(raw),
                Some("timeline") | Some("log") => return compact_fossil_timeline(raw),
                Some("undo") => return compact_fossil_undo(raw),
                Some("stash") => return compact_fossil_stash(raw),
                Some("merge") => return compact_fossil_merge(raw),
                Some("sync") => return compact_fossil_sync(raw),
                _ => {}
            }
        }
    }

    compact_fossil_generic(raw)
}

/// 使用统一 argv 解析识别 fossil 子命令，避免正文词汇触发误判。
fn fossil_subcommand_is(raw: &str, expected: &[&str]) -> bool {
    let first = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");
    let Some((tool, words)) = parse_vcs_command_words_from_line(first) else {
        return false;
    };
    if tool != "fossil" {
        return false;
    }
    let Some(sub) = words.first().map(String::as_str) else {
        return false;
    };
    expected.iter().any(|cmd| *cmd == sub)
}

// ============================================================================
// Case 29/152: status/changes — 保留锚点，状态码映射 M:/A:/D:
// ============================================================================
fn compact_fossil_status_cmd(raw: &str) -> String {
    let mut out = Vec::new();
    let mut got_anchor = false;

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if !got_anchor && (t.starts_with("fossil status") || t.starts_with("fossil changes")) {
            out.push(t.to_string());
            got_anchor = true;
            continue;
        }
        if t.starts_with("fossil ") {
            continue;
        }

        // 跳过仓库元数据噪音
        if is_fossil_meta_noise(t) {
            continue;
        }

        // 状态码映射: EDITED→M, ADDED→A, DELETED→D, RENAMED→R, CONFLICT→!
        if let Some(mapped) = map_fossil_status(t) {
            out.push(mapped);
            continue;
        }

        // 单字符状态码行: "M src/file.rs"
        if let Some(c) = t.chars().next() {
            if matches!(c, 'M' | 'A' | 'D' | 'R') && t.len() > 2 {
                let rest = t[1..].trim();
                out.push(format!("{}:{}", c, rest));
                continue;
            }
        }

        out.push(t.to_string());
    }
    out.join("\n")
}

fn is_fossil_meta_noise(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.starts_with("repository:")
        || l.starts_with("local-root:")
        || l.starts_with("checkout:")
        || l.starts_with("tags:")
        || l.starts_with("comment:")
}

fn map_fossil_status(line: &str) -> Option<String> {
    let (code, rest) = if let Some(r) = line.strip_prefix("EDITED ") {
        ('M', r)
    } else if let Some(r) = line.strip_prefix("MODIFIED ") {
        ('M', r)
    } else if let Some(r) = line.strip_prefix("ADDED ") {
        ('A', r)
    } else if let Some(r) = line.strip_prefix("DELETED ") {
        ('D', r)
    } else if let Some(r) = line.strip_prefix("RENAMED ") {
        ('R', r)
    } else if let Some(r) = line.strip_prefix("CONFLICT ") {
        ('!', r)
    } else {
        return None;
    };
    Some(format!("ST:{} {}", code, rest.trim()))
}

// ============================================================================
// Case 41/104: timeline/log — 保留锚点，哈希/作者符号化
// ============================================================================
fn compact_fossil_timeline(raw: &str) -> String {
    let mut out = Vec::new();
    let mut got_anchor = false;

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if !got_anchor && (t.starts_with("fossil timeline") || t.starts_with("fossil log")) {
            out.push(t.to_string());
            got_anchor = true;
            continue;
        }
        if t.starts_with("fossil ") {
            continue;
        }

        // 跳过 user: 行（信息已在 bracketed 格式中）
        if t.to_ascii_lowercase().starts_with("user:") {
            continue;
        }

        // [hash] date [author] subject → @hash date OW:@author subject
        if t.starts_with('[') {
            if let Some(formatted) = format_fossil_commit(t) {
                out.push(formatted);
                continue;
            }
        }

        // 纯日期格式: "2026-04-08 alice 1abc123 Add new feature"
        if let Some(formatted) = format_fossil_simple_commit(t) {
            out.push(formatted);
            continue;
        }

        out.push(t.to_string());
    }
    out.join("\n")
}

fn format_fossil_commit(line: &str) -> Option<String> {
    let rest = line.strip_prefix('[')?;
    let hash_end = rest.find(']')?;
    let hash = &rest[..hash_end];
    let after = rest[hash_end + 1..].trim();

    // 提取 date: first two space-separated tokens
    let parts: Vec<&str> = after.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return None;
    }
    let date = format!("{} {}", parts[0], parts[1]);

    // 从 after 中定位 author 方括号，提取 author 和 subject
    let mut author = String::new();
    let mut subject = String::new();
    if let Some(bracket_start) = after.find('[') {
        if let Some(bracket_end) = after[bracket_start..].find(']') {
            let abs_end = bracket_start + bracket_end;
            author = after[bracket_start + 1..abs_end].to_string();
            let subj_start = abs_end + 2; // 跳过 "] "
            if subj_start < after.len() {
                subject = after[subj_start..].trim().to_string();
            }
        }
    }

    let mut result = format!("@{} {}", hash, date);
    if !author.is_empty() {
        result.push_str(&format!(" OW:@{}", author));
    }
    if !subject.is_empty() {
        result.push_str(&format!(" {}", subject));
    }
    Some(result)
}

fn format_fossil_simple_commit(line: &str) -> Option<String> {
    // "2026-04-08 alice 1abc123 Add new feature"
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }
    if parts[0].len() != 10 || parts[0].as_bytes().get(4) != Some(&b'-') {
        return None;
    }

    let date = parts[0];
    let author = parts[1];
    let hash = parts[2];
    let subject = parts[3..].join(" ");

    Some(format!("@{} {} OW:@{} {}", hash, date, author, subject))
}

// ============================================================================
// Case 153: undo — 保留锚点，极简 REVERT:
// ============================================================================
fn compact_fossil_undo(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("fossil undo") {
            out.push(t.to_string());
            continue;
        }
        // "Undo successful: changes reverted to version abc123" → "REVERT:@abc123"
        if let Some(idx) = t.rfind("version ") {
            let ver = t[idx + 8..].trim();
            out.push(format!("REVERT:@{}", ver));
            continue;
        }
        if is_fossil_narrative(t) {
            continue;
        }
    }
    out.join("\n")
}

// ============================================================================
// Case 194: stash — 保留锚点，抹除头部废话
// ============================================================================
fn compact_fossil_stash(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("fossil stash") {
            out.push(t.to_string());
            continue;
        }
        if is_fossil_narrative(t) || t == "Stash changes:" {
            continue;
        }
        out.push(t.to_string());
    }
    out.join("\n")
}

// ============================================================================
// Case 195: merge — 保留锚点，抹除网络废话
// ============================================================================
fn compact_fossil_merge(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("fossil merge") {
            out.push(t.to_string());
            continue;
        }
        if is_fossil_narrative(t) {
            continue;
        }
        out.push(t.to_string());
    }
    out.join("\n")
}

// ============================================================================
// Case 196: sync — 保留锚点，极简 Pull/Push
// ============================================================================
fn compact_fossil_sync(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("fossil sync") {
            out.push(t.to_string());
            continue;
        }
        if is_fossil_narrative(t) {
            continue;
        }
        // Keep Pull/Push lines, skip others
        if t.starts_with("Pull:") || t.starts_with("Push:") {
            out.push(t.to_string());
        }
    }
    out.join("\n")
}

// ============================================================================
// 通用 fallback
// ============================================================================
fn compact_fossil_generic(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("fossil ") && first {
            out.push(t.to_string());
            first = false;
            continue;
        }
        if is_fossil_narrative(t) || is_fossil_meta_noise(t) {
            continue;
        }
        if let Some(alert) = map_fossil_alert(t) {
            out.push(alert);
            continue;
        }
        if let Some(status) = map_fossil_status(t) {
            out.push(status);
            continue;
        }
        out.push(t.to_string());
    }
    out.join("\n")
}

// ============================================================================
// 辅助函数
// ============================================================================
fn is_fossil_narrative(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.starts_with("undo successful")
        || l == "done."
        || l.starts_with("sync with")
        || l.starts_with("pulling from")
        || l.starts_with("stash changes:")
}

pub(super) fn map_fossil_alert(line: &str) -> Option<String> {
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
