#![allow(dead_code)]
//! Bitbucket 压缩方法 — Compression Protocol V1
use super::parser::*;
use crate::core::plugin_config_loader::parse_vcs_command_words_from_line;

// ============================================================================
// 遗留 parser 集成（保留兼容性）
// ============================================================================
#[tracing::instrument(level = "debug", skip_all)]
pub fn process_parser(parser: &dyn VcsParser, raw: &str) -> String {
    match parser.parse(raw) {
        Some(doc) => render_bb_doc(&doc),
        None => raw.to_string(),
    }
}

fn render_bb_doc(doc: &VcsDocument) -> String {
    let mut lines = Vec::new();
    for rec in &doc.records {
        lines.push(render_bb_record(rec));
    }
    lines.join("\n")
}

fn render_bb_record(rec: &VcsRecord) -> String {
    match rec {
        VcsRecord::Section(s) => format!("[{}]", s),
        VcsRecord::Commit(c) => format!("#{}", c),
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
pub fn compact_bitbucket_log_for_ai(raw: &str) -> String {
    compact_bb_dispatch(raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_bitbucket_other_for_ai(raw: &str) -> String {
    compact_bb_dispatch(raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn is_bitbucket_log_block(_: &str) -> bool {
    true
}

// ============================================================================
// 调度器
// ============================================================================
fn compact_bb_dispatch(raw: &str) -> String {
    let cleaned = crate::core::utils::strip_ansi(raw);

    // 健壮性：过短输入直接返回原始文本，避免反向压缩
    if cleaned.len() < 50 {
        return cleaned;
    }

    let first_line = cleaned
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();

    if let Some((tool, words)) = parse_vcs_command_words_from_line(first_line) {
        if tool == "bitbucket" {
            match (
                words.first().map(String::as_str),
                words.get(1).map(String::as_str),
            ) {
                (Some("pr"), Some("list")) => return compact_bb_pr_list(&cleaned),
                (Some("pr"), Some("view")) => return compact_bb_pr_view(&cleaned),
                (Some("pr"), Some("create")) => return compact_bb_pr_create(&cleaned),
                (Some("issue"), Some("list")) => return compact_bb_issue_list(&cleaned),
                _ => {}
            }
        }
    }

    // 通用噪声过滤
    compact_bb_generic(&cleaned)
}

// ============================================================================
// Case 113: pr list — 保留锚点，移除表头，行列压缩
// ============================================================================
fn compact_bb_pr_list(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // 保留命令锚点（仅第一行）
        if trimmed.starts_with("bitbucket pr list") && first {
            out.push(trimmed.to_string());
            first = false;
            continue;
        }

        // 跳过表头行（全大写缩写的标题行）
        if is_bb_table_header(trimmed) {
            continue;
        }

        // 跳过分隔线（全由 - 或 = 组成的长行）
        if is_bb_separator(trimmed) {
            continue;
        }

        // 数据行：解析列 → #<ID> ST:<STATE> OW:@<author> <title>
        if let Some(row) = parse_pr_row(trimmed) {
            out.push(row);
            continue;
        }

        out.push(trimmed.to_string());
    }

    out.join("\n")
}

/// 数据行解析：OPEN 123 Fix auth bug alice OPEN 2026-04-01
/// 结构: TYPE ID <title...> AUTHOR STATE DATE
fn parse_pr_row(line: &str) -> Option<String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 5 {
        return None;
    }

    let id = tokens[1];
    if id.parse::<u64>().is_err() {
        return None;
    }

    let known_states = ["OPEN", "MERGED", "DECLINED", "SUPERSEDED"];
    let state_idx = tokens.iter().enumerate().rev().find_map(|(idx, tok)| {
        let upper = tok.to_ascii_uppercase();
        if known_states.contains(&upper.as_str()) {
            Some(idx)
        } else {
            None
        }
    })?;

    if state_idx < 3 {
        return None;
    }

    let author = tokens[state_idx - 1];
    let state = tokens[state_idx];
    let title = tokens[2..state_idx - 1].join(" ");
    if title.is_empty() {
        return None;
    }

    Some(format!(
        "#{} ST:{} OW:@{} {}",
        id,
        state.to_uppercase(),
        author,
        title
    ))
}

// ============================================================================
// Case 114: pr view — 保留锚点，扁平化 K-V，DESC 单独行
// ============================================================================
fn compact_bb_pr_view(raw: &str) -> String {
    let mut out = Vec::new();
    let mut meta_parts: Vec<String> = Vec::new();
    let mut desc_lines: Vec<String> = Vec::new();
    let mut in_desc = false;

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() && !in_desc {
            continue;
        }

        // 保留命令锚点
        if trimmed.starts_with("bitbucket pr view") {
            out.push(trimmed.to_string());
            continue;
        }

        // 跳过冗余标题行: "Pull request #125: Refactor codebase"
        if trimmed.starts_with("Pull request #") {
            continue;
        }

        // 跳过分隔线
        if is_bb_separator(trimmed) {
            continue;
        }

        // 跳过噪音：Created / Updated / Participants / Comments
        if is_bb_view_noise(trimmed) {
            continue;
        }

        // Description 段落（可能跨行）
        if trimmed == "Description:" {
            in_desc = true;
            continue;
        }
        if in_desc {
            desc_lines.push(trimmed.to_string());
            continue;
        }

        // K-V 压缩
        if let Some(compact) = compact_bb_view_kv(trimmed) {
            meta_parts.push(compact);
            continue;
        }
    }

    // 元数据同行输出
    if !meta_parts.is_empty() {
        out.push(meta_parts.join(" "));
    }

    // Description 单独行
    if !desc_lines.is_empty() {
        let desc = desc_lines.join(" ");
        out.push(format!("DESC: {}", desc));
    }

    out.join("\n")
}

/// 压缩协议 V1: 扁平化 K-V
fn compact_bb_view_kv(line: &str) -> Option<String> {
    let (k, v) = line.split_once(':')?;
    let key = k.trim().to_ascii_lowercase();
    let val = v.trim();

    if val.is_empty() {
        return None;
    }

    match key.as_str() {
        "state" => Some(format!("ST:{}", val.to_uppercase())),
        "author" => Some(format!("OW:@{}", val)),
        "reviewers" => {
            // "bob (approved), charlie" → "bob,charlie"
            let cleaned = val
                .split(',')
                .map(|r| r.split('(').next().unwrap_or("").trim())
                .collect::<Vec<_>>()
                .join(",");
            Some(format!("RV:{}", cleaned))
        }
        "source" => {
            // "feature-refactor -> main" → "BR:feature-refactor->main"
            let cleaned = val.replace(" -> ", "->");
            Some(format!("BR:{}", cleaned))
        }
        _ => None,
    }
}

// ============================================================================
// Case 207: pr create — 保留锚点，URL 消除，SRC 映射
// ============================================================================
fn compact_bb_pr_create(raw: &str) -> String {
    let mut out = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }

        // 保留命令锚点
        if trimmed.starts_with("bitbucket pr create") {
            out.push(trimmed.to_string());
            continue;
        }

        // URL 消除
        if trimmed.starts_with("URL:") || trimmed.starts_with("http") {
            continue;
        }

        // Source 映射: "Source: feature-auth -> main" → "SRC:feature-auth->main"
        if let Some(rest) = trimmed.strip_prefix("Source:") {
            let cleaned = rest.trim().replace(" -> ", "->");
            out.push(format!("SRC:{}", cleaned));
            continue;
        }

        // 创建结果: "✓ Created PR #125" → "Created PR #125"
        if trimmed.contains("Created PR #") {
            let cleaned = trimmed.trim_start_matches('✓').trim();
            out.push(cleaned.to_string());
            continue;
        }
    }

    out.join("\n")
}

// ============================================================================
// Case 208: issue list — 保留锚点，移除表头，行列压缩
// ============================================================================
fn compact_bb_issue_list(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }

        // 保留命令锚点
        if trimmed.starts_with("bitbucket issue list") {
            if first {
                out.push(trimmed.to_string());
                first = false;
            }
            continue;
        }

        // 跳过表头行 / 分隔线
        if is_bb_table_header(trimmed) || is_bb_separator(trimmed) {
            continue;
        }

        // 数据行：解析列 → #<ID> ST:<STATUS> OW:@<assignee> <title> PRI:<priority>
        if let Some(row) = parse_issue_row(trimmed) {
            out.push(row);
            continue;
        }

        out.push(trimmed.to_string());
    }

    out.join("\n")
}

/// 数据行解析：1    Fix login bug    OPEN    alice    High
/// 结构: ID <title...> STATUS ASSIGNEE PRIORITY
fn parse_issue_row(line: &str) -> Option<String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 4 {
        return None;
    }

    // 第一列必须是数字 ID
    if tokens[0].parse::<u32>().is_err() {
        return None;
    }

    let id = tokens[0];
    let priority = tokens[tokens.len() - 1];
    let assignee = tokens[tokens.len() - 2];
    let status = tokens[tokens.len() - 3];
    let title = tokens[1..tokens.len() - 3].join(" ");

    // 验证 status 是合法值
    let upper_status = status.to_uppercase();
    if upper_status != "OPEN" && upper_status != "CLOSED" && upper_status != "RESOLVED" {
        return None;
    }

    Some(format!(
        "#{} ST:{} OW:@{} {} PRI:{}",
        id, upper_status, assignee, title, priority
    ))
}

// ============================================================================
// 通用噪音过滤与 fallback
// ============================================================================
fn compact_bb_generic(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }

        // 保留命令锚点
        if trimmed.starts_with("bitbucket ") && first {
            out.push(trimmed.to_string());
            first = false;
            continue;
        }

        // 跳过分隔线与表头
        if is_bb_separator(trimmed) || is_bb_table_header(trimmed) {
            continue;
        }

        // 跳过噪音
        if is_bb_noise(trimmed) {
            continue;
        }

        // URL 消除
        if trimmed.starts_with("URL:") || trimmed.starts_with("http") {
            continue;
        }

        // 压缩协议 V1: 异常状态映射为 ! 前缀
        if let Some(alert) = map_bb_alert(trimmed) {
            out.push(alert);
            continue;
        }

        out.push(trimmed.to_string());
    }

    out.join("\n")
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 表头检测：包含 3 个以上全大写缩写词（≥2 字符）
fn is_bb_table_header(line: &str) -> bool {
    let upper_words: Vec<&str> = line
        .split_whitespace()
        .filter(|w| w.len() >= 2 && w.chars().all(|c| c.is_ascii_uppercase()))
        .collect();
    upper_words.len() >= 3
}

/// 分隔线检测：全由 - 或 = 组成且长度 ≥ 10
fn is_bb_separator(line: &str) -> bool {
    if line.len() < 10 {
        return false;
    }
    line.chars().all(|c| c == '-' || c == '=')
}

/// PR view 中的叙述性噪音行
fn is_bb_view_noise(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("created:")
        || lower.starts_with("updated:")
        || lower.starts_with("participants:")
        || lower.starts_with("comments:")
}

/// Bitbucket 通用噪音检测
pub(super) fn is_bb_noise(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("description:")
        || lower.starts_with("changes:")
        || lower.starts_with("pull request #")
        || lower.starts_with("created:")
        || lower.starts_with("updated:")
}

/// Compression Protocol V1: 异常状态映射
pub(super) fn map_bb_alert(line: &str) -> Option<String> {
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
