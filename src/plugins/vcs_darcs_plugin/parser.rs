#![allow(dead_code)]
//! Darcs 解析器 - 自包含

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum VcsTool {
    Darcs,
}
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum VcsDocKind {
    Status,
    Log,
    Diff,
    Show,
}
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum VcsRecord {
    Section(String),
    Branch(String),
    File { status: Option<char>, path: String },
    LabeledFile { label: String, path: String },
    DiffFile { left: String, right: String },
    Subject(String),
    Author(String),
    Date(String),
    Commit(String),
    Stat(String),
    Patch(String),
    Hunk(String),
    Raw(String),
}
impl std::fmt::Display for VcsRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VcsRecord::Section(s) => write!(f, "[{}]", s),
            VcsRecord::Branch(b) => write!(f, "branch: {}", b),
            VcsRecord::File { status, path } => {
                if let Some(st) = status {
                    write!(f, "{} {}", st, path)
                } else {
                    write!(f, "{}", path)
                }
            }
            VcsRecord::LabeledFile { label, path } => write!(f, "{}: {}", label, path),
            VcsRecord::DiffFile { left, right } => write!(f, "--- {}\n+++ {}", left, right),
            VcsRecord::Subject(s) => write!(f, "{}", s),
            VcsRecord::Author(a) => write!(f, "au:{}", a.trim_start_matches("Author:").trim()),
            VcsRecord::Date(d) => write!(f, "Date: {}", d),
            VcsRecord::Commit(c) => write!(f, "commit {}", c),
            VcsRecord::Stat(s) => write!(f, "{}", s),
            VcsRecord::Patch(p) => write!(f, "{}", p),
            VcsRecord::Hunk(p) => write!(f, "{}", p),
            VcsRecord::Raw(r) => write!(f, "{}", r),
        }
    }
}
pub struct VcsDocument {
    pub tool: VcsTool,
    pub kind: VcsDocKind,
    pub records: Vec<VcsRecord>,
}
pub trait VcsParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument>;
}
pub struct DarcsStatusParser;
pub struct DarcsDiffParser;
pub struct DarcsLogParser;
pub struct DarcsRecordParser;
pub struct DarcsAmendParser;
pub struct DarcsObliterateParser;
pub struct DarcsWhatsnewParser;

// --- helpers ---
#[tracing::instrument(level = "debug", skip_all)]
fn to_doc_if_any(tool: VcsTool, kind: VcsDocKind, records: Vec<VcsRecord>) -> Option<VcsDocument> {
    if records.is_empty() {
        None
    } else {
        Some(VcsDocument {
            tool,
            kind,
            records,
        })
    }
}
#[tracing::instrument(level = "debug", skip_all)]
fn parse_simple_status_path(line: &str) -> Option<(char, String)> {
    let token_end = line.find(char::is_whitespace).unwrap_or(line.len());
    if token_end == 0 {
        return None;
    }
    let status_token = &line[..token_end];
    let rest = line[token_end..].trim_start();
    if rest.is_empty() {
        return None;
    }
    let allowed = |c: char| matches!(c, 'M' | 'A' | 'D' | 'R' | 'C' | '!' | '?' | '~' | 'I');
    let status = if status_token.len() == 1 {
        let first = status_token.chars().next()?;
        if !allowed(first) {
            return None;
        }
        first
    } else if status_token.len() == 2 && !status_token.chars().any(|c| c.is_ascii_digit()) {
        let mut it = status_token.chars();
        let a = it.next()?;
        let b = it.next()?;
        if !allowed(a) || !allowed(b) {
            return None;
        }
        if a == '?' && b == '?' {
            '?'
        } else {
            a
        }
    } else {
        return None;
    };
    Some((status, rest.to_string()))
}
#[tracing::instrument(level = "debug", skip_all)]
fn looks_like_vcs_path(path: &str) -> bool {
    let trimmed = path.trim_matches('"');
    // 过滤邮箱和 URL
    if trimmed.contains('@') || trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return false;
    }
    // 过滤代码调用模式
    if trimmed.contains('(')
        || trimmed.contains(')')
        || trimmed.contains(';')
        || trimmed.contains('=')
        || trimmed.contains('<')
        || trimmed.contains('>')
    {
        return false;
    }
    // 过滤方法名模式（如 .trim_end_matches）
    if trimmed.starts_with('.')
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
        && trimmed
            .chars()
            .skip(1)
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return false;
    }
    // 过滤常见顶级域名
    let common_tlds = [
        ".com", ".org", ".net", ".io", ".edu", ".gov", ".mil", ".int",
    ];
    if common_tlds
        .iter()
        .any(|tld| trimmed.eq_ignore_ascii_case(tld))
    {
        return false;
    }
    // 判断是路径的正向条件
    trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.ends_with(".rs")
        || trimmed.ends_with(".md")
        || trimmed.ends_with(".txt")
        || trimmed.ends_with(".lock")
        || (!trimmed.contains(' ')
            && trimmed.contains('.')
            && trimmed.chars().any(|c| c.is_ascii_alphabetic()))
        || trimmed.starts_with('.')
}
#[tracing::instrument(level = "debug", skip_all)]
fn collapse_inline_whitespace(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}
#[tracing::instrument(level = "debug", skip_all)]
fn split_first_token(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let token_end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    let token = &trimmed[..token_end];
    let rest = trimmed[token_end..].trim_start();
    Some((token, rest))
}
fn parse_generic_patch_or_stat_line(
    line: &str,
    trimmed: &str,
    records: &mut Vec<VcsRecord>,
) -> bool {
    if trimmed.starts_with("index ") || trimmed.starts_with("--- ") || trimmed.starts_with("+++ ") {
        records.push(VcsRecord::Raw(trimmed.to_string()));
        return true;
    }
    if trimmed.starts_with("@@") {
        records.push(VcsRecord::Hunk(trimmed.to_string()));
        return true;
    }
    if (line.starts_with('+') || line.starts_with('-') || line.starts_with(' '))
        && !trimmed.starts_with("+++")
        && !trimmed.starts_with("---")
    {
        records.push(VcsRecord::Patch(line.to_string()));
        return true;
    }
    false
}
#[tracing::instrument(level = "debug", skip_all)]
fn parse_status_word_and_path(line: &str) -> Option<(char, String)> {
    let lower = line.to_ascii_lowercase();
    let candidates = [
        ("modified:", 'M'),
        ("added:", 'A'),
        ("removed:", 'D'),
        ("deleted:", 'D'),
        ("renamed:", 'R'),
    ];
    for (prefix, status) in candidates {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let start = line.len() - rest.len();
            let path = line[start..].trim();
            if !path.is_empty() {
                return Some((status, path.to_string()));
            }
        }
    }
    None
}
#[tracing::instrument(level = "debug", skip_all)]
fn parse_darcs_hunk_record(line: &str) -> Option<VcsRecord> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("hunk ")?;
    let (path, _suffix) = split_first_token(rest)?;
    if path.is_empty() {
        return None;
    }
    Some(VcsRecord::File {
        status: None,
        path: path.to_string(),
    })
}

fn parse_generic_status_for_tool(
    raw: &str,
    tool: VcsTool,
    command_prefixes: &[&str],
) -> Option<VcsDocument> {
    let mut records = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if command_prefixes
            .iter()
            .any(|prefix| lower.starts_with(prefix))
        {
            continue;
        }
        if let Some((status, path)) = parse_simple_status_path(trimmed) {
            records.push(VcsRecord::File {
                status: Some(status),
                path,
            });
            continue;
        }
        if let Some((status, path)) = parse_status_word_and_path(trimmed) {
            records.push(VcsRecord::File {
                status: Some(status),
                path,
            });
            continue;
        }
        if let Some(record) = parse_darcs_hunk_record(trimmed) {
            records.push(record);
            continue;
        }
        if looks_like_vcs_path(trimmed) && !trimmed.contains(':') && trimmed.len() < 220 {
            records.push(VcsRecord::File {
                status: None,
                path: trimmed.to_string(),
            });
            continue;
        }
        records.push(VcsRecord::Raw(trimmed.to_string()));
    }
    to_doc_if_any(tool, VcsDocKind::Status, records)
}
fn parse_generic_log_for_tool(
    raw: &str,
    tool: VcsTool,
    command_prefixes: &[&str],
) -> Option<VcsDocument> {
    let mut records = Vec::new();
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.chars().all(|c| c == '-' || c == '=') && trimmed.len() >= 3 {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if command_prefixes
            .iter()
            .any(|prefix| lower.starts_with(prefix))
        {
            continue;
        }
        if lower.starts_with("author:")
            || lower.starts_with("committer:")
            || lower.starts_with("user:")
        {
            records.push(VcsRecord::Author(trimmed.to_string()));
            continue;
        }
        if lower.starts_with("date:") || lower.starts_with("timestamp:") {
            let value = trimmed
                .split_once(':')
                .map(|(_, v)| v.trim())
                .unwrap_or(trimmed);
            records.push(VcsRecord::Date(value.to_string()));
            continue;
        }
        if line.starts_with("    ") || line.starts_with('\t') {
            records.push(VcsRecord::Subject(trimmed.to_string()));
            continue;
        }
        if looks_like_vcs_path(trimmed) && !trimmed.contains(':') && trimmed.len() < 220 {
            records.push(VcsRecord::File {
                status: None,
                path: trimmed.to_string(),
            });
            continue;
        }
        records.push(VcsRecord::Raw(trimmed.to_string()));
    }
    to_doc_if_any(tool, VcsDocKind::Log, records)
}
fn parse_generic_diff_for_tool(
    raw: &str,
    tool: VcsTool,
    command_prefixes: &[&str],
) -> Option<VcsDocument> {
    let mut records = Vec::new();
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.chars().all(|c| c == '-' || c == '=') && trimmed.len() >= 3 {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if command_prefixes
            .iter()
            .any(|prefix| lower.starts_with(prefix))
        {
            continue;
        }
        if let Some(path) = trimmed.strip_prefix("Index: ") {
            let p = path.trim().to_string();
            if !p.is_empty() {
                records.push(VcsRecord::DiffFile {
                    left: p.clone(),
                    right: p,
                });
                continue;
            }
        }
        if parse_generic_patch_or_stat_line(line, trimmed, &mut records) {
            continue;
        }
        if let Some(record) = parse_darcs_hunk_record(trimmed) {
            records.push(record);
            continue;
        }
        if looks_like_vcs_path(trimmed) && !trimmed.contains(':') && trimmed.len() < 220 {
            records.push(VcsRecord::File {
                status: None,
                path: trimmed.to_string(),
            });
            continue;
        }
        records.push(VcsRecord::Raw(trimmed.to_string()));
    }
    to_doc_if_any(tool, VcsDocKind::Diff, records)
}

// --- Parsers ---
impl VcsParser for DarcsStatusParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        parse_generic_status_for_tool(raw, VcsTool::Darcs, &["darcs status", "darcs whatsnew"])
    }
}
impl VcsParser for DarcsDiffParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        parse_generic_diff_for_tool(raw, VcsTool::Darcs, &["darcs diff"])
    }
}
impl VcsParser for DarcsLogParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        parse_generic_log_for_tool(raw, VcsTool::Darcs, &["darcs log", "darcs changes"])
    }
}
impl VcsParser for DarcsRecordParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        parse_generic_log_for_tool(raw, VcsTool::Darcs, &["darcs record"])
    }
}
impl VcsParser for DarcsAmendParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        parse_generic_log_for_tool(raw, VcsTool::Darcs, &["darcs amend"])
    }
}
impl VcsParser for DarcsObliterateParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        parse_generic_log_for_tool(raw, VcsTool::Darcs, &["darcs obliterate"])
    }
}
impl VcsParser for DarcsWhatsnewParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        parse_generic_status_for_tool(raw, VcsTool::Darcs, &["darcs whatsnew"])
    }
}
