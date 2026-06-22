#![allow(dead_code)]
//! GitLab (glab) 压缩方法 — Compression Protocol V1
use super::parser::*;
use crate::core::plugin_config_loader::parse_vcs_command_words_from_line;
use crate::core::utils::roi::prefer_non_expanding;

// ============================================================================
// 遗留 parser 集成（保留兼容性）
// ============================================================================
#[tracing::instrument(level = "debug", skip_all)]
pub fn process_parser(parser: &dyn VcsParser, raw: &str) -> String {
    match parser.parse(raw) {
        Some(doc) => render_glab_doc(&doc),
        None => raw.to_string(),
    }
}

fn render_glab_doc(doc: &VcsDocument) -> String {
    let mut lines = Vec::new();
    for rec in &doc.records {
        lines.push(render_glab_record(rec));
    }
    lines.join("\n")
}

fn render_glab_record(rec: &VcsRecord) -> String {
    match rec {
        VcsRecord::Section(s) => format!("[{}]", s),
        VcsRecord::Commit(c) => format!("!{}", c),
        VcsRecord::Subject(s) => s.clone(),
        VcsRecord::Author(a) => format!("@{}", a),
        VcsRecord::Date(d) => d.clone(),
        VcsRecord::Stat(s) => s.clone(),
        VcsRecord::Raw(r) => r.clone(),
        VcsRecord::File { status, path } => {
            if let Some(st) = status {
                format!("{} {}", st, path)
            } else {
                path.clone()
            }
        }
        VcsRecord::LabeledFile { label, path } => format!("[{}] {}", label, path),
        _ => rec.to_string(),
    }
}

// ============================================================================
// 公开 API
// ============================================================================
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_glab_log_for_ai(raw: &str) -> String {
    prefer_non_expanding(raw, compact_glab_dispatch(raw))
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_glab_other_for_ai(raw: &str) -> String {
    prefer_non_expanding(raw, compact_glab_dispatch(raw))
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn is_glab_log_block(_: &str) -> bool {
    true
}

// ============================================================================
// 调度器
// ============================================================================
fn compact_glab_dispatch(raw: &str) -> String {
    if raw.len() < 50 {
        return raw.to_string();
    }

    let first_line = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();

    if let Some((tool, words)) = parse_vcs_command_words_from_line(first_line) {
        if tool == "glab" {
            match (
                words.first().map(String::as_str),
                words.get(1).map(String::as_str),
            ) {
                (Some("mr"), Some("list")) => return compact_glab_mr_list(raw),
                (Some("mr"), Some("view")) => return compact_glab_mr_view(raw),
                (Some("mr"), Some("create")) => return compact_glab_mr_create(raw),
                (Some("issue"), Some("list")) => return compact_glab_issue_list(raw),
                (Some("issue"), Some("view")) => return compact_glab_issue_view(raw),
                (Some("issue"), Some("create")) => return compact_glab_issue_create(raw),
                _ => {}
            }
        }
    }

    compact_glab_generic(raw)
}

// ============================================================================
// Case 95: mr list — 保留锚点，列解析，符号化输出
// ============================================================================
fn compact_glab_mr_list(raw: &str) -> String {
    let mut out = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("glab mr list") {
            out.push(trimmed.to_string());
            continue;
        }

        if is_glab_separator(trimmed) || is_glab_table_header(trimmed) {
            continue;
        }

        if let Some(row) = parse_glab_mr_row(trimmed) {
            out.push(row);
            continue;
        }

        out.push(trimmed.to_string());
    }

    out.join("\n")
}

/// MR 行: !123   Add user authentication flow   [open]   alice   2026-04-01
fn parse_glab_mr_row(line: &str) -> Option<String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 3 || !tokens[0].starts_with('!') {
        return None;
    }

    let id = &tokens[0][1..]; // strip !
    let date = tokens
        .last()
        .filter(|t| t.len() == 10 && t.as_bytes()[4] == b'-');
    let author_idx = if date.is_some() {
        tokens.len() - 2
    } else {
        tokens.len() - 1
    };

    let state = tokens
        .iter()
        .find(|t| t.starts_with('[') && t.ends_with(']'))
        .map(|s| &s[1..s.len() - 1]);

    let author = tokens[author_idx];

    let title_end = {
        let mut end = author_idx;
        for i in (1..end).rev() {
            if tokens[i].starts_with('[') && tokens[i].ends_with(']') {
                continue;
            }
            end = i + 1;
            break;
        }
        end
    };
    let title = tokens[1..title_end]
        .iter()
        .filter(|t| !t.starts_with('['))
        .copied()
        .collect::<Vec<_>>()
        .join(" ");

    let mut result = format!(
        "!{} ST:{} OW:@{} {}",
        id,
        state.unwrap_or("?"),
        author,
        title
    );
    if let Some(d) = date {
        result.push_str(&format!(" {}", d));
    }
    Some(result)
}

// ============================================================================
// Case 110: issue list — 保留锚点，表头清除，列解析
// ============================================================================
fn compact_glab_issue_list(raw: &str) -> String {
    let mut out = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("glab issue list") {
            out.push(trimmed.to_string());
            continue;
        }

        if is_glab_separator(trimmed) || is_glab_table_header(trimmed) {
            continue;
        }

        if let Some(row) = parse_glab_issue_row(trimmed) {
            out.push(row);
            continue;
        }

        out.push(trimmed.to_string());
    }

    out.join("\n")
}

/// Issue 行: 1   Fix login bug   bug   alice   alice   Open
fn parse_glab_issue_row(line: &str) -> Option<String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 4 {
        return None;
    }
    if tokens[0].parse::<u32>().is_err() {
        return None;
    }

    let id = tokens[0];
    let status = tokens.last()?;
    let author = tokens[tokens.len() - 2];
    let assignee = tokens[tokens.len() - 3];

    let label = if tokens[tokens.len() - 4] == "-" {
        None
    } else {
        Some(tokens[tokens.len() - 4])
    };

    let title = tokens[1..tokens.len() - 4].join(" ");

    let mut result = format!("#{} ST:{} OW:@{} {}", id, status, author, title);
    if let Some(lb) = label {
        result.push_str(&format!(" LB:{}", lb));
    }
    if assignee != author && assignee != "-" {
        result.push_str(&format!(" AS:@{}", assignee));
    }
    Some(result)
}

// ============================================================================
// Case 108: mr view — 保留锚点，K-V 扁平化，DESC 单独行
// ============================================================================
fn compact_glab_mr_view(raw: &str) -> String {
    let mut out = Vec::new();
    let mut meta: Vec<String> = Vec::new();
    let mut desc: Vec<String> = Vec::new();
    let mut in_desc = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("glab mr view") {
            out.push(trimmed.to_string());
            continue;
        }

        // 跳过 MR 标题行和分隔线
        if trimmed.starts_with('!') && trimmed.len() > 2 && !trimmed.contains(':') {
            // Extract MR ID for the meta
            if let Some(id) = parse_glab_mr_id(trimmed) {
                meta.push(id);
            }
            continue;
        }
        if is_glab_separator(trimmed) {
            continue;
        }

        // Description 段
        if trimmed == "Description:" {
            in_desc = true;
            continue;
        }
        if in_desc {
            // 停止在 Changes / Steps to reproduce 行
            if is_glab_desc_boundary(trimmed) {
                in_desc = false;
                continue;
            }
            desc.push(trimmed.to_string());
            continue;
        }

        // 跳过 Changes / Steps to reproduce 及之后
        if is_glab_desc_boundary(trimmed) {
            continue;
        }

        if line_has_preserved_keyword(trimmed) {
            desc.push(trimmed.to_string());
            continue;
        }

        // 跳过噪音行
        if is_glab_view_noise(trimmed) {
            continue;
        }

        // K-V 压缩
        if let Some(kv) = compact_glab_view_kv(trimmed) {
            meta.push(kv);
            continue;
        }
    }

    if !meta.is_empty() {
        out.push(meta.join(" "));
    }
    if !desc.is_empty() {
        out.push(format!("DESC: {}", desc.join(" ")));
    }

    out.join("\n")
}

fn parse_glab_mr_id(line: &str) -> Option<String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.is_empty() || !tokens[0].starts_with('!') {
        return None;
    }
    let id = &tokens[0][1..];
    // Verify numeric
    if id.parse::<u32>().is_ok() {
        Some(format!("!{}", id))
    } else {
        None
    }
}

fn compact_glab_view_kv(line: &str) -> Option<String> {
    let colon = line.find(':')?;
    let key = line[..colon].trim().to_ascii_lowercase();
    let val = line[colon + 1..].trim();

    if val.is_empty() {
        return None;
    }

    match key.as_str() {
        "status" => Some(format!("ST:{}", val)),
        "author" => {
            // "alice <alice@example.com>" → "alice"
            let name = val.split('<').next().unwrap_or(val).trim();
            Some(format!("OW:@{}", name))
        }
        "reviewers" => {
            // "charlie (1)" → "charlie"
            let cleaned = val
                .split(',')
                .map(|r| r.split('(').next().unwrap_or("").trim())
                .collect::<Vec<_>>()
                .join(",");
            Some(format!("RV:{}", cleaned))
        }
        "source" => {
            let cleaned = val.replace(" -> ", "->");
            Some(format!("BR:{}", cleaned))
        }
        "web url" => Some(format!("URL:{}", abbreviate_glab_url(val))),
        "assignee" => Some(format!("AS:@{}", val)),
        _ => None,
    }
}

// ============================================================================
// Case 111: issue view — 保留锚点，K-V 扁平化
// ============================================================================
fn compact_glab_issue_view(raw: &str) -> String {
    let mut out = Vec::new();
    let mut meta: Vec<String> = Vec::new();
    let mut desc: Vec<String> = Vec::new();
    let mut in_desc = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("glab issue view") {
            out.push(trimmed.to_string());
            continue;
        }

        if is_glab_separator(trimmed) {
            continue;
        }

        if trimmed == "Description:" {
            in_desc = true;
            continue;
        }
        if in_desc {
            if is_glab_desc_boundary(trimmed) {
                in_desc = false;
                continue;
            }
            desc.push(trimmed.to_string());
            continue;
        }

        if line_has_preserved_keyword(trimmed) {
            desc.push(trimmed.to_string());
            continue;
        }

        if is_glab_desc_boundary(trimmed) || is_glab_view_noise(trimmed) {
            continue;
        }

        if let Some(kv) = compact_glab_view_kv(trimmed) {
            meta.push(kv);
            continue;
        }

        // 非 K-V 行可能是标题行（以 ! 开头）
        if trimmed.starts_with('!') {
            if let Some(id) = parse_glab_mr_id(trimmed) {
                meta.push(id);
            }
        }
    }

    if !meta.is_empty() {
        out.push(meta.join(" "));
    }
    if !desc.is_empty() {
        out.push(format!("DESC: {}", desc.join(" ")));
    }

    out.join("\n")
}

// ============================================================================
// Case 109/159: mr create — 保留锚点，去除 ✓，A: 映射
// ============================================================================
fn compact_glab_mr_create(raw: &str) -> String {
    let mut out = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("glab mr create") {
            out.push(trimmed.to_string());
            continue;
        }

        // 去除噪声行
        let cleaned = trimmed.trim_start_matches('✓').trim();

        if cleaned.starts_with("Creating merge request") || cleaned.starts_with("URL:") {
            continue;
        }

        if cleaned.starts_with("http") {
            continue;
        }

        // "Merge request created: !456" / "Created merge request !200"
        if let Some(rest) = cleaned
            .strip_prefix("Merge request created:")
            .or_else(|| cleaned.strip_prefix("Created merge request"))
        {
            out.push(format!("A:{}", rest.trim()));
            continue;
        }

        out.push(cleaned.to_string());
    }

    out.join("\n")
}

// ============================================================================
// Case 206: issue create — 保留锚点，去除 ✓，A: 映射
// ============================================================================
fn compact_glab_issue_create(raw: &str) -> String {
    let mut out = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("glab issue create") {
            out.push(trimmed.to_string());
            continue;
        }

        let cleaned = trimmed.trim_start_matches('✓').trim();

        if cleaned.starts_with("URL:") || cleaned.starts_with("http") {
            continue;
        }

        // "Created issue !50"
        if let Some(rest) = cleaned
            .strip_prefix("Created issue")
            .or_else(|| cleaned.strip_prefix("Created issue:"))
        {
            out.push(format!("A:{}", rest.trim()));
            continue;
        }

        out.push(cleaned.to_string());
    }

    out.join("\n")
}

// ============================================================================
// 通用噪音过滤与 fallback
// ============================================================================
fn compact_glab_generic(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("glab ") && first {
            out.push(trimmed.to_string());
            first = false;
            continue;
        }

        let cleaned = trimmed.trim_start_matches('✓').trim();

        if is_glab_separator(cleaned) || is_glab_table_header(cleaned) {
            continue;
        }

        if is_glab_noise(cleaned) || is_glab_view_noise(cleaned) {
            continue;
        }

        if cleaned.starts_with("URL:") || cleaned.starts_with("http") {
            continue;
        }

        if let Some(alert) = map_glab_alert(cleaned) {
            out.push(alert);
            continue;
        }

        out.push(cleaned.to_string());
    }

    out.join("\n")
}

// ============================================================================
// 辅助函数
// ============================================================================
fn is_glab_separator(line: &str) -> bool {
    line.len() >= 10 && line.chars().all(|c| c == '-' || c == '=')
}

fn is_glab_table_header(line: &str) -> bool {
    let header_words: Vec<&str> = line
        .split_whitespace()
        .filter(|w| {
            w.len() >= 2
                && w.chars()
                    .next()
                    .map(|c| c.is_ascii_uppercase())
                    .unwrap_or(false)
        })
        .collect();
    header_words.len() >= 3
}

fn is_glab_view_noise(line: &str) -> bool {
    if line_has_preserved_keyword(line) {
        return false;
    }
    let lower = line.to_ascii_lowercase();
    lower.starts_with("created:")
        || lower.starts_with("updated:")
        || lower.starts_with("milestone:")
        || lower.starts_with("due:")
        || lower.starts_with("labels:")
        || lower.starts_with("participants:")
        || lower.starts_with("comments:")
}

fn is_glab_noise(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("creating merge request")
        || lower.starts_with("merge request created")
        || lower.starts_with("created merge request")
        || lower.starts_with("created issue")
        || lower.starts_with("url:")
}

fn is_glab_desc_boundary(line: &str) -> bool {
    if line_has_preserved_keyword(line) {
        return false;
    }
    let lower = line.to_ascii_lowercase();
    lower == "changes:"
        || lower == "steps to reproduce:"
        || lower.starts_with("expected:")
        || lower.starts_with("actual:")
}

fn line_has_preserved_keyword(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.contains("error")
        || l.contains("fatal")
        || l.contains("panic")
        || l.contains("exception")
        || l.contains("uncaught")
}

fn abbreviate_glab_url(url: &str) -> String {
    url.replace("https://gitlab.com/", "gl:")
        .replace("https://github.com/", "gh:")
        .replace("https://dev.azure.com/", "az:")
        .replace("https://bitbucket.org/", "bb:")
}

pub(super) fn is_glab_noise_line(line: &str) -> bool {
    is_glab_noise(line) || is_glab_view_noise(line)
}

pub(super) fn map_glab_alert(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let triggers = ["conflict", "error:", "failed", "rejected"];
    if triggers.iter().any(|t| lower.contains(t)) {
        let cleaned = line.trim_start();
        if cleaned.starts_with('!') {
            Some(cleaned.to_string())
        } else {
            Some(format!("!{}", cleaned))
        }
    } else {
        None
    }
}
