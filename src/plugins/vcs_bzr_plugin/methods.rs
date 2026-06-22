#![allow(dead_code)]
//! Bzr 压缩方法 — Compression Protocol V1
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
        let trimmed = out.trim();
        if trimmed.is_empty() {
            // 法则 E: 空状态防歧义
            let anchor = raw
                .lines()
                .find(|l| !l.trim().is_empty())
                .map(|l| l.trim())
                .unwrap_or("");
            format!("{}\nST:[CLEAN]", anchor)
        } else {
            trimmed.to_string()
        }
    } else {
        raw.to_string()
    }
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_bzr_status_block(text: &str) -> bool {
    bzr_subcommand_is(text, &["status", "st", "resolve"])
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_bzr_log_block(text: &str) -> bool {
    bzr_subcommand_is(text, &["log", "pull", "push", "merge", "branch", "commit"])
}

// ============================================================================
// 公开 API — 全部 anchor_guard 包裹
// ============================================================================
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_bzr_status_for_ai(raw: &str) -> String {
    prefer_non_expanding(raw, anchor_guard(raw, || compact_bzr_dispatch(raw)))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_bzr_diff_for_ai(raw: &str) -> String {
    prefer_non_expanding(
        raw,
        anchor_guard(raw, || process_parser(&BzrDiffParser, raw)),
    )
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_bzr_log_for_ai(raw: &str) -> String {
    prefer_non_expanding(raw, anchor_guard(raw, || compact_bzr_dispatch(raw)))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_bzr_other_for_ai(raw: &str) -> String {
    prefer_non_expanding(raw, compact_bzr_dispatch(raw))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_bzr_log_family_for_ai(raw: &str) -> String {
    compact_bzr_log_for_ai(raw)
}

fn anchor_guard(raw: &str, f: impl FnOnce() -> String) -> String {
    let anchor = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");
    let body = f();
    if body.starts_with(anchor) {
        // 法则 E: 空状态 - 若压缩后仅剩锚点，追加 ST:[CLEAN]
        let after_anchor = &body[anchor.len()..];
        if after_anchor.trim().is_empty() && should_append_bzr_clean_marker(raw) {
            return format!("{}\nST:[CLEAN]", anchor);
        }
        return body;
    }
    if body.is_empty() || body == raw {
        return body;
    }
    format!("{}\n{}", anchor, body.trim_start())
}

/// 仅对状态类命令追加空状态标记，避免 bzr help 等普通命令被污染。
fn should_append_bzr_clean_marker(raw: &str) -> bool {
    let first = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");
    let Some((tool, words)) = parse_vcs_command_words_from_line(first) else {
        return false;
    };
    if tool != "bzr" {
        return false;
    }
    matches!(
        words.first().map(String::as_str),
        Some("status" | "st" | "resolve")
    )
}

// ============================================================================
// 调度器
// ============================================================================
fn compact_bzr_dispatch(raw: &str) -> String {
    if raw.len() < 30 {
        return raw.to_string();
    }
    let first = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();

    if let Some((tool, words)) = parse_vcs_command_words_from_line(first) {
        if tool == "bzr" {
            match words.first().map(String::as_str) {
                Some("status") | Some("resolve") => return compact_bzr_status_cmd(raw),
                Some("log") => return compact_bzr_log_cmd(raw),
                Some("push") => return compact_bzr_push(raw),
                Some("merge") => return compact_bzr_merge(raw),
                Some("revert") => return compact_bzr_revert(raw),
                Some("commit") => return compact_bzr_commit(raw),
                Some("pull") => return compact_bzr_pull(raw),
                _ => {}
            }
        }
    }

    compact_bzr_generic(raw)
}

/// 使用统一 argv 解析识别 bzr 子命令，避免正文关键字污染路由判断。
fn bzr_subcommand_is(raw: &str, expected: &[&str]) -> bool {
    let first = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");
    let Some((tool, words)) = parse_vcs_command_words_from_line(first) else {
        return false;
    };
    if tool != "bzr" {
        return false;
    }
    let Some(sub) = words.first().map(String::as_str) else {
        return false;
    };
    expected.iter().any(|cmd| *cmd == sub)
}

// ============================================================================
// Case 38/319: status — 保留锚点，状态码映射 M:/A:/D:/?:
// ============================================================================
fn compact_bzr_status_cmd(raw: &str) -> String {
    let mut out = Vec::new();
    let mut got_anchor = false;

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if !got_anchor && t.starts_with("bzr ") {
            out.push(t.to_string());
            got_anchor = true;
            continue;
        }
        if t.starts_with("bzr ") {
            continue;
        }

        // 噪音先处理再映射
        if is_bzr_noise(t) {
            continue;
        }

        // 长词状态: "modified: src/file.rs" or "modified src/file.rs"
        if let Some(mapped) = map_bzr_long_status(t) {
            out.push(mapped);
            continue;
        }

        // 短格式状态: " M  src/file.rs" or " M src/file.rs"
        if let Some(mapped) = map_bzr_short_status(t) {
            out.push(mapped);
            continue;
        }

        out.push(t.to_string());
    }
    out.join("\n")
}

fn map_bzr_long_status(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    // 带冒号的长词状态: "modified: src/file.rs" → "M:src/file.rs"
    let (code, rest) = if lower.starts_with("modified:") {
        ('M', &line[9..])
    } else if lower.starts_with("added:") {
        ('A', &line[6..])
    } else if lower.starts_with("removed:") {
        ('D', &line[8..])
    } else if lower.starts_with("deleted:") {
        ('D', &line[8..])
    } else if lower.starts_with("unknown:") {
        ('?', &line[8..])
    } else if lower.starts_with("renamed:") {
        ('R', &line[8..])
    } else if lower.starts_with("reverted:") {
        return Some(format!("ST:R {}", &line[9..].trim()));
    } else if lower.starts_with("reverted ") {
        return Some(format!("ST:R {}", &line[9..].trim()));
    // 不带冒号的状态（如 bzr commit 输出）: "modified src/file.rs"
    } else if lower.starts_with("modified ") {
        ('M', &line[9..])
    } else if lower.starts_with("added ") {
        ('A', &line[6..])
    } else if lower.starts_with("removed ") {
        ('D', &line[8..])
    } else if lower.starts_with("deleted ") {
        ('D', &line[8..])
    } else if lower.starts_with("renamed ") {
        ('R', &line[8..])
    } else {
        return None;
    };
    Some(format!("ST:{} {}", code, rest.trim()))
}

fn map_bzr_short_status(line: &str) -> Option<String> {
    let t = line.trim();
    if t.len() < 3 {
        return None;
    }
    let first = t.chars().next()?;
    let code = match first {
        'M' => 'M',
        'A' => 'A',
        'D' => 'D',
        'R' => 'R',
        '?' => '?',
        '+' => 'A',
        _ => return None,
    };
    // 状态码后必须紧跟空格或大写字母（避免 "Resolved" → "R:esolved" 误匹配）
    let second = t.as_bytes().get(1).copied().unwrap_or(0);
    if second != b' ' && !second.is_ascii_uppercase() {
        return None;
    }
    // 跳过状态码和可能的第二个状态字母
    let rest = t[1..]
        .trim_start_matches(|c: char| c.is_ascii_uppercase() || c == ' ')
        .trim();
    if rest.is_empty() {
        return None;
    }
    Some(format!("ST:{} {}", code, rest))
}

// ============================================================================
// Case 39/318: log — 保留锚点，CM: 保留，CP V1 符号化
// ============================================================================
fn compact_bzr_log_cmd(raw: &str) -> String {
    let mut out = Vec::new();
    let mut got_anchor = false;
    if let Some(structured) = compact_bzr_log_structured(raw) {
        for line in structured.lines() {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            if !got_anchor && (t.starts_with("bzr log") || t.starts_with("bzr ")) {
                out.push(t.to_string());
                got_anchor = true;
                continue;
            }
            out.push(t.to_string());
        }
        return out.join("\n");
    }
    // fallback
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if !got_anchor && t.starts_with("bzr ") {
            out.push(t.to_string());
            got_anchor = true;
            continue;
        }
        if t.chars().all(|c| c == '-') && t.len() >= 3 {
            continue;
        }
        if is_bzr_noise(t) {
            continue;
        }
        out.push(t.to_string());
    }
    out.join("\n")
}

fn compact_bzr_log_structured(input: &str) -> Option<String> {
    #[derive(Default)]
    struct Commit {
        rev: String,
        author: Option<String>,
        timestamp: Option<String>,
        subject: String,
    }

    let lines: Vec<&str> = input.lines().collect();
    if lines.is_empty() {
        return None;
    }
    let command = lines
        .iter()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");
    let mut commits: Vec<Commit> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() || (line.chars().all(|c| c == '-') && line.len() >= 3) {
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("revno:") {
            let rev = rest.trim().to_string();
            let mut commit = Commit {
                rev,
                ..Default::default()
            };
            let mut j = i + 1;
            while j < lines.len() {
                let t = lines[j].trim();
                if t.is_empty()
                    || t.starts_with("revno:")
                    || (t.chars().all(|c| c == '-') && t.len() >= 3)
                {
                    j += 1;
                    break;
                }
                if let Some(v) = t.strip_prefix("committer:") {
                    commit.author = Some(v.trim().to_string());
                    j += 1;
                    continue;
                }
                if let Some(v) = t.strip_prefix("timestamp:") {
                    commit.timestamp = Some(v.trim().to_string());
                    j += 1;
                    continue;
                }
                if t.starts_with("branch nick:") || t.starts_with("revision-id:") {
                    j += 1;
                    continue;
                }
                if t.eq_ignore_ascii_case("message:") {
                    j += 1;
                    continue;
                }
                // 缩进行（原行的前导空格）是 message 的一部分
                let raw_line = lines[j];
                if raw_line.starts_with("  ")
                    || raw_line.starts_with('\t')
                    || raw_line.starts_with("    ")
                {
                    if !commit.subject.is_empty() {
                        commit.subject.push(' ');
                    }
                    commit.subject.push_str(t);
                    j += 1;
                    continue;
                }
                // 非缩进行且非已知字段 → 也可能是 message
                if !t.contains(':') && !commit.subject.is_empty() {
                    commit.subject.push(' ');
                    commit.subject.push_str(t);
                    j += 1;
                    continue;
                }
                j += 1;
            }
            commits.push(commit);
            i = j;
            continue;
        }
        i += 1;
    }

    if commits.is_empty() {
        return None;
    }
    let mut out: Vec<String> = vec![command.to_string()];
    for c in commits {
        let mut parts = vec![format!("r{}", c.rev)];
        if let Some(ts) = c.timestamp {
            parts.push(ts);
        }
        if let Some(au) = c.author {
            let name = au.split('<').next().unwrap_or(&au).trim();
            parts.push(format!("OW:@{}", name));
        }
        if !c.subject.is_empty() {
            parts.push(format!("CM:{}", c.subject));
        }
        out.push(parts.join(" "));
    }
    Some(out.join("\n"))
}

// ============================================================================
// Case 148: push — 保留锚点，抹除帮助废话
// ============================================================================
fn compact_bzr_push(raw: &str) -> String {
    let mut out = Vec::new();
    let mut got_anchor = false;
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if !got_anchor && t.starts_with("bzr push") {
            out.push(t.to_string());
            got_anchor = true;
            continue;
        }
        if is_bzr_noise(t) {
            continue;
        }
        out.push(t.to_string());
    }
    out.join("\n")
}

// ============================================================================
// Case 149: merge — 保留锚点，抹除 "All changes applied"
// ============================================================================
fn compact_bzr_merge(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("bzr merge") {
            out.push(t.to_string());
            continue;
        }
        if is_bzr_noise(t) {
            continue;
        }
        out.push(t.to_string());
    }
    out.join("\n")
}

// ============================================================================
// Case 192: revert — 保留锚点，REVERT: 映射
// ============================================================================
fn compact_bzr_revert(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("bzr revert") {
            out.push(t.to_string());
            continue;
        }
        if let Some(rest) = t.strip_prefix("reverted ") {
            out.push(format!("ST:R {}", rest.trim()));
            continue;
        }
        if t.starts_with("reverted") {
            if let Some(idx) = t.find("reverted") {
                out.push(format!("ST:R {}", t[idx + 8..].trim()));
                continue;
            }
        }
        if is_bzr_noise(t) {
            continue;
        }
        out.push(t.to_string());
    }
    out.join("\n")
}

// ============================================================================
// Case 193: commit — 保留锚点，抹除 "Committing to" / "Committed revision"
// ============================================================================
fn compact_bzr_commit(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("bzr commit") {
            out.push(t.to_string());
            continue;
        }
        if is_bzr_noise(t) {
            continue;
        }
        if let Some(mapped) = map_bzr_long_status(t) {
            out.push(mapped);
            continue;
        }
        out.push(t.to_string());
    }
    out.join("\n")
}

// ============================================================================
// Case 103: pull — 保留锚点
// ============================================================================
fn compact_bzr_pull(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("bzr pull") {
            out.push(t.to_string());
            continue;
        }
        if is_bzr_noise(t) {
            continue;
        }
        out.push(t.to_string());
    }
    out.join("\n")
}

// ============================================================================
// 通用 fallback
// ============================================================================
fn compact_bzr_generic(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("bzr ") && first {
            out.push(t.to_string());
            first = false;
            continue;
        }
        if is_bzr_noise(t) {
            continue;
        }
        if let Some(mapped) = map_bzr_long_status(t) {
            out.push(mapped);
            continue;
        }
        if let Some(alert) = map_bzr_alert(t) {
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
fn is_bzr_noise(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.contains("committing to:")
        || l.starts_with("committed revision")
        || l.contains("all changes applied")
        || l.starts_with("to push to a branch")
        || l.starts_with("bzr push")
        || l.starts_with("pulled ")
        || l.contains("revisions in branch")
        || l.starts_with("branched ")
        || l.contains("resolved conflicts")
        || l.starts_with("these commits are missing")
        || l.starts_with("you are missing")
}

pub(super) fn map_bzr_alert(line: &str) -> Option<String> {
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
