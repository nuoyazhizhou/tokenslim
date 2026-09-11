#![allow(dead_code)]
//! GitLab (glab) 压缩方法 — Compression Protocol V1
use crate::core::plugin_config_loader::parse_vcs_command_words_from_line;
use crate::core::utils::roi::prefer_non_expanding;

// ============================================================================
// 公开 API
// ============================================================================
/// glab 日志的 AI 压缩入口：dispatch 压缩 + ROI 门控。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_glab_log_for_ai(raw: &str) -> String {
    prefer_non_expanding(raw, compact_glab_dispatch(raw))
}

/// glab 其他输出的 AI 压缩入口：dispatch 压缩 + ROI 门控。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_glab_other_for_ai(raw: &str) -> String {
    prefer_non_expanding(raw, compact_glab_dispatch(raw))
}

/// 判断是否为 glab 命令块：首个非空行以 glab 命令头开头。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_glab_log_block(text: &str) -> bool {
    text.lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim_start().starts_with("glab "))
        .unwrap_or(false)
}

// ============================================================================
// 调度器
// ============================================================================
/// 调度器：按首行 glab 命令分派到 mr/issue list/view/create 专用压缩。
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
/// 压缩 glab mr list 输出：保留锚点，跳过表头/分隔线，行解析为 !ID ST: OW: 格式。
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
/// 压缩 glab issue list 输出：行解析为 #ID ST: OW: LB: 格式。
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
    // 守卫：至少需要 ID+标题+标签+指派+作者+状态 6 列；len<5 时 tokens[1..len-4]
    // 会退化为逆序切片（如 tokens[1..0]）直接 panic，故收紧到 <5 提前拒绝。
    if tokens.len() < 5 {
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
/// 压缩 glab mr view 输出：K-V 扁平化（ST/OW/RV/BR/URL），Description 单独 DESC 行。
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

/// 从标题行提取 MR 编号（!123）。
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

/// 将 mr view 的 K-V 行映射为符号标记（ST/OW/RV/BR/URL/AS）。
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
/// 压缩 glab issue view 输出：K-V 扁平化，Description 单独 DESC 行。
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
/// 压缩 glab mr create 输出：Created merge request 映射为 A:。
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
/// 压缩 glab issue create 输出：Created issue 映射为 A:。
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
/// 通用压缩：保留命令锚点，过滤分隔线/表头/噪音/URL，警报映射。
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
/// 判断是否为分隔线（≥10 个 - 或 =）。
fn is_glab_separator(line: &str) -> bool {
    line.len() >= 10 && line.chars().all(|c| c == '-' || c == '=')
}

/// 判断是否为表格表头行。对齐 gh 的「全大写 + 表头关键词」双判据（P3-189）：
/// 旧判据为「首字母大写词 ≥3」，会把 Title Case 普通数据行（如
/// `Add User Authentication Flow`）误判为表头而在 compact_glab_generic/mr_list 中整行
/// 丢弃，造成数据丢失。现仅当 ≥3 个「全大写」词或命中 ≥3 个表头关键词时才判表头。
fn is_glab_table_header(line: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.len() < 3 {
        return false;
    }
    // ALL-CAPS 表头（STATE/STATUS/MILESTONE/LABELS...）
    let caps_count = words
        .iter()
        .filter(|w| w.len() >= 2 && w.chars().all(|c| c.is_ascii_uppercase()))
        .count();
    if caps_count >= 3 {
        return true;
    }
    // 表头关键词命中（gitlab mr/issue 表格常见列名）
    let kw = [
        "name",
        "description",
        "visibility",
        "updated",
        "created",
        "files",
        "public",
        "active",
        "deploy",
        "environment",
        "title",
        "labels",
        "assignee",
        "author",
        "status",
        "duration",
        "trigger",
    ];
    let kw_count = words
        .iter()
        .filter(|w| kw.contains(&w.to_ascii_lowercase().as_str()))
        .count();
    kw_count >= 3
}

/// 判断是否为 mr/issue view 噪音行（created/updated/milestone/labels 等）。
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

/// 判断是否为 glab 噪音行（creating/created merge request 等）。
fn is_glab_noise(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("creating merge request")
        || lower.starts_with("merge request created")
        || lower.starts_with("created merge request")
        || lower.starts_with("created issue")
        || lower.starts_with("url:")
}

/// 判断是否为 Description 段边界（Changes/Steps to reproduce/Expected/Actual）。
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

/// 判断行是否含需保留的错误关键词（error/fatal/panic 等）。
fn line_has_preserved_keyword(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.contains("error")
        || l.contains("fatal")
        || l.contains("panic")
        || l.contains("exception")
        || l.contains("uncaught")
}

/// 缩写 URL 主机名（gitlab/github/azure/bitbucket）。
fn abbreviate_glab_url(url: &str) -> String {
    url.replace("https://gitlab.com/", "gl:")
        .replace("https://github.com/", "gh:")
        .replace("https://dev.azure.com/", "az:")
        .replace("https://bitbucket.org/", "bb:")
}

/// 综合噪音判断：is_glab_noise 或 is_glab_view_noise。
pub(super) fn is_glab_noise_line(line: &str) -> bool {
    is_glab_noise(line) || is_glab_view_noise(line)
}

/// 将含 conflict/error/failed/rejected 的行标记为警报（前缀 !）。
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
