#![allow(dead_code)]
//! Perforce (P4) 解析器 - 自包含，无外部依赖

// ============================================================================
// 核心类型定义
// ============================================================================

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum VcsTool {
    P4,
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
    Raw(String),
    Patch(String),
    Hunk(String),
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
            VcsRecord::Raw(r) => write!(f, "{}", r),
            VcsRecord::Patch(p) => write!(f, "{}", p),
            VcsRecord::Hunk(p) => write!(f, "{}", p),
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

// ============================================================================
// P4 解析器结构体定义
// ============================================================================

pub struct P4OpenedParser;
pub struct P4DescribeParser;
pub struct P4ChangesParser;
pub struct P4FstatParser;
pub struct P4WhereParser;
pub struct P4InfoParser;
pub struct P4LabelsParser;
pub struct P4DirsParser;
pub struct P4SyncParser;
pub struct P4SubmitParser;
pub struct P4ShelveParser;
pub struct P4UnshelveParser;
pub struct P4ResolveParser;
pub struct P4RevertParser;
pub struct P4EditParser;
pub struct P4AddParser;
pub struct P4DeleteParser;

// ============================================================================
// 通用辅助函数（内联自旧 helpers.rs，仅 P4 需要的）
// ============================================================================

/// 通用文档构造 — records 为空返回 None
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

/// 简单状态路径解析（如 "M path/to/file"）
fn parse_simple_status_path(line: &str) -> Option<(char, String)> {
    let token_end = line.find(char::is_whitespace).unwrap_or(line.len());
    if token_end == 0 {
        return None;
    }

    let status_token = &line[..token_end];
    let rest_trimmed = line[token_end..].trim_start();
    if rest_trimmed.is_empty() {
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

    Some((status, rest_trimmed.to_string()))
}

/// 通用 diff 补丁行解析
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
        if trimmed.starts_with("--- ") || trimmed.starts_with("+++ ") || trimmed.starts_with("*** ")
        {
            if let Some((path, _)) = trimmed.split_once('\t') {
                records.push(VcsRecord::Raw(path.to_string()));
                return true;
            }
        }
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

/// 判断是否像是 diff stat 行
fn looks_like_diff_stat_line(line: &str) -> bool {
    if line.contains(" | ") {
        return true;
    }
    line.ends_with("files changed")
        || line.ends_with("file changed")
        || line.contains("insertion(+)")
        || line.contains("insertions(+)")
        || line.contains("deletion(-)")
        || line.contains("deletions(-)")
}

/// 压缩 diff stat 行
fn compact_diff_stat_line(line: &str) -> String {
    let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some((path, rhs)) = normalized.split_once('|') {
        let path = path.trim();
        let rhs = rhs.trim();
        if !path.is_empty() && !rhs.is_empty() {
            let mut tokens = rhs.split_whitespace();
            let count = tokens.next().unwrap_or_default();
            if count.chars().all(|c| c.is_ascii_digit()) {
                return format!("{path}:{count}");
            }
        }
    }

    let lower = normalized.to_ascii_lowercase();
    if lower.contains("file changed") || lower.contains("files changed") {
        let files = normalized
            .split_whitespace()
            .next()
            .and_then(|v| v.parse::<usize>().ok());
        let parse_count_before = |needle: &str| -> Option<usize> {
            let idx = lower.find(needle)?;
            let head = &normalized[..idx];
            head.split_whitespace().last()?.parse::<usize>().ok()
        };
        let ins = parse_count_before(" insertion(+)")
            .or_else(|| parse_count_before(" insertions(+)"))
            .unwrap_or(0);
        let del = parse_count_before(" deletion(-)")
            .or_else(|| parse_count_before(" deletions(-)"))
            .unwrap_or(0);
        if let Some(files) = files {
            let noun = if files == 1 { "file" } else { "files" };
            return format!("{files} {noun} changed +{ins} -{del}");
        }
    }

    normalized
}

/// 分隔首个 token
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

/// 压缩行内空白
fn collapse_inline_whitespace(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 键值行解析
fn parse_key_value_line(line: &str) -> Option<(String, String)> {
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() || value.is_empty() {
        return None;
    }
    Some((key.to_string(), value.to_string()))
}

/// 压缩 info 时间戳（截取前 19 字符）
fn compact_info_timestamp(value: &str) -> String {
    if value.len() >= 19 {
        let candidate = &value[..19];
        if looks_like_compact_info_timestamp(candidate) {
            return candidate.to_string();
        }
    }
    value.trim().to_string()
}

fn looks_like_compact_info_timestamp(value: &str) -> bool {
    value.len() == 19
        && value.chars().enumerate().all(|(idx, ch)| match idx {
            4 | 7 => ch == '-' || ch == '/',
            10 => ch == ' ',
            13 | 16 => ch == ':',
            _ => ch.is_ascii_digit(),
        })
}

fn compact_info_value(key: &str, value: &str) -> String {
    if is_info_date_key(key) {
        compact_info_timestamp(value)
    } else if let Some(compact) = compact_size_metadata_value(key, value) {
        compact
    } else {
        value.trim().to_string()
    }
}

fn is_info_date_key(key: &str) -> bool {
    let lower = key.trim().to_ascii_lowercase();
    lower == "date"
        || lower.ends_with(" date")
        || lower.ends_with("_date")
        || lower.ends_with("-date")
        || lower.ends_with(" timestamp")
        || lower.ends_with("_timestamp")
        || lower.ends_with("-timestamp")
}

/// 压缩可读大小
fn compact_human_size_token(token: &str) -> Option<String> {
    let bytes = token.parse::<u64>().ok()?;
    if bytes < 1024 {
        return Some(token.to_string());
    }
    const UNITS: [&str; 4] = ["K", "M", "G", "T"];
    let mut value = bytes as f64 / 1024.0;
    let mut unit_idx = 0usize;
    while value >= 1024.0 && unit_idx < UNITS.len() - 1 {
        value /= 1024.0;
        unit_idx += 1;
    }
    if (value - value.floor()).abs() < 0.05 {
        Some(format!("{:.0}{}", value, UNITS[unit_idx]))
    } else {
        Some(format!("{:.1}{}", value, UNITS[unit_idx]))
    }
}

fn compact_size_metadata_value(key: &str, value: &str) -> Option<String> {
    let lower_key = key.trim().to_ascii_lowercase();
    let looks_like_size_key = lower_key.contains("size")
        || lower_key.ends_with("bytes")
        || lower_key == "bytes"
        || lower_key == "filesize";
    if !looks_like_size_key {
        return None;
    }

    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower_value = trimmed.to_ascii_lowercase();
    let bytes_token = if let Some(num) = lower_value.strip_suffix(" bytes") {
        num.trim()
    } else if let Some(num) = lower_value.strip_suffix(" byte") {
        num.trim()
    } else if let Some(num) = lower_value.strip_suffix('b') {
        let num = num.trim();
        if !num.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        num
    } else {
        trimmed
    };

    if !bytes_token.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    compact_human_size_token(bytes_token)
}

fn push_compact_raw_line(records: &mut Vec<VcsRecord>, parts: Vec<String>) {
    let compact: Vec<String> = parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect();
    if !compact.is_empty() {
        records.push(VcsRecord::Raw(compact.join(" | ")));
    }
}

/// 判断是否像 P4 时间 token
fn looks_like_p4_time_token(token: &str) -> bool {
    let trimmed = token.trim();
    if trimmed.len() < 4 || !trimmed.contains(':') {
        return false;
    }
    let mut digits = 0usize;
    for ch in trimmed.chars() {
        if ch.is_ascii_digit() {
            digits += 1;
            continue;
        }
        if matches!(ch, ':' | '.' | '+' | '-') {
            continue;
        }
        return false;
    }
    digits >= 4
}

/// 缩短 P4 info 键名
fn shorten_p4_info_key(key: &str) -> String {
    match key {
        "User name" => "User".to_string(),
        "Client name" => "Client".to_string(),
        "Client host" => "Host".to_string(),
        "Client root" => "Root".to_string(),
        "Current directory" => "Cwd".to_string(),
        "Server address" => "Server".to_string(),
        "Server root" => "SRoot".to_string(),
        "Server date" => "SDate".to_string(),
        "Server version" => "Ver".to_string(),
        "Case Handling" => "Case".to_string(),
        "Unicode enabled" => "Unicode".to_string(),
        _ => key.to_string(),
    }
}

// ============================================================================
// P4 专用解析辅助函数
// ============================================================================

/// 解析 P4 depot 路径（以 // 开头）
fn parse_p4_depot_path(line: &str) -> Option<String> {
    let start = line.find("//")?;
    let tail = &line[start..];
    let end = tail
        .find(|c: char| c.is_whitespace() || c == '#')
        .unwrap_or(tail.len());
    let path = tail[..end].trim();
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

/// 解析 P4 depot spec（仅 whitespace 分隔）
fn parse_p4_depot_spec(line: &str) -> Option<String> {
    let start = line.find("//")?;
    let tail = &line[start..];
    let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
    let spec = tail[..end].trim();
    if spec.is_empty() {
        None
    } else {
        Some(spec.to_string())
    }
}

/// 解析 p4 opened 行（status + path）
fn parse_p4_opened_line(line: &str) -> Option<(char, String)> {
    let path = parse_p4_depot_path(line)?;
    let lower = line.to_ascii_lowercase();
    let status = if lower.contains(" add") {
        'A'
    } else if lower.contains(" delete") {
        'D'
    } else {
        'M'
    };
    Some((status, path))
}

/// 通用 P4 操作路径行解析（"path - action" 格式）
fn parse_p4_action_path_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    let (left, right) = trimmed.split_once(" - ")?;
    let path = parse_p4_depot_spec(left)
        .or_else(|| parse_p4_depot_spec(trimmed))
        .unwrap_or_else(|| left.trim().to_string());
    if path.is_empty() {
        return None;
    }
    Some((path, right.trim().to_string()))
}

/// 解析 p4 sync 行
fn parse_p4_sync_line(line: &str) -> Option<(char, String)> {
    let (path, action) = parse_p4_action_path_line(line)?;
    let lower = action.to_ascii_lowercase();
    let status = if lower.starts_with("updated")
        || lower.starts_with("updating ")
        || lower.starts_with("refresh")
    {
        'U'
    } else if lower == "added" || lower.starts_with("added as ") {
        'A'
    } else if lower == "deleted" || lower.starts_with("deleted as ") {
        'D'
    } else {
        return None;
    };
    Some((status, path))
}

/// 压缩 p4 sync 预览摘要
fn compact_p4_sync_preview_summary(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    if !(lower.ends_with(" files would be updated.") || lower.ends_with(" file would be updated."))
    {
        return None;
    }
    let count = line.split_whitespace().next()?;
    if !count.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("{count} would-update"))
}

/// 解析 P4 change 编号
fn parse_p4_change_number(line: &str) -> Option<String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let idx = tokens
        .iter()
        .position(|token| token.eq_ignore_ascii_case("change"))?;
    let change = tokens
        .get(idx + 1)?
        .trim_end_matches(|ch| ch == '.' || ch == ':');
    if change.chars().all(|ch| ch.is_ascii_digit()) {
        Some(change.to_string())
    } else {
        None
    }
}

/// 压缩 P4 files 摘要
fn compact_p4_files_summary(line: &str, action: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let needle = format!(" files {}", action);
    if !lower.ends_with(&needle) && !lower.ends_with(&format!(" files {}.", action)) {
        return None;
    }
    let count = line.split_whitespace().next()?;
    if !count.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    Some(format!("{} {}", count, action))
}

/// 解析 p4 resolve 行
fn parse_p4_resolve_line(line: &str) -> Option<(String, String)> {
    let (path, action) = parse_p4_action_path_line(line)?;
    let lower = action.to_ascii_lowercase();
    if lower.starts_with("resolved using ") {
        let label = action
            .strip_prefix("resolved using ")
            .unwrap_or(action.as_str())
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        let label = if label.is_empty() {
            "resolve".to_string()
        } else {
            label
        };
        return Some((label, path));
    }
    if lower.starts_with("skipped") {
        return Some(("skip".to_string(), path));
    }
    None
}

/// 压缩 p4 resolve 摘要
fn compact_p4_resolve_summary(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    if !lower.contains("resolved") || !lower.contains("conflict") {
        return None;
    }
    let compact = line
        .trim_end_matches('.')
        .replace(" files ", " ")
        .replace(" conflict remaining", " conflict")
        .replace(" conflicts remaining", " conflicts");
    Some(compact)
}

/// 解析 p4 revert 行
fn parse_p4_revert_line(line: &str) -> Option<String> {
    let (path, action) = parse_p4_action_path_line(line)?;
    if action.eq_ignore_ascii_case("reverted") {
        Some(path)
    } else {
        None
    }
}

/// 解析 p4 edit 行
fn parse_p4_edit_line(line: &str) -> Option<String> {
    let (path, action) = parse_p4_action_path_line(line)?;
    if action.eq_ignore_ascii_case("opened for edit") {
        Some(path)
    } else {
        None
    }
}

/// 解析 p4 add 行
fn parse_p4_add_line(line: &str) -> Option<String> {
    let (path, action) = parse_p4_action_path_line(line)?;
    if action.eq_ignore_ascii_case("added for add") {
        Some(path)
    } else {
        None
    }
}

/// 解析 p4 delete 行
fn parse_p4_delete_line(line: &str) -> Option<String> {
    let (path, action) = parse_p4_action_path_line(line)?;
    if action.eq_ignore_ascii_case("deleted for delete") {
        Some(path)
    } else {
        None
    }
}

/// 解析 p4 fstat 元数据行（... key value 格式）
fn parse_p4_metadata_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("... ")?;
    let (key, value) = split_first_token(rest)?;
    Some((key.to_string(), value.to_string()))
}

/// 判断是否为 P4 路径元数据键
fn is_p4_path_metadata_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.ends_with("file") || lower.ends_with("path") || lower == "path"
}

/// 判断是否为 P4 路径值
fn is_p4_path_value(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    trimmed.starts_with("//")
        || trimmed.starts_with("\\\\")
        || trimmed.starts_with('/')
        || (trimmed.len() >= 3
            && trimmed.as_bytes()[1] == b':'
            && trimmed.as_bytes()[0].is_ascii_alphabetic())
}

/// 解析 p4 where 行（depot client local）
fn parse_p4_where_line(line: &str) -> Option<(String, String, String)> {
    let (depot, rest) = split_first_token(line)?;
    let (client, local) = split_first_token(rest)?;
    if !depot.starts_with("//") || !client.starts_with("//") || local.is_empty() {
        return None;
    }
    Some((depot.to_string(), client.to_string(), local.to_string()))
}

/// 解析 p4 labels 行
fn parse_p4_label_line(line: &str) -> Option<String> {
    let rest = line.trim();
    if !rest.starts_with("Label ") {
        return None;
    }
    let rest = rest.strip_prefix("Label ").unwrap_or(rest).trim();
    let (metadata, description) = if let Some(start) = rest.find('\'') {
        let end = rest.rfind('\'')?;
        (
            rest[..start].trim(),
            Some(rest[start + 1..end].trim().to_string()),
        )
    } else {
        (rest, None)
    };

    let mut parts = metadata.split_whitespace();
    let name = parts.next()?.trim();
    let date = parts.next()?.trim();
    let mut compact = format!("Label {} {}", name, date);

    if let Some(time) = parts.next().filter(|token| looks_like_p4_time_token(token)) {
        compact.push(' ');
        compact.push_str(time.trim());
    }

    if let Some(owner) = parts.next().filter(|token| !token.trim().is_empty()) {
        compact.push(' ');
        compact.push_str(owner.trim());
    }

    if let Some(description) = description.filter(|text| !text.trim().is_empty()) {
        compact.push(' ');
        compact.push_str(description.trim());
    }

    Some(compact)
}

// ============================================================================
// P4 解析器实现
// ============================================================================

impl VcsParser for P4OpenedParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("p4 opened") {
                continue;
            }
            if let Some((status, path)) = parse_p4_opened_line(trimmed) {
                records.push(VcsRecord::File {
                    status: Some(status),
                    path,
                });
                continue;
            }
            records.push(VcsRecord::Raw(trimmed.to_string()));
        }
        to_doc_if_any(VcsTool::P4, VcsDocKind::Status, records)
    }
}

impl VcsParser for P4DescribeParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let line = line.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("Change ") {
                let id = rest.split_whitespace().next().unwrap_or_default();
                if !id.is_empty() {
                    records.push(VcsRecord::Commit(id.to_string()));
                    records.push(VcsRecord::Raw(trimmed.to_string()));
                    continue;
                }
            }
            if let Some(path) = parse_p4_depot_path(trimmed) {
                records.push(VcsRecord::File { status: None, path });
                continue;
            }
            if parse_generic_patch_or_stat_line(line, trimmed, &mut records) {
                continue;
            }
            records.push(VcsRecord::Raw(trimmed.to_string()));
        }
        to_doc_if_any(VcsTool::P4, VcsDocKind::Diff, records)
    }
}

impl VcsParser for P4ChangesParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut pending_header: Option<String> = None;

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("p4 changes") {
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("Change ") {
                if let Some(header) = pending_header.take() {
                    records.push(VcsRecord::Raw(header));
                }

                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.len() >= 5 && parts[1] == "on" && parts[3] == "by" {
                    let id = parts[0];
                    let date = parts[2].replace('/', "");
                    let author = parts[4].split('@').next().unwrap_or(parts[4]);

                    let status_part = parts.get(5).unwrap_or(&"").trim_matches('*');
                    let status_str = if status_part.is_empty() {
                        "".to_string()
                    } else {
                        format!(" {}", status_part)
                    };

                    pending_header = Some(format!("{} {} {}{}", id, date, author, status_str));
                } else {
                    records.push(VcsRecord::Raw(trimmed.to_string()));
                }
                continue;
            }

            if let Some(header) = pending_header.take() {
                records.push(VcsRecord::Raw(format!("{} {}", header, trimmed)));
            } else {
                records.push(VcsRecord::Raw(trimmed.to_string()));
            }
        }

        if let Some(header) = pending_header.take() {
            records.push(VcsRecord::Raw(header));
        }

        to_doc_if_any(VcsTool::P4, VcsDocKind::Log, records)
    }
}

impl VcsParser for P4FstatParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.to_ascii_lowercase().starts_with("p4 fstat") {
                continue;
            }

            if let Some((key, value)) = parse_p4_metadata_line(trimmed) {
                if is_p4_path_metadata_key(&key) && is_p4_path_value(&value) {
                    records.push(VcsRecord::LabeledFile {
                        label: key,
                        path: value,
                    });
                } else if value.is_empty() {
                    records.push(VcsRecord::Raw(key));
                } else {
                    let value = compact_size_metadata_value(&key, &value).unwrap_or(value);
                    records.push(VcsRecord::Raw(format!("{}: {}", key, value)));
                }
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::P4, VcsDocKind::Show, records)
    }
}

impl VcsParser for P4WhereParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.to_ascii_lowercase().starts_with("p4 where") {
                continue;
            }

            if let Some((depot, client, local)) = parse_p4_where_line(trimmed) {
                records.push(VcsRecord::LabeledFile {
                    label: "depot".to_string(),
                    path: depot,
                });
                records.push(VcsRecord::LabeledFile {
                    label: "client".to_string(),
                    path: client,
                });
                records.push(VcsRecord::LabeledFile {
                    label: "local".to_string(),
                    path: local,
                });
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::P4, VcsDocKind::Show, records)
    }
}

impl VcsParser for P4InfoParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let records = compact_p4_info_records(raw);
        to_doc_if_any(VcsTool::P4, VcsDocKind::Show, records)
    }
}

impl VcsParser for P4LabelsParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.to_ascii_lowercase().starts_with("p4 labels") {
                continue;
            }

            if let Some(compact) = parse_p4_label_line(trimmed) {
                records.push(VcsRecord::Raw(compact));
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::P4, VcsDocKind::Show, records)
    }
}

impl VcsParser for P4DirsParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.to_ascii_lowercase().starts_with("p4 dirs") {
                continue;
            }

            if trimmed.starts_with("//") {
                records.push(VcsRecord::File {
                    status: None,
                    path: trimmed.to_string(),
                });
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::P4, VcsDocKind::Show, records)
    }
}

impl VcsParser for P4SyncParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut emitted_sync = false;

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("p4 sync") {
                continue;
            }

            if trimmed.eq_ignore_ascii_case("Sync completed.") {
                if !emitted_sync {
                    records.push(VcsRecord::Raw("sync".to_string()));
                    emitted_sync = true;
                }
                continue;
            }

            if let Some((status, path)) = parse_p4_sync_line(trimmed) {
                records.push(VcsRecord::File {
                    status: Some(status),
                    path,
                });
                continue;
            }

            if let Some(summary) = compact_p4_sync_preview_summary(trimmed) {
                records.push(VcsRecord::Raw(summary));
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::P4, VcsDocKind::Status, records)
    }
}

impl VcsParser for P4SubmitParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut last_change: Option<String> = None;

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("p4 submit") {
                continue;
            }

            if let Some(change) = parse_p4_change_number(trimmed) {
                if last_change.as_deref() != Some(change.as_str()) {
                    records.push(VcsRecord::Commit(change.clone()));
                    last_change = Some(change);
                }
                continue;
            }

            if let Some(path) = parse_p4_depot_spec(trimmed) {
                records.push(VcsRecord::File { status: None, path });
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::P4, VcsDocKind::Log, records)
    }
}

impl VcsParser for P4ShelveParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut last_change: Option<String> = None;

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("p4 shelve") {
                continue;
            }

            if let Some(change) = parse_p4_change_number(trimmed) {
                if last_change.as_deref() != Some(change.as_str()) {
                    records.push(VcsRecord::Commit(change.clone()));
                    last_change = Some(change);
                }
                continue;
            }

            if let Some(path) = parse_p4_depot_spec(trimmed) {
                records.push(VcsRecord::File { status: None, path });
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::P4, VcsDocKind::Log, records)
    }
}

impl VcsParser for P4UnshelveParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut last_change: Option<String> = None;

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("p4 unshelve") {
                continue;
            }

            if let Some(change) = parse_p4_change_number(trimmed) {
                if last_change.as_deref() != Some(change.as_str()) {
                    records.push(VcsRecord::Commit(change.clone()));
                    last_change = Some(change);
                }
                continue;
            }

            if let Some(path) = parse_p4_depot_spec(trimmed) {
                records.push(VcsRecord::File { status: None, path });
                continue;
            }

            if let Some(summary) = compact_p4_files_summary(trimmed, "restored") {
                records.push(VcsRecord::Raw(summary));
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::P4, VcsDocKind::Log, records)
    }
}

impl VcsParser for P4ResolveParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("p4 resolve") {
                continue;
            }

            if let Some((label, path)) = parse_p4_resolve_line(trimmed) {
                records.push(VcsRecord::LabeledFile { label, path });
                continue;
            }

            if let Some(summary) = compact_p4_resolve_summary(trimmed) {
                records.push(VcsRecord::Raw(summary));
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::P4, VcsDocKind::Status, records)
    }
}

impl VcsParser for P4RevertParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("p4 revert") {
                continue;
            }

            if let Some(path) = parse_p4_revert_line(trimmed) {
                records.push(VcsRecord::File {
                    status: Some('R'),
                    path,
                });
                continue;
            }

            if let Some(summary) = compact_p4_files_summary(trimmed, "reverted") {
                records.push(VcsRecord::Raw(summary));
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::P4, VcsDocKind::Status, records)
    }
}

impl VcsParser for P4EditParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("p4 edit") {
                continue;
            }

            if let Some(path) = parse_p4_edit_line(trimmed) {
                records.push(VcsRecord::File {
                    status: Some('M'),
                    path,
                });
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::P4, VcsDocKind::Status, records)
    }
}

impl VcsParser for P4AddParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("p4 add") {
                continue;
            }

            if let Some(path) = parse_p4_add_line(trimmed) {
                records.push(VcsRecord::File {
                    status: Some('A'),
                    path,
                });
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::P4, VcsDocKind::Status, records)
    }
}

impl VcsParser for P4DeleteParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("p4 delete") {
                continue;
            }

            if let Some(path) = parse_p4_delete_line(trimmed) {
                records.push(VcsRecord::File {
                    status: Some('D'),
                    path,
                });
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::P4, VcsDocKind::Status, records)
    }
}

// ============================================================================
// P4 Info 紧凑记录构建（内联自 helpers.rs）
// ============================================================================

fn compact_p4_info_records(raw: &str) -> Vec<VcsRecord> {
    let mut user = None;
    let mut client = None;
    let mut host = None;
    let mut root = None;
    let mut cwd = None;
    let mut server = None;
    let mut server_root = None;
    let mut server_date = None;
    let mut version = None;
    let mut case_handling = None;
    let mut unicode = None;
    let mut extras = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() || trimmed.to_ascii_lowercase().starts_with("p4 info") {
            continue;
        }

        if let Some((key, value)) = parse_key_value_line(trimmed) {
            match key.as_str() {
                "User name" => user = Some(value),
                "Client name" => client = Some(value),
                "Client host" => host = Some(value),
                "Client root" => root = Some(value),
                "Current directory" => cwd = Some(value),
                "Client address" | "Server license" | "Server uptime" | "Client unknown" => {}
                "Server address" => server = Some(value),
                "Server root" => server_root = Some(value),
                "Server date" => server_date = Some(compact_info_timestamp(&value)),
                "Server version" => version = Some(value),
                "Case Handling" => case_handling = Some(value),
                "Unicode enabled" => unicode = Some(value),
                _ => extras.push(format!(
                    "{}: {}",
                    shorten_p4_info_key(&key),
                    compact_info_value(&key, &value)
                )),
            }
            continue;
        }

        extras.push(trimmed.to_string());
    }

    let mut records = Vec::new();

    let mut identity_parts = Vec::new();
    if let Some(user) = user {
        identity_parts.push(format!("User: {}", user));
    }
    if let Some(client) = client {
        identity_parts.push(format!("Client: {}", client));
    }
    if let Some(host) = host {
        identity_parts.push(format!("Host: {}", host));
    }
    push_compact_raw_line(&mut records, identity_parts);

    if let Some(path) = root {
        records.push(VcsRecord::LabeledFile {
            label: "Root".to_string(),
            path,
        });
    }
    if let Some(path) = cwd {
        records.push(VcsRecord::LabeledFile {
            label: "Cwd".to_string(),
            path,
        });
    }

    let mut server_parts = Vec::new();
    if let Some(server) = server {
        server_parts.push(format!("Server: {}", server));
    }
    if let Some(server_root) = server_root {
        server_parts.push(format!("SRoot: {}", server_root));
    }
    push_compact_raw_line(&mut records, server_parts);

    let mut meta_parts = Vec::new();
    if let Some(server_date) = server_date {
        meta_parts.push(format!("SDate: {}", server_date));
    }
    if let Some(version) = version {
        meta_parts.push(format!("Ver: {}", version));
    }
    if let Some(case_handling) = case_handling {
        meta_parts.push(format!("Case: {}", case_handling));
    }
    if let Some(unicode) = unicode {
        meta_parts.push(format!("Unicode: {}", unicode));
    }
    push_compact_raw_line(&mut records, meta_parts);
    push_compact_raw_line(&mut records, extras);

    records
}
