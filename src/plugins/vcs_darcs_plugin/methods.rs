#![allow(dead_code)]
//! Darcs 压缩方法 — Compression Protocol V1
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
pub fn is_darcs_status_block(text: &str) -> bool {
    darcs_subcommand_is(text, &["status", "whatsnew"])
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_darcs_log_block(text: &str) -> bool {
    darcs_subcommand_is(text, &["log", "record", "amend", "obliterate", "changes"])
}

// ============================================================================
// 公开 API
// ============================================================================
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_darcs_status_for_ai(raw: &str) -> String {
    anchor_guard(raw, || process_parser(&DarcsStatusParser, raw))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_darcs_diff_for_ai(raw: &str) -> String {
    anchor_guard(raw, || process_parser(&DarcsDiffParser, raw))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_darcs_log_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_darcs_dispatch(raw))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_darcs_other_for_ai(raw: &str) -> String {
    compact_darcs_dispatch(raw)
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_darcs_log_family_for_ai(raw: &str) -> String {
    compact_darcs_log_for_ai(raw)
}

/// 全局锚点兜底：无条件提取原始输入第一行，强制拼接到输出开头
fn anchor_guard(raw: &str, f: impl FnOnce() -> String) -> String {
    let anchor = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");

    let body = f();

    // 如果输出已以锚点开头，直接返回
    if body.starts_with(anchor) {
        return body;
    }
    // 否则强插锚点到第一行
    if body.is_empty() || body == raw {
        return body;
    }
    format!("{}\n{}", anchor, body.trim_start())
}

// ============================================================================
// 调度器
// ============================================================================
fn compact_darcs_dispatch(raw: &str) -> String {
    if raw.len() < 50 {
        return raw.to_string();
    }
    let first = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();

    if let Some((tool, words)) = parse_vcs_command_words_from_line(first) {
        if tool == "darcs" {
            match words.first().map(String::as_str) {
                Some("log") | Some("changes") => return compact_darcs_log_cmd(raw),
                Some("status") | Some("whatsnew") => return compact_darcs_status_cmd(raw),
                Some("obliterate") => return compact_darcs_obliterate_cmd(raw),
                Some("amend") => return compact_darcs_amend_cmd(raw),
                Some("rebase") => return compact_darcs_rebase_cmd(raw),
                Some("record") => return compact_darcs_record_cmd(raw),
                _ => {}
            }
        }
    }

    compact_darcs_generic(raw)
}

/// 使用统一 argv 解析识别 darcs 子命令，避免正文关键字污染。
fn darcs_subcommand_is(raw: &str, expected: &[&str]) -> bool {
    let first = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");
    let Some((tool, words)) = parse_vcs_command_words_from_line(first) else {
        return false;
    };
    if tool != "darcs" {
        return false;
    }
    let Some(sub) = words.first().map(String::as_str) else {
        return false;
    };
    expected.iter().any(|cmd| *cmd == sub)
}

// ============================================================================
// Case 35/321: log — 保留锚点，结构化提取，CP V1 符号化
// ============================================================================
fn compact_darcs_log_cmd(raw: &str) -> String {
    let mut out = Vec::new();

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        // 保留所有命令锚点（darcs log, darcs changes）
        if t.starts_with("darcs log") || t.starts_with("darcs changes") {
            out.push(t.to_string());
            continue;
        }
    }

    // 复用结构化提取
    if let Some(structured) = compact_darcs_log_structured(raw) {
        // structured 的第一行已经是命令锚点，第二行开始是补丁信息
        let lines: Vec<&str> = structured.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if i == 0 {
                continue;
            } // skip anchor (already pushed)
            out.push(line.to_string());
        }
    }

    if out.len() <= 1 {
        return cost_gate(raw, out.join("\n"));
    }
    cost_gate(raw, out.join("\n"))
}

/// 复用原有结构化提取，但输出格式对齐 CP V1
fn compact_darcs_log_structured(input: &str) -> Option<String> {
    #[derive(Default)]
    struct Patch {
        hash: Option<String>,
        author: Option<String>,
        date: Option<String>,
        subject: String,
        files: Vec<String>,
    }

    let command = input
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");
    if command.is_empty() {
        return None;
    }

    let lines: Vec<&str> = input.lines().collect();
    let mut patches: Vec<Patch> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() {
            i += 1;
            continue;
        }

        if line.starts_with("patch ") || line.starts_with("patch\t") {
            let rest = line[5..].trim();
            let mut patch = Patch::default();
            if rest.starts_with("20") && rest.len() >= 19 {
                patch.date = Some(rest[..19].to_string());
            } else {
                patch.hash = Some(rest.split_whitespace().next().unwrap_or(rest).to_string());
            }
            let mut j = i + 1;
            while j < lines.len() {
                let t = lines[j].trim();
                if t.is_empty() {
                    j += 1;
                    continue;
                }
                if t.starts_with("patch ") || t.starts_with("patch\t") {
                    break;
                }
                if t.to_ascii_lowercase().starts_with("author:") {
                    patch.author = Some(
                        t.split_once(':')
                            .map(|(_, v)| v.trim())
                            .unwrap_or(t)
                            .to_string(),
                    );
                    j += 1;
                    continue;
                }
                if t.to_ascii_lowercase().starts_with("date:")
                    || t.to_ascii_lowercase().starts_with("timestamp:")
                {
                    patch.date = Some(
                        t.split_once(':')
                            .map(|(_, v)| v.trim())
                            .unwrap_or(t)
                            .to_string(),
                    );
                    j += 1;
                    continue;
                }
                if t.starts_with("* ") || t.starts_with("  * ") {
                    let subj = t.trim_start_matches("  ").trim_start_matches('*').trim();
                    if !patch.subject.is_empty() {
                        patch.subject.push(' ');
                    }
                    patch.subject.push_str(subj);
                    j += 1;
                    continue;
                }
                if t.starts_with("M ")
                    || t.starts_with("A ")
                    || t.starts_with("R ")
                    || t.starts_with("D ")
                {
                    patch.files.push(t.to_string());
                    j += 1;
                    continue;
                }
                if t.starts_with("  ") && (t.contains("./") || t.contains('/')) {
                    patch.files.push(t.trim().to_string());
                    j += 1;
                    continue;
                }
                // fallback: 非匹配行当作 subject 的一部分
                if t.len() > 0
                    && !t.starts_with("Author:")
                    && !t.starts_with("Date:")
                    && !t.starts_with("patch ")
                {
                    if !patch.subject.is_empty() {
                        patch.subject.push(' ');
                    }
                    patch.subject.push_str(t);
                    j += 1;
                    continue;
                }
                j += 1;
            }
            patches.push(patch);
            i = j;
            continue;
        }

        if line.to_ascii_lowercase().starts_with("author:") {
            let mut patch = Patch::default();
            patch.author = Some(
                line.split_once(':')
                    .map(|(_, v)| v.trim())
                    .unwrap_or(line)
                    .to_string(),
            );
            let mut j = i + 1;
            while j < lines.len() {
                let t = lines[j].trim();
                if t.is_empty() {
                    j += 1;
                    continue;
                }
                if t.starts_with("Author:") || t.starts_with("patch ") {
                    break;
                }
                if t.to_ascii_lowercase().starts_with("date:") {
                    patch.date = Some(
                        t.split_once(':')
                            .map(|(_, v)| v.trim())
                            .unwrap_or(t)
                            .to_string(),
                    );
                    j += 1;
                    continue;
                }
                if !t.starts_with("M ")
                    && !t.starts_with("A ")
                    && !t.starts_with("R ")
                    && !t.starts_with("D ")
                {
                    if !patch.subject.is_empty() {
                        patch.subject.push(' ');
                    }
                    patch.subject.push_str(t);
                } else {
                    patch.files.push(t.to_string());
                }
                j += 1;
            }
            if !patch.subject.is_empty() || patch.author.is_some() {
                patches.push(patch);
            }
            i = j;
            continue;
        }

        i += 1;
    }

    if patches.is_empty() {
        return None;
    }

    let mut out: Vec<String> = vec![command.to_string()];
    for p in patches {
        let mut parts: Vec<String> = Vec::new();
        if let Some(d) = p.date {
            parts.push(format!("CR:{}", d));
        }
        if let Some(a) = p.author {
            parts.push(format!("OW:@{}", a));
        }
        if let Some(h) = p.hash {
            parts.push(format!("@{}", h));
        }
        if !p.subject.is_empty() {
            parts.push(format!("CM:{}", p.subject));
        }
        out.push(parts.join(" "));
        for f in p.files {
            out.push(format!("  {}", f));
        }
    }
    Some(out.join("\n"))
}

// ============================================================================
// Case 42/322: status — 保留锚点，保留文件状态码 A/M/R
// ============================================================================
fn compact_darcs_status_cmd(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if first && (t.starts_with("darcs status") || t.starts_with("darcs whatsnew")) {
            out.push(t.to_string());
            first = false;
            continue;
        }

        // 跳过重复命令
        if t.starts_with("darcs ") {
            continue;
        }

        // 文件状态行: "M src/file.rs" 或 " M src/file.rs"
        let trimmed = t.trim();
        if let Some(rest) = trimmed
            .strip_prefix("M ")
            .or_else(|| trimmed.strip_prefix("A "))
            .or_else(|| trimmed.strip_prefix("R "))
            .or_else(|| trimmed.strip_prefix("D "))
        {
            let status = trimmed.chars().next().unwrap_or(' ');
            out.push(format!("{}:{}", status, rest.trim()));
            continue;
        }

        out.push(trimmed.to_string());
    }
    out.join("\n")
}

// ============================================================================
// Case 210: obliterate — 保留锚点，抹除交互对话
// ============================================================================
fn compact_darcs_obliterate_cmd(raw: &str) -> String {
    let mut out = Vec::new();

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if t.starts_with("darcs obliterate") {
            out.push(t.to_string());
            continue;
        }
        if is_darcs_noise(t) {
            continue;
        }

        // 补丁行: "  abc1234: Add feature A"
        if t.contains(": ")
            && t.split(':').next().map_or(false, |p| {
                p.len() <= 12 && p.chars().all(|c| c.is_alphanumeric())
            })
        {
            out.push(format!("D:{}", t.trim()));
            continue;
        }

        out.push(t.to_string());
    }
    out.join("\n")
}

// ============================================================================
// Case 154: amend — 保留锚点，使用 -> 表示流向
// ============================================================================
fn compact_darcs_amend_cmd(raw: &str) -> String {
    let mut out = Vec::new();
    let mut old_msg: Option<String> = None;
    let mut new_msg: Option<String> = None;

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if t.starts_with("darcs amend") {
            out.push(t.to_string());
            continue;
        }
        if is_darcs_noise(t) {
            continue;
        }

        if let Some(rest) = t.strip_prefix("Old message:") {
            old_msg = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = t.strip_prefix("New message:") {
            new_msg = Some(rest.trim().to_string());
            continue;
        }

        out.push(t.to_string());
    }

    if let (Some(old), Some(new)) = (old_msg, new_msg) {
        out.push(format!("AMEND:{}->{}", old, new));
    }
    out.join("\n")
}

// ============================================================================
// Case 282: rebase — 保留锚点，使用 -> 表示流向
// ============================================================================
fn compact_darcs_rebase_cmd(raw: &str) -> String {
    let mut out = Vec::new();
    let mut from: Option<String> = None;
    let mut to: Option<String> = None;

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if t.starts_with("darcs rebase") {
            out.push(t.to_string());
            continue;
        }

        if let Some(rest) = t.strip_prefix("Rebasing from:") {
            from = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = t.strip_prefix("Rebasing to:") {
            to = Some(rest.trim().to_string());
            continue;
        }

        if is_darcs_noise(t) {
            continue;
        }

        out.push(t.to_string());
    }

    if let (Some(f), Some(t)) = (from, to) {
        out.push(format!("REBASE:{}->{}", f, t));
    }
    out.join("\n")
}

// ============================================================================
// Case 105: record — 保留锚点，抹除记录噪音
// ============================================================================
fn compact_darcs_record_cmd(raw: &str) -> String {
    let mut out = Vec::new();

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if t.starts_with("darcs record") {
            out.push(t.to_string());
            continue;
        }
        if is_darcs_noise(t) {
            continue;
        }

        out.push(t.to_string());
    }
    out.join("\n")
}

// ============================================================================
// 通用 fallback
// ============================================================================
fn compact_darcs_generic(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("darcs ") && first {
            out.push(t.to_string());
            first = false;
            continue;
        }
        if is_darcs_noise(t) {
            continue;
        }
        if let Some(alert) = map_darcs_alert(t) {
            out.push(alert);
            continue;
        }
        out.push(t.to_string());
    }
    out.join("\n")
}

// ============================================================================
// 辅助函数
// ============================================================================
fn cost_gate(raw: &str, output: String) -> String {
    if output.len() < raw.len() {
        output
    } else {
        raw.to_string()
    }
}
pub(super) fn is_darcs_noise(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.contains("about to delete")
        || l.contains("really delete")
        || l.contains("(yes/no)")
        || l == "yes"
        || l.starts_with("recording patch")
        || l.starts_with("recording changes")
        || l.contains("patch applied")
        || l.contains("rebase in progress")
        || l.contains("rebasing in progress")
        || l.starts_with("amending patch")
        || l == "no patches selected."
        || l == "no changes!"
        || l.starts_with("deleted ")
}
pub(super) fn map_darcs_alert(line: &str) -> Option<String> {
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
