#![allow(dead_code)]
//! CVS 解析器 - 自包含，无外部依赖

// ============================================================================
// 核心类型定义
// ============================================================================

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum VcsTool {
    Cvs,
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
            VcsRecord::Branch(b) => write!(f, "BR:{}", b),
            VcsRecord::File { status, path } => {
                if let Some(st) = status {
                    write!(f, "{} {}", st, path)
                } else {
                    write!(f, "{}", path)
                }
            }
            VcsRecord::LabeledFile { label, path } => write!(f, "ST:{} {}", label, path),
            VcsRecord::DiffFile { left, right } => write!(f, "--- {}\n+++ {}", left, right),
            VcsRecord::Subject(s) => write!(f, "{}", s),
            VcsRecord::Author(a) => write!(f, "OW:@{}", a.trim_start_matches("Author:").trim()),
            VcsRecord::Date(d) => write!(f, "Date: {}", d),
            VcsRecord::Commit(c) => write!(f, "CH:{}", c),
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

pub struct CvsStatusParser;
pub struct CvsDiffParser;
pub struct CvsLogParser;
pub struct CvsAnnotateParser;
pub struct CvsUpdateParser;
pub struct CvsCommitParser;
pub struct CvsTagParser;
pub struct CvsEditParser;

// ============================================================================
// 通用辅助函数（内联自 helpers.rs）
// ============================================================================

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
    } else if (status_token.starts_with('R') || status_token.starts_with('C'))
        && status_token.len() > 1
        && status_token[1..].chars().all(|c| c.is_ascii_digit())
    {
        status_token.chars().next().unwrap()
    } else {
        return None;
    };
    Some((status, rest.to_string()))
}

fn parse_single_char_status_path(line: &str, allowed: &[char]) -> Option<(char, String)> {
    let token_end = line.find(char::is_whitespace).unwrap_or(line.len());
    if token_end != 1 {
        return None;
    }
    let status = line[..token_end].chars().next()?;
    if !allowed.contains(&status) {
        return None;
    }
    let path = line[token_end..].trim_start();
    if path.is_empty() {
        return None;
    }
    Some((status, path.to_string()))
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
        if let Some(rest) = trimmed.strip_prefix("commit ") {
            let id = rest
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string();
            if !id.is_empty() {
                records.push(VcsRecord::Commit(id));
                continue;
            }
        }
        if let Some(rest) = trimmed.strip_prefix("revision ") {
            let id = rest
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string();
            if !id.is_empty() {
                records.push(VcsRecord::Commit(format!("r{}", id)));
                continue;
            }
        }
        if lower.starts_with("author:")
            || lower.starts_with("committer:")
            || lower.starts_with("user:")
        {
            records.push(VcsRecord::Author(trimmed.to_string()));
            continue;
        }
        if lower.starts_with("date:")
            || lower.starts_with("timestamp:")
            || lower.starts_with("time:")
        {
            let value = trimmed
                .split_once(':')
                .map(|(_, v)| v.trim())
                .unwrap_or(trimmed);
            records.push(VcsRecord::Date(compact_log_date_value(value)));
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
                records.push(VcsRecord::Raw(format!("DIFF://{}", p)));
                continue;
            }
        }
        if parse_generic_patch_or_stat_line(line, trimmed, &mut records) {
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

fn parse_status_word_and_path(line: &str) -> Option<(char, String)> {
    let lower = line.to_ascii_lowercase();
    let candidates = [
        ("modified:", 'M'),
        ("modified ", 'M'),
        ("added:", 'A'),
        ("added ", 'A'),
        ("removed:", 'D'),
        ("removed ", 'D'),
        ("deleted:", 'D'),
        ("deleted ", 'D'),
        ("renamed:", 'R'),
        ("renamed ", 'R'),
        ("unknown:", '?'),
        ("unknown ", '?'),
        ("missing:", '!'),
        ("missing ", '!'),
        ("edited ", 'M'),
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

fn parse_generic_patch_or_stat_line(
    line: &str,
    trimmed: &str,
    records: &mut Vec<VcsRecord>,
) -> bool {
    if trimmed.starts_with("index ")
        || trimmed.starts_with("new file mode ")
        || trimmed.starts_with("deleted file mode ")
        || trimmed.starts_with("old mode ")
        || trimmed.starts_with("new mode ")
        || trimmed.starts_with("similarity index ")
        || trimmed.starts_with("rename from ")
        || trimmed.starts_with("rename to ")
        || trimmed.starts_with("Binary files ")
        || trimmed.starts_with("--- ")
        || trimmed.starts_with("+++ ")
        || trimmed.starts_with("*** ")
    {
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
    if looks_like_diff_stat_line(trimmed) {
        records.push(VcsRecord::Stat(compact_diff_stat_line(trimmed)));
        return true;
    }
    false
}

#[tracing::instrument(level = "debug", skip_all)]
fn looks_like_diff_stat_line(line: &str) -> bool {
    line.contains(" | ")
        || line.ends_with("files changed")
        || line.ends_with("file changed")
        || line.contains("insertion(+)")
        || line.contains("insertions(+)")
        || line.contains("deletion(-)")
        || line.contains("deletions(-)")
}

#[tracing::instrument(level = "debug", skip_all)]
fn compact_diff_stat_line(line: &str) -> String {
    let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
    normalized
}

#[tracing::instrument(level = "debug", skip_all)]
fn compact_log_date_value(value: &str) -> String {
    value.trim().to_string()
}

// ============================================================================
// CVS 专用辅助函数（内联自 helpers.rs）
// ============================================================================

fn parse_cvs_checking_in_path(line: &str) -> Option<String> {
    let path = line
        .strip_prefix("Checking in ")?
        .trim_end_matches(';')
        .trim();
    if path.is_empty() || !looks_like_vcs_path(path) {
        return None;
    }
    Some(path.to_string())
}

fn looks_like_cvs_revision_token(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed.contains('.')
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '.' || ch == '_')
}

fn parse_cvs_tag_name_from_command(line: &str) -> Option<String> {
    let mut parts = line.split_whitespace();
    let tool = parts.next()?;
    let command = parts.next()?;
    if !tool.eq_ignore_ascii_case("cvs") || !command.eq_ignore_ascii_case("tag") {
        return None;
    }
    parts
        .find(|part| !part.starts_with('-'))
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
}

fn parse_cvs_quoted_path(line: &str) -> Option<String> {
    let extract = |start_char: char, end_char: char| -> Option<String> {
        let start = line.find(start_char)?;
        let rest = &line[start + start_char.len_utf8()..];
        let end = rest.find(end_char)?;
        let path = rest[..end].trim();
        if path.is_empty() || !looks_like_vcs_path(path) {
            return None;
        }
        Some(path.to_string())
    };
    extract('`', '\'')
        .or_else(|| extract('\'', '\''))
        .or_else(|| extract('"', '"'))
}

struct CvsAnnotateLine {
    revision: String,
    author: String,
    date: String,
    rendered: String,
    blanked_rendered: String,
}

fn parse_cvs_annotate_line(line: &str) -> Option<CvsAnnotateLine> {
    let line = line.trim_end_matches('\r');
    let open_paren = line.find('(')?;
    let prefix_head = line[..open_paren].trim_end();
    if !prefix_head.starts_with("***") {
        return None;
    }
    let revision = prefix_head.trim_start_matches('*').trim();
    if revision.is_empty() {
        return None;
    }
    let meta_tail = &line[open_paren + 1..];
    let close_meta = meta_tail.find("):")?;
    let meta = &meta_tail[..close_meta];
    let (author_raw, date_raw) = meta.split_once(':')?;
    let author = author_raw.trim();
    let date = date_raw.trim();
    if author.is_empty() || date.is_empty() {
        return None;
    }
    let after_meta = &meta_tail[close_meta + 2..];
    let content = if after_meta.trim().is_empty() {
        String::new()
    } else {
        compact_blame_content_after_prefix(after_meta)
    };
    let prefix = format!("*** {} ({}: {}):", revision, author, date);
    Some(CvsAnnotateLine {
        revision: revision.to_string(),
        author: author.to_string(),
        date: date.to_string(),
        rendered: render_compacted_blame_line(&prefix, &content),
        blanked_rendered: blank_compacted_blame_line(&prefix, &content),
    })
}

fn render_compacted_blame_line(prefix: &str, content: &str) -> String {
    let stripped = prefix.trim_start();
    let pad = prefix.len().saturating_sub(stripped.len());
    let mut out = " ".repeat(pad);
    out.push_str(stripped);
    if !content.is_empty() {
        out.push(' ');
        out.push_str(content);
    }
    out
}

fn blank_compacted_blame_line(prefix: &str, content: &str) -> String {
    let mut chars: Vec<char> = prefix.chars().collect();
    for ch in chars.iter_mut() {
        if !ch.is_whitespace() {
            *ch = ' ';
        }
    }
    let mut rendered = chars.into_iter().collect::<String>();
    rendered.push(' ');
    rendered.push_str(content);
    rendered
}

fn compact_blame_content_after_prefix(content: &str) -> String {
    let trimmed = content
        .strip_prefix(' ')
        .or_else(|| content.strip_prefix('\t'))
        .unwrap_or(content);
    compact_blame_code_indent(trimmed)
}

fn compact_blame_code_indent(content: &str) -> String {
    let trimmed_end = content.trim_end();
    if trimmed_end.is_empty() {
        return String::new();
    }
    let leading_spaces = trimmed_end.chars().take_while(|c| *c == ' ').count();
    let body = trimmed_end.trim_start_matches(' ');
    if body.is_empty() {
        return String::new();
    }
    let unit = if leading_spaces >= 4 && leading_spaces % 4 == 0 {
        4
    } else if leading_spaces >= 2 {
        2
    } else {
        1
    };
    let levels = if unit > 0 { leading_spaces / unit } else { 0 };
    let kept_levels = if levels <= 1 { levels } else { levels / 2 };
    let kept_spaces = " ".repeat(kept_levels * unit);
    format!("{}{}", kept_spaces, body)
}

// ============================================================================
// CVS 解析器实现
// ============================================================================

impl VcsParser for CvsStatusParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        parse_generic_status_for_tool(raw, VcsTool::Cvs, &["cvs status", "cvs update", "cvs -q"])
    }
}

impl VcsParser for CvsDiffParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        parse_generic_diff_for_tool(raw, VcsTool::Cvs, &["cvs diff"])
    }
}

impl VcsParser for CvsLogParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        parse_generic_log_for_tool(raw, VcsTool::Cvs, &["cvs log"])
    }
}

impl VcsParser for CvsAnnotateParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut matched_annotate_line = false;
        let mut last_revision: Option<String> = None;
        let mut last_author: Option<String> = None;
        let mut last_date: Option<String> = None;
        for line in raw.lines() {
            let line = line.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.is_empty() {
                last_revision = None;
                last_author = None;
                last_date = None;
                continue;
            }
            if trimmed.to_ascii_lowercase().starts_with("cvs annotate") {
                records.push(VcsRecord::Raw(trimmed.to_string()));
                last_revision = None;
                last_author = None;
                last_date = None;
                continue;
            }
            if let Some(parsed) = parse_cvs_annotate_line(line) {
                matched_annotate_line = true;
                let is_repeat = last_revision.as_deref() == Some(parsed.revision.as_str())
                    && last_author.as_deref() == Some(parsed.author.as_str())
                    && last_date.as_deref() == Some(parsed.date.as_str());
                records.push(VcsRecord::Raw(if is_repeat {
                    parsed.blanked_rendered
                } else {
                    parsed.rendered
                }));
                last_revision = Some(parsed.revision);
                last_author = Some(parsed.author);
                last_date = Some(parsed.date);
                continue;
            }
            last_revision = None;
            last_author = None;
            last_date = None;
            records.push(VcsRecord::Raw(trimmed.to_string()));
        }
        if matched_annotate_line {
            to_doc_if_any(VcsTool::Cvs, VcsDocKind::Show, records)
        } else {
            None
        }
    }
}

impl VcsParser for CvsUpdateParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() {
                continue;
            }
            let lower = trimmed.to_ascii_lowercase();
            if lower == "cvs update" || lower.starts_with("cvs update: updating ") {
                continue;
            }
            if let Some((status, path)) =
                parse_single_char_status_path(trimmed, &['U', 'A', 'R', 'M', 'D', 'C', '?', '!'])
            {
                records.push(VcsRecord::File {
                    status: Some(status),
                    path,
                });
                continue;
            }
            if looks_like_vcs_path(trimmed) && !trimmed.contains(':') && trimmed.len() < 220 {
                records.push(VcsRecord::File {
                    status: None,
                    path: trimmed.to_string(),
                });
                continue;
            }
            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }
        to_doc_if_any(VcsTool::Cvs, VcsDocKind::Status, records)
    }
}

impl VcsParser for CvsCommitParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut pending_path: Option<String> = None;
        let mut pending_revision: Option<String> = None;
        let flush_pending =
            |records: &mut Vec<VcsRecord>, pp: &mut Option<String>, pr: &mut Option<String>| {
                if let Some(path) = pp.take() {
                    records.push(VcsRecord::LabeledFile {
                        label: pr.take().unwrap_or_else(|| "checkin".to_string()),
                        path,
                    });
                }
            };
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(path) = parse_cvs_checking_in_path(trimmed) {
                flush_pending(&mut records, &mut pending_path, &mut pending_revision);
                pending_path = Some(path);
                continue;
            }
            if pending_path.is_some()
                && pending_revision.is_none()
                && looks_like_cvs_revision_token(trimmed)
            {
                pending_revision = Some(trimmed.to_string());
                continue;
            }
            if let Some(path) = pending_path.take() {
                records.push(VcsRecord::LabeledFile {
                    label: pending_revision
                        .take()
                        .unwrap_or_else(|| "checkin".to_string()),
                    path,
                });
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
            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }
        flush_pending(&mut records, &mut pending_path, &mut pending_revision);
        to_doc_if_any(VcsTool::Cvs, VcsDocKind::Log, records)
    }
}

impl VcsParser for CvsTagParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut tag_name: Option<String> = None;
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(name) = parse_cvs_tag_name_from_command(trimmed) {
                tag_name = Some(name);
                continue;
            }
            if trimmed
                .to_ascii_lowercase()
                .starts_with("cvs tag: tagging ")
            {
                continue;
            }
            if let Some((status, path)) = parse_single_char_status_path(trimmed, &['T']) {
                let _ = status;
                let label = tag_name
                    .as_deref()
                    .map(|name| format!("tag({name})"))
                    .unwrap_or_else(|| "tag".to_string());
                records.push(VcsRecord::LabeledFile { label, path });
                continue;
            }
            if let Some(path) = parse_cvs_quoted_path(trimmed) {
                let label = tag_name
                    .as_deref()
                    .map(|name| format!("tag({name})"))
                    .unwrap_or_else(|| "tag".to_string());
                records.push(VcsRecord::LabeledFile { label, path });
                continue;
            }
            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }
        to_doc_if_any(VcsTool::Cvs, VcsDocKind::Log, records)
    }
}

impl VcsParser for CvsEditParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut fallback_path: Option<String> = None;
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(path) = trimmed.strip_prefix("cvs edit ") {
                let path = path.trim();
                if !path.is_empty() && looks_like_vcs_path(path) {
                    fallback_path = Some(path.to_string());
                }
                continue;
            }
            if let Some(path) = parse_cvs_quoted_path(trimmed) {
                let lower = trimmed.to_ascii_lowercase();
                let label = if let Some((_, user)) = lower.rsplit_once("edited by ") {
                    let original_start = trimmed.len() - user.len();
                    let user = trimmed[original_start..].trim();
                    if user.is_empty() {
                        "edited".to_string()
                    } else {
                        format!("edited({user})")
                    }
                } else {
                    "edited".to_string()
                };
                records.push(VcsRecord::LabeledFile { label, path });
                fallback_path = None;
                continue;
            }
            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }
        if records.is_empty() {
            if let Some(path) = fallback_path {
                records.push(VcsRecord::LabeledFile {
                    label: "edit".to_string(),
                    path,
                });
            }
        }
        to_doc_if_any(VcsTool::Cvs, VcsDocKind::Status, records)
    }
}
