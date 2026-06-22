#![allow(dead_code)]
//! Azure DevOps (az) 压缩方法 — Compression Protocol V1
use super::parser::*;
use crate::core::plugin_config_loader::parse_vcs_command_words_from_line;

// ============================================================================
// 遗留 parser 集成（保留兼容性）
// ============================================================================
#[tracing::instrument(level = "debug", skip_all)]
pub fn process_parser(parser: &dyn VcsParser, raw: &str) -> String {
    match parser.parse(raw) {
        Some(doc) => render_az_doc(&doc),
        None => raw.to_string(),
    }
}

fn render_az_doc(doc: &VcsDocument) -> String {
    let mut lines = Vec::new();
    for rec in &doc.records {
        lines.push(render_az_record(rec));
    }
    lines.join("\n")
}

fn render_az_record(rec: &VcsRecord) -> String {
    match rec {
        VcsRecord::Section(s) => format!("[{}]", s),
        VcsRecord::Commit(c) => format!("id:{}", c),
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
pub fn compact_az_log_for_ai(raw: &str) -> String {
    compact_az_dispatch(raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_az_other_for_ai(raw: &str) -> String {
    compact_az_dispatch(raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn is_az_log_block(_: &str) -> bool {
    true
}

// ============================================================================
// 调度器
// ============================================================================
fn compact_az_dispatch(raw: &str) -> String {
    let cleaned = crate::core::utils::strip_ansi(raw);
    if cleaned.len() < 50 {
        return cleaned;
    }

    let first_line = cleaned
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();

    if let Some((tool, words)) = parse_vcs_command_words_from_line(first_line) {
        if tool == "az" && words.first().is_some_and(|w| w == "repos") {
            match words.get(1).map(String::as_str) {
                Some("show") => return compact_az_show(&cleaned),
                Some("list") => return compact_az_list(&cleaned),
                Some("create") => return compact_az_create(&cleaned),
                Some("delete") => return compact_az_delete(&cleaned),
                _ => {}
            }
        }
    }

    compact_az_generic(&cleaned)
}

// ============================================================================
// Case 96: show — 保留锚点，K-V 扁平化
// ============================================================================
fn compact_az_show(raw: &str) -> String {
    let mut out = Vec::new();
    let mut parts: Vec<String> = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // 保留命令锚点
        if trimmed.starts_with("az repos show") {
            out.push(trimmed.to_string());
            continue;
        }

        if let Some(alert) = map_az_alert(trimmed) {
            out.push(alert);
            continue;
        }

        // 不含冒号的行 → 项目名
        if !trimmed.contains(':') {
            parts.push(format!("PRJ:{}", trimmed));
            continue;
        }

        // K-V 行映射
        if let Some(colon_idx) = trimmed.find(':') {
            let key = trimmed[..colon_idx].trim().to_ascii_lowercase();
            let val = trimmed[colon_idx + 1..].trim();

            match key.as_str() {
                "defaultbranch" => parts.push(format!("BR:{}", val)),
                "remoteurl" => {
                    parts.push(format!("URL:{}", abbreviate_az_url(val)));
                }
                "syncstatus" => parts.push(format!("SS:{}", val)),
                // 跳过 Branches 行（与 DefaultBranch 信息重复）
                "branches" => {}
                _ => {}
            }
        }
    }

    if !parts.is_empty() {
        out.push(parts.join(" "));
    }
    out.join("\n")
}

// ============================================================================
// Case 112: list — 保留锚点，JSON 整块解析，嵌套字段提取
// ============================================================================
fn compact_az_list(raw: &str) -> String {
    let mut lines_iter = raw.lines();
    let anchor = lines_iter.next().unwrap_or("").trim().to_string();

    // 拼接 JSON 体
    let json_body: String = lines_iter
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && *l != "[" && *l != "]")
        .collect::<Vec<_>>()
        .join(" ");

    let mut out = vec![anchor];

    let name = extract_az_json_string(&json_body, "name");
    let id = extract_az_json_string(&json_body, "id");
    let db = extract_az_json_string(&json_body, "defaultBranch");
    let project = extract_nested_json_field(&json_body, "\"project\"");

    let mut parts = Vec::new();
    if let Some(p) = project {
        parts.push(format!("PRJ:{}", p));
    }
    if let Some(n) = name {
        parts.push(format!("REPO:{}", n));
    }
    if let Some(i) = id {
        parts.push(format!("ID:{}", i));
    }
    if let Some(b) = db {
        parts.push(format!("BR:{}", b));
    }
    if !parts.is_empty() {
        out.push(parts.join(" "));
    }

    out.join("\n")
}

/// 提取 JSON 字符串字段: "key":"value" 或 "key": "value"
fn extract_az_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let start = json.find(&pattern)?;
    let rest = &json[start + pattern.len()..];
    // 跳过冒号与空白
    let rest = rest.trim_start_matches(':').trim();
    if rest.starts_with('"') {
        let inner = &rest[1..];
        let end = inner.find('"')?;
        return Some(inner[..end].to_string());
    }
    // 数字/布尔值
    let end = rest
        .find(|c: char| c == ',' || c == '}')
        .unwrap_or(rest.len());
    let val = rest[..end].trim();
    if !val.is_empty() {
        return Some(val.to_string());
    }
    None
}

/// 提取嵌套对象中的 name 字段
fn extract_nested_json_field(json: &str, outer_key: &str) -> Option<String> {
    let start = json.find(outer_key)?;
    let after = &json[start + outer_key.len()..].trim();
    let after = after.trim_start_matches(':').trim();
    if !after.starts_with('{') {
        return None;
    }
    let mut brace_count = 0;
    let mut end_idx = 0;
    for (i, c) in after.char_indices() {
        if c == '{' {
            brace_count += 1;
        } else if c == '}' {
            brace_count -= 1;
            if brace_count == 0 {
                end_idx = i;
                break;
            }
        }
    }
    let inner = &after[..=end_idx];
    extract_az_json_string(inner, "name")
}

// ============================================================================
// Case 160: create — 保留锚点，去除 ✓，A: 映射，URL 缩写
// ============================================================================
fn compact_az_create(raw: &str) -> String {
    let mut out = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("az repos create") {
            out.push(trimmed.to_string());
            continue;
        }

        let cleaned = trimmed.trim_start_matches('✓').trim();

        if let Some(rest) = cleaned.strip_prefix("Repository created:") {
            let content = rest.trim();
            if let Some(url) = extract_az_url(content) {
                let repo_name = url.rsplit("/_git/").next().unwrap_or(content);
                out.push(format!("A:{} URL:{}", repo_name, abbreviate_az_url(url)));
            } else {
                out.push(format!("A:{}", content));
            }
            continue;
        }

        out.push(cleaned.to_string());
    }

    out.join("\n")
}

/// 提取文本中第一个 URL
fn extract_az_url(text: &str) -> Option<&str> {
    if let Some(idx) = text.find("http://") {
        Some(&text[idx..])
    } else if let Some(idx) = text.find("https://") {
        Some(&text[idx..])
    } else {
        None
    }
}

// ============================================================================
// Case 161: delete — 保留锚点，去除 ✓，D: 映射
// ============================================================================
fn compact_az_delete(raw: &str) -> String {
    let mut out = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("az repos delete") {
            out.push(trimmed.to_string());
            continue;
        }

        let cleaned = trimmed.trim_start_matches('✓').trim();

        if let Some(rest) = cleaned.strip_prefix("Repository deleted:") {
            out.push(format!("D:{}", rest.trim()));
            continue;
        }

        out.push(cleaned.to_string());
    }

    out.join("\n")
}

// ============================================================================
// 通用噪音过滤与 fallback
// ============================================================================
fn compact_az_generic(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("az ") && first {
            out.push(trimmed.to_string());
            first = false;
            continue;
        }

        if trimmed == "[" || trimmed == "]" || trimmed == "{" || trimmed == "}" {
            continue;
        }

        let cleaned = trimmed.trim_start_matches('✓').trim();

        if is_az_noise(cleaned) {
            continue;
        }

        if let Some(alert) = map_az_alert(cleaned) {
            out.push(alert);
            continue;
        }

        if let Some(compact) = compact_az_kv(cleaned) {
            out.push(compact);
            continue;
        }

        out.push(cleaned.to_string());
    }

    out.join("\n")
}

fn compact_az_kv(line: &str) -> Option<String> {
    let colon_idx = line.find(':')?;
    let key = line[..colon_idx].trim().to_ascii_lowercase();
    let val = line[colon_idx + 1..].trim();
    if val.is_empty() {
        return None;
    }
    let sym = match key.as_str() {
        "defaultbranch" => "BR",
        "remoteurl" => return Some(format!("URL:{}", abbreviate_az_url(val))),
        "weburl" => return Some(format!("URL:{}", abbreviate_az_url(val))),
        "syncstatus" => "SS",
        "name" => "REPO",
        "project" => "PRJ",
        "id" => "ID",
        _ => &key,
    };
    Some(format!("{}:{}", sym, val))
}

// ============================================================================
// 辅助函数
// ============================================================================
fn abbreviate_az_url(url: &str) -> String {
    url.replace("https://dev.azure.com/", "az:")
        .replace("https://github.com/", "gh:")
        .replace("https://gitlab.com/", "gl:")
        .replace("https://bitbucket.org/", "bb:")
}

pub(super) fn is_az_noise(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("repository created") || lower.starts_with("repository deleted")
}

pub(super) fn map_az_alert(line: &str) -> Option<String> {
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
