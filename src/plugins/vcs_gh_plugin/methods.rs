#![allow(dead_code)]
//! GitHub CLI (gh) 压缩方法 — Compression Protocol V1
use super::parser::*;
use crate::core::plugin_config_loader::parse_vcs_command_words_from_line;
use crate::core::utils::roi::prefer_non_expanding;

// ============================================================================
// 遗留 parser 集成
// ============================================================================
#[tracing::instrument(level = "debug", skip_all)]
pub fn process_parser(parser: &dyn VcsParser, raw: &str) -> String {
    match parser.parse(raw) {
        Some(doc) => render_gh_doc(&doc),
        None => raw.to_string(),
    }
}
fn render_gh_doc(doc: &VcsDocument) -> String {
    doc.records
        .iter()
        .map(|r| render_gh_record(r))
        .collect::<Vec<_>>()
        .join("\n")
}
fn render_gh_record(rec: &VcsRecord) -> String {
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
pub fn compact_gh_log_for_ai(raw: &str) -> String {
    prefer_non_expanding(raw, compact_gh_dispatch(raw))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_gh_other_for_ai(raw: &str) -> String {
    prefer_non_expanding(raw, compact_gh_dispatch(raw))
}
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_gh_log_block(_: &str) -> bool {
    true
}

// ============================================================================
// 调度器
// ============================================================================
fn compact_gh_dispatch(raw: &str) -> String {
    if raw.len() < 50 {
        return raw.to_string();
    }

    let first = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();

    if let Some((tool, words)) = parse_vcs_command_words_from_line(first) {
        if tool == "gh" {
            match (
                words.first().map(String::as_str),
                words.get(1).map(String::as_str),
            ) {
                (Some("pr"), Some("list")) => return compact_gh_pr_list(raw),
                (Some("issue"), Some("list")) => return compact_gh_issue_list(raw),
                (Some("run"), Some("list")) => return compact_gh_run_list(raw),
                (Some("pr"), Some("create")) => return compact_gh_pr_create(raw),
                (Some("issue"), Some("create")) => return compact_gh_issue_create(raw),
                (Some("api"), _) => return compact_gh_api(raw),
                (Some("auth"), _) => return compact_gh_auth(raw),
                (Some("pr"), Some("merge")) => return compact_gh_pr_merge(raw),
                (Some("run"), Some("view")) => return compact_gh_run_view(raw),
                _ => {}
            }
        }
    }

    compact_gh_generic(raw)
}

// ============================================================================
// Case 92: pr list — 保留锚点，列解析
// 格式: #22   feature/new-ui   [open]   Add dark mode support   alice   2026-04-01
// 输出: #22 ST:open OW:@alice CR:2026-04-01 Add dark mode support
// ============================================================================
fn compact_gh_pr_list(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("gh pr list") {
            out.push(t.to_string());
            continue;
        }
        if let Some(row) = parse_gh_pr_row(t) {
            out.push(row);
        }
    }
    out.join("\n")
}

fn parse_gh_pr_row(line: &str) -> Option<String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 4 || !tokens[0].starts_with('#') {
        return None;
    }

    let id = &tokens[0][1..];
    let date = tokens
        .last()
        .filter(|t| t.len() == 10 && t.as_bytes().get(4) == Some(&b'-'));
    let author_idx = if date.is_some() {
        tokens.len() - 2
    } else {
        tokens.len() - 1
    };
    let author = tokens[author_idx];
    let state = tokens
        .iter()
        .find(|t| t.starts_with('[') && t.ends_with(']'))
        .map(|s| &s[1..s.len() - 1]);
    // 标题从状态括号后开始（跳过 #, branch, [state]）
    let state_pos = tokens
        .iter()
        .position(|t| t.starts_with('[') && t.ends_with(']'));
    let has_branch = tokens.get(1).map_or(false, |t| t.contains('/'));
    let title_start = if has_branch {
        // pr_list: columns #ID branch [state] title...
        state_pos.map_or(2, |p| p + 1)
    } else {
        // issue_list: columns #ID title [state]...
        1
    };
    let title = tokens[title_start..author_idx]
        .iter()
        .filter(|t| !t.starts_with('['))
        .copied()
        .collect::<Vec<_>>()
        .join(" ");

    let mut r = format!("#{} ST:{} OW:@{}", id, state.unwrap_or("?"), author);
    if let Some(d) = date {
        r.push_str(&format!(" CR:{}", d));
    }
    r.push_str(&format!(" {}", title));
    Some(r)
}

// ============================================================================
// Case 93: issue list — 复用 pr list 解析
// ============================================================================
fn compact_gh_issue_list(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("gh issue list") {
            out.push(t.to_string());
            continue;
        }
        if let Some(row) = parse_gh_pr_row(t) {
            out.push(row);
        }
    }
    out.join("\n")
}

// ============================================================================
// Case 99: run list — 保留锚点，WF 块压缩
// ============================================================================
fn compact_gh_run_list(raw: &str) -> String {
    let mut out = Vec::new();
    let mut block: Vec<String> = Vec::new();

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            if !block.is_empty() {
                out.push(block.join(" "));
                block.clear();
            }
            continue;
        }
        if t.starts_with("gh run list") {
            out.push(t.to_string());
            continue;
        }
        if let Some(kv) = compact_gh_run_kv(t) {
            block.push(kv);
        }
    }
    if !block.is_empty() {
        out.push(block.join(" "));
    }
    out.join("\n")
}

fn compact_gh_run_kv(line: &str) -> Option<String> {
    let colon = line.find(':')?;
    let key = line[..colon].trim().to_ascii_lowercase();
    let val = line[colon + 1..].trim();
    if val.is_empty() {
        return None;
    }
    let sym = match key.as_str() {
        "workflow" => "WF",
        "status" => "ST",
        "run" => "RN",
        "branch" => "BR",
        "commit" => "CM",
        "duration" => {
            return Some(format!("DUR:{}", val));
        }
        "event" => "EV",
        "conclusion" => "CN",
        _ => &key,
    };
    Some(format!("{}:{}", sym, val))
}

// ============================================================================
// Case 155/157: pr/issue create — 保留锚点，去除 ✓，A: 映射
// ============================================================================
fn compact_gh_pr_create(raw: &str) -> String {
    let mut out = Vec::new();
    let mut labels = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("gh pr create") {
            out.push(t.to_string());
            continue;
        }
        let c = t.trim_start_matches('✓').trim();
        if c.starts_with("Created pull request") {
            out.push(format!("A:{}", compact_gh_result(c)));
            continue;
        }
        if c.starts_with("Labeled as") {
            if let Some(lb) = c.strip_prefix("Labeled as") {
                labels.push(lb.trim().to_string());
            }
            continue;
        }
        if c.starts_with("http") {
            continue;
        }
    }
    if !labels.is_empty() {
        out.push(format!("LB:{}", labels.join(",")));
    }
    out.join("\n")
}

fn compact_gh_issue_create(raw: &str) -> String {
    let mut out = Vec::new();
    let mut labels = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("gh issue create") {
            out.push(t.to_string());
            continue;
        }
        let c = t.trim_start_matches('✓').trim();
        if c.starts_with("Created issue") {
            out.push(format!("A:{}", compact_gh_result(c)));
            continue;
        }
        if c.starts_with("Labeled as") {
            if let Some(lb) = c.strip_prefix("Labeled as") {
                labels.push(lb.trim().to_string());
            }
            continue;
        }
    }
    if !labels.is_empty() {
        out.push(format!("LB:{}", labels.join(",")));
    }
    out.join("\n")
}

fn compact_gh_result(msg: &str) -> String {
    if let Some(idx) = msg.rfind('#') {
        msg[idx..].to_string()
    } else if let Some(idx) = msg.find("http") {
        let url = &msg[idx..];
        let num = url.trim_end_matches('/').rsplit('/').next().unwrap_or("");
        if !num.is_empty() {
            format!("#{}", num)
        } else {
            url.to_string()
        }
    } else {
        msg.to_string()
    }
}

// ============================================================================
// Case 106: api — 保留锚点，JSON 平面化提取
// ============================================================================
fn compact_gh_api(raw: &str) -> String {
    let mut lines = raw.lines();
    let anchor = lines.next().unwrap_or("").trim().to_string();
    let body: String = lines
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && *l != "{" && *l != "}")
        .collect::<Vec<_>>()
        .join(" ");

    let mut out = vec![anchor];
    let mut parts = Vec::new();
    if let Some(v) = extract_gh_json(&body, "name") {
        parts.push(format!("NM:{}", v));
    }
    if let Some(v) = extract_gh_json(&body, "id") {
        parts.push(format!("ID:{}", v));
    }
    if let Some(v) = extract_gh_json(&body, "full_name") {
        parts.push(format!("FN:{}", v));
    }
    if let Some(v) = extract_gh_json(&body, "description") {
        parts.push(format!("DESC:{}", v));
    }
    if let Some(v) = extract_gh_json(&body, "html_url") {
        parts.push(format!("URL:{}", abbreviate_gh_url(&v)));
    }
    if !parts.is_empty() {
        out.push(parts.join(" "));
    }
    out.join("\n")
}

fn extract_gh_json(json: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\"", key);
    let start = json.find(&pat)?;
    let rest = json[start + pat.len()..].trim_start_matches(':').trim();
    if rest.starts_with('"') {
        let inner = &rest[1..];
        let end = inner.find('"')?;
        Some(inner[..end].to_string())
    } else {
        let end = rest
            .find(|c: char| c == ',' || c == '}')
            .unwrap_or(rest.len());
        let v = rest[..end].trim();
        if v == "true" {
            Some("true".into())
        } else if v == "false" {
            Some("false".into())
        } else if !v.is_empty() {
            Some(v.to_string())
        } else {
            None
        }
    }
}

// ============================================================================
// Case 107: auth — 保留锚点，K-V 扁平化
// ============================================================================
fn compact_gh_auth(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("gh auth") && !t.contains(':') {
            out.push(t.to_string());
            continue;
        }
        let c = t.trim_start_matches('✓').trim_start_matches('-').trim();
        if let Some(compact) = compact_gh_kv(c) {
            out.push(compact);
            continue;
        }
        if c.starts_with("Logged in") {
            if let Some(at) = c.rfind("@") {
                let user = c[at..].trim_end_matches(')');
                out.push(format!("OW:{}", user));
            }
            continue;
        }
        if c.starts_with("Current account") {
            if let Some(col) = c.find(':') {
                out.push(format!("ACC:{}", c[col + 1..].trim()));
            }
            continue;
        }
    }
    out.join("\n")
}

// ============================================================================
// Case 156: pr merge — ✓ 去除，动作映射
// ============================================================================
fn compact_gh_pr_merge(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("gh pr merge") {
            out.push(t.to_string());
            continue;
        }
        let c = t.trim_start_matches('✓').trim();
        if c.starts_with("Merged pull request") {
            out.push(format!("MRG:{}", compact_gh_result(c)));
            continue;
        }
        if c.starts_with("Deleted branch") {
            out.push(format!(
                "D:{}",
                c.strip_prefix("Deleted branch").unwrap_or("").trim()
            ));
            continue;
        }
    }
    out.join("\n")
}

// ============================================================================
// Case 162: run view — K-V 扁平化
// ============================================================================
fn compact_gh_run_view(raw: &str) -> String {
    let mut out = Vec::new();
    let mut parts = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("gh run view") {
            out.push(t.to_string());
            continue;
        }
        // 跳过缩进的任务行
        if t.starts_with("  ") {
            continue;
        }
        if let Some(kv) = compact_gh_run_view_kv(t) {
            parts.push(kv);
            continue;
        }
    }
    if !parts.is_empty() {
        out.push(parts.join(" "));
    }
    out.join("\n")
}

fn compact_gh_run_view_kv(line: &str) -> Option<String> {
    let colon = line.find(':')?;
    let key = line[..colon].trim().to_ascii_lowercase();
    let val = line[colon + 1..].trim();
    if val.is_empty() {
        return None;
    }
    let sym = match key.as_str() {
        "workflow" => "WF",
        "run" => "RN",
        "status" => "ST",
        "branch" => "BR",
        "event" => "EV",
        "conclusion" => "CN",
        "jobs" => "JB",
        _ => &key,
    };
    Some(format!("{}:{}", sym, val))
}

// ============================================================================
// 通用 fallback — 处理剩余的 list/view/gist 等
// ============================================================================
fn compact_gh_generic(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if t.starts_with("gh ") && first {
            out.push(t.to_string());
            first = false;
            continue;
        }

        // 分隔线 / 表头
        if is_gh_separator(t) || is_gh_table_header(t) {
            continue;
        }

        // 去除 ✓ ✗ ○ 符号（全局替换，非仅行首）+ 空格压缩
        let c = t
            .replace('✓', " ")
            .replace('✗', " ")
            .replace('○', " ")
            .trim()
            .to_string();

        // 通用空格压缩：2个以上连续空格 → 单个空格
        let c = collapse_whitespace(&c);

        // 噪音
        if is_gh_noise(&c) {
            continue;
        }

        // K-V 压缩
        if let Some(kv) = compact_gh_kv(&c) {
            out.push(kv);
            continue;
        }

        // 状态行
        if let Some(status) = compact_gh_status_line(&c) {
            out.push(status);
            continue;
        }

        // 异常映射
        if let Some(alert) = map_gh_alert(&c) {
            out.push(alert);
            continue;
        }

        out.push(c);
    }
    out.join("\n")
}

/// 压缩连续空格：2个以上 → 单个空格
fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut last_was_space = false;
    for c in s.chars() {
        if c.is_ascii_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(c);
            last_was_space = false;
        }
    }
    result.trim().to_string()
}

fn compact_gh_kv(line: &str) -> Option<String> {
    let c = if line.starts_with("- ") {
        &line[2..]
    } else {
        line
    };
    let colon = c.find(':')?;
    let key = c[..colon].trim().to_ascii_lowercase();
    let val = c[colon + 1..].trim();
    if val.is_empty() {
        return None;
    }

    let sym = match key.as_str() {
        "workflow" => "WF",
        "status" | "state" => "ST",
        "branch" => "BR",
        "event" => "EV",
        "conclusion" => "CN",
        "author" => "AU",
        "assignee" | "assignees" => "AS",
        "labels" => "LB",
        "created" => "CR",
        "updated" => "UP",
        "description" => "DC",
        "url" | "html_url" | "web url" => return Some(format!("URL:{}", abbreviate_gh_url(val))),
        "visibility" => "VIS",
        "default branch" => "DB",
        "license" => "LC",
        "run" => "RN",
        "jobs" => "JB",
        "owner" => "OW",
        "project" => "PRJ",
        "source" => "SRC",
        "target" => "TGT",
        "milestone" => "MS",
        "due" => "DU",
        "priority" => "PRI",
        "node" => "NODE",
        "manager" => "MGR",
        "current account" => "ACC",
        "merge commit" => "MC",
        "id" => "ID",
        "name" => "NM",
        _ => &key,
    };
    Some(format!("{}:{}", sym, val))
}

fn compact_gh_status_line(line: &str) -> Option<String> {
    let rest = line
        .trim_start_matches(|c: char| c == '✓' || c == '✗' || c == '○')
        .trim();
    if rest.is_empty() || rest == line.trim() {
        return None;
    }
    if let Some(idx) = rest.find("http") {
        let url = &rest[idx..];
        Some(format!(
            "{} URL:{}",
            rest[..idx].trim(),
            abbreviate_gh_url(url)
        ))
    } else {
        Some(rest.to_string())
    }
}

// ============================================================================
// 辅助函数
// ============================================================================
fn is_gh_separator(line: &str) -> bool {
    line.len() >= 10 && line.chars().all(|c| c == '-')
}
fn is_gh_table_header(line: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.len() < 3 {
        return false;
    }
    // ALL-CAPS 表头 (WORKFLOW, STATUS, DURATION, TRIGGER, DESCRIPTION, FILES, ENVIRONMENT, ACTIVE...)
    let caps_count = words
        .iter()
        .filter(|w| w.len() >= 2 && w.chars().all(|c| c.is_ascii_uppercase()))
        .count();
    if caps_count >= 3 {
        return true;
    }
    // 小写关键词表头 (name, description, visibility, updated, files, public 等)
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
fn is_gh_noise(line: &str) -> bool {
    if line_has_preserved_keyword(line) {
        return false;
    }
    let l = line.to_ascii_lowercase();
    l.starts_with("created pull request")
        || l.starts_with("created issue")
        || l.starts_with("labeled as")
        || l.starts_with("merged pull request")
        || l.starts_with("deleted branch")
        || l.starts_with("logged in")
        || l.starts_with("steps to reproduce")
        || l.starts_with("expected:")
        || l.starts_with("actual:")
        || l.starts_with("url:")
        || l.starts_with("description:")
        || l.starts_with("changes:")
}

fn line_has_preserved_keyword(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.contains("error")
        || l.contains("fatal")
        || l.contains("panic")
        || l.contains("exception")
        || l.contains("uncaught")
}
fn abbreviate_gh_url(url: &str) -> String {
    url.replace("https://github.com/", "gh:")
        .replace("https://gitlab.com/", "gl:")
        .replace("https://dev.azure.com/", "az:")
        .replace("https://bitbucket.org/", "bb:")
        .replace("https://gist.github.com/", "gist:")
        .replace("https://api.github.com/repos/", "api:")
}
pub(super) fn map_gh_alert(line: &str) -> Option<String> {
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
