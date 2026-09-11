#![allow(dead_code)]
//! Bazaar (Bzr) 解析器 - 自包含

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum VcsTool {
    Bzr,
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
    /// 将 VcsRecord 格式化为可读字符串（含各变体的显示格式）。
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
    /// VcsParser 解析入口：子类实现具体解析逻辑。
    fn parse(&self, raw: &str) -> Option<VcsDocument>;
}
pub struct BzrStatusParser;
pub struct BzrDiffParser;
pub struct BzrLogParser;
pub struct BzrPullParser;
pub struct BzrPushParser;
pub struct BzrMergeParser;
pub struct BzrResolveParser;
pub struct BzrBranchParser;

// --- inlined helpers ---
/// 非空记录列表包装为 VcsDocument；空列表返回 None。
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
/// 解析单字符状态码 + 路径（支持两字符组合状态，R/C 后随数字）。
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
/// 判断字符串是否为 VCS 路径形态（过滤邮箱/URL/代码调用/方法名）。
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
    // 过滤方法名模式
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
/// 折叠行内连续空白为单空格。
#[tracing::instrument(level = "debug", skip_all)]
fn collapse_inline_whitespace(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}
/// 按长词状态前缀（modified:/added: 等）解析状态码与路径。
#[tracing::instrument(level = "debug", skip_all)]
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
/// 解析 diff 的 index/---/+++/@@ 行与 +/- 补丁行，分类为 Raw/Hunk/Patch 记录。
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
/// 判断是否为 diff stat 行（含 " | " 分隔）。
#[tracing::instrument(level = "debug", skip_all)]
fn looks_like_diff_stat_line(line: &str) -> bool {
    line.contains(" | ")
}
/// 紧凑化 diff stat 行：折叠连续空白。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_diff_stat_line(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 通用 status 解析：按命令前缀过滤，解析状态行与路径为 File 记录。
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
/// 通用 log 解析：按命令前缀过滤，解析 revno/author/date/缩进消息为记录。
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
        if let Some(rest) = trimmed.strip_prefix("revno:") {
            let id = rest.trim();
            if !id.is_empty() {
                records.push(VcsRecord::Commit(id.to_string()));
                continue;
            }
        }
        if lower.starts_with("author:") || lower.starts_with("committer:") {
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
/// 通用 diff 解析：降维 bzr diff 头，解析补丁/路径为记录。
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
        // Bzr diff 头降维: "=== modified file 'path'" → "DIFF://path"
        if let Some(inner) = trimmed.strip_prefix("=== modified file '") {
            if let Some(path) = inner.strip_suffix('\'') {
                if looks_like_vcs_path(path) {
                    records.push(VcsRecord::Raw(format!("DIFF://{}", path)));
                    continue;
                }
            }
        }
        let lower = trimmed.to_ascii_lowercase();
        if command_prefixes
            .iter()
            .any(|prefix| lower.starts_with(prefix))
        {
            continue;
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

/// 从行中解析修订计数（形如 "revno: N" 前缀）。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_bzr_revision_count_line(line: &str, prefix: &str) -> Option<usize> {
    let rest = line.strip_prefix(prefix)?.trim();
    let count = rest.split_whitespace().next()?.parse::<usize>().ok()?;
    Some(count)
}
/// 从行中解析总修订数（形如 "Total N" 前缀）。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_bzr_total_revisions_line(line: &str) -> Option<usize> {
    let rest = line.strip_prefix("Total ")?.trim();
    let count = rest.split_whitespace().next()?.parse::<usize>().ok()?;
    Some(count)
}

// --- Parsers ---
impl VcsParser for BzrStatusParser {
    /// 解析 `bzr status` 输出：委托通用状态解析器，将文件状态行归并为 Status 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        parse_generic_status_for_tool(raw, VcsTool::Bzr, &["bzr status", "bzr st"])
    }
}
impl VcsParser for BzrDiffParser {
    /// 解析 `bzr diff` 输出：委托通用差异解析器，将变更片段归并为 Diff 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        parse_generic_diff_for_tool(raw, VcsTool::Bzr, &["bzr diff"])
    }
}
impl VcsParser for BzrLogParser {
    /// 解析 `bzr log` 输出：委托通用日志解析器，将版本历史归并为 Log 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        parse_generic_log_for_tool(raw, VcsTool::Bzr, &["bzr log"])
    }
}

impl VcsParser for BzrPullParser {
    /// 解析 `bzr pull` 输出：提取 `Pulled N` 与 `Total revisions` 计数摘要并保留其余行，输出 Log 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut pulled = None;
        let mut total = None;
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("bzr pull") {
                continue;
            }
            if let Some(count) = parse_bzr_revision_count_line(trimmed, "Pulled ") {
                pulled = Some(count);
                continue;
            }
            if let Some(count) = parse_bzr_total_revisions_line(trimmed) {
                total = Some(count);
                continue;
            }
            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }
        if pulled.is_some() || total.is_some() {
            let summary = match (pulled, total) {
                (Some(p), Some(t)) => format!("pull {p}/{t} revs"),
                (Some(p), None) => format!("pull {p} revs"),
                (None, Some(t)) => format!("total {t} revs"),
                (None, None) => String::new(),
            };
            if !summary.is_empty() {
                records.insert(0, VcsRecord::Raw(summary));
            }
        }
        to_doc_if_any(VcsTool::Bzr, VcsDocKind::Log, records)
    }
}

impl VcsParser for BzrPushParser {
    /// 解析 `bzr push` 输出：识别「无远端分支」情形与 `bzr push <target>` 建议目标，输出 Log 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut saw_no_remote = false;
        let mut suggestion: Option<String> = None;
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("bzr push") {
                continue;
            }
            if trimmed.eq_ignore_ascii_case("No remote branch to push to.") {
                saw_no_remote = true;
                continue;
            }
            if trimmed.eq_ignore_ascii_case("To push to a branch, use:") {
                continue;
            }
            if let Some(target) = trimmed.strip_prefix("bzr push ") {
                if !target.trim().is_empty() {
                    suggestion = Some(target.trim().to_string());
                }
                continue;
            }
            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }
        if saw_no_remote {
            if let Some(target) = suggestion {
                records.insert(0, VcsRecord::Raw(format!("push: no remote -> {target}")));
            } else {
                records.insert(0, VcsRecord::Raw("push: no remote".to_string()));
            }
        } else if let Some(target) = suggestion {
            records.insert(0, VcsRecord::Raw(format!("push {target}")));
        }
        to_doc_if_any(VcsTool::Bzr, VcsDocKind::Log, records)
    }
}

impl VcsParser for BzrMergeParser {
    /// 解析 `bzr merge` 输出：将 `Merging from` 来源与变更路径分别归为记录，输出 Log 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("bzr merge") {
                continue;
            }
            if let Some(source) = trimmed.strip_prefix("Merging from: ") {
                if !source.trim().is_empty() {
                    records.push(VcsRecord::Raw(format!("merge {source}")));
                }
                continue;
            }
            if trimmed.eq_ignore_ascii_case("All changes applied successfully.") {
                records.push(VcsRecord::Raw("applied".to_string()));
                continue;
            }
            if looks_like_vcs_path(trimmed) {
                records.push(VcsRecord::File {
                    status: None,
                    path: trimmed.to_string(),
                });
                continue;
            }
            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }
        to_doc_if_any(VcsTool::Bzr, VcsDocKind::Log, records)
    }
}

impl VcsParser for BzrResolveParser {
    /// 解析 `bzr resolve` 输出：将 `Resolved conflicts in` 块内路径归为 resolved 标签文件，输出 Status 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut in_resolved_block = false;
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("bzr resolve") {
                continue;
            }
            if trimmed.eq_ignore_ascii_case("Resolved conflicts in:") {
                in_resolved_block = true;
                continue;
            }
            if in_resolved_block && looks_like_vcs_path(trimmed) {
                records.push(VcsRecord::LabeledFile {
                    label: "resolved".to_string(),
                    path: trimmed.to_string(),
                });
                continue;
            }
            in_resolved_block = false;
            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }
        to_doc_if_any(VcsTool::Bzr, VcsDocKind::Status, records)
    }
}

impl VcsParser for BzrBranchParser {
    /// 解析 `bzr branch` 输出：提取分支目标与 `Branched N revs` 修订数摘要，输出 Log 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut target: Option<String> = None;
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(path) = trimmed.strip_prefix("bzr branch ") {
                if !path.trim().is_empty() {
                    target = Some(path.trim().to_string());
                }
                continue;
            }
            if let Some(count) = parse_bzr_revision_count_line(trimmed, "Branched ") {
                records.push(VcsRecord::Raw(format!("{count} revs")));
                continue;
            }
            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }
        if let Some(target) = target {
            records.insert(0, VcsRecord::Raw(format!("branch {target}")));
        }
        to_doc_if_any(VcsTool::Bzr, VcsDocKind::Log, records)
    }
}
