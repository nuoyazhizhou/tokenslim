// --- IR 通用定义 (内联隔离) ---
#[derive(Debug, PartialEq, Eq)]
pub enum VcsTool {
    Hg,
}

#[derive(Debug, PartialEq, Eq)]
pub enum VcsDocKind {
    Status,
    Log,
    Diff,
    Show,
}

#[derive(Debug, PartialEq, Eq)]
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
    Hunk(String),
    Patch(String),
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
            VcsRecord::LabeledFile { label, path } => write!(f, "[{}] {}", label, path),
            VcsRecord::DiffFile { left, right } => write!(f, "--- {}\n+++ {}", left, right),
            VcsRecord::Subject(s) => write!(f, "CM:{}", s),
            VcsRecord::Author(a) => write!(f, "OW:{}", a),
            VcsRecord::Date(d) => write!(f, "DT:{}", d),
            VcsRecord::Commit(c) => write!(f, "CH:{}", c),
            VcsRecord::Stat(s) => write!(f, "{}", s),
            VcsRecord::Hunk(h) => write!(f, "{}", h),
            VcsRecord::Patch(p) => write!(f, "{}", p),
            VcsRecord::Raw(r) => write!(f, "{}", r),
        }
    }
}

#[derive(Debug)]
pub struct VcsDocument {
    pub tool: VcsTool,
    pub kind: VcsDocKind,
    pub records: Vec<VcsRecord>,
}

pub trait VcsParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument>;
}

// --- 公共帮助函数 (独立化) ---
#[tracing::instrument(level = "debug", skip_all)]
pub fn to_doc_if_any(
    tool: VcsTool,
    kind: VcsDocKind,
    records: Vec<VcsRecord>,
) -> Option<VcsDocument> {
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
pub fn extract_hg_author(raw_user: &str) -> String {
    // 提取邮箱 @ 前的完整前缀，丢弃域名后缀
    // 例: "alice.chen@domain.com" → "@alice.chen"
    // 例: "developer <dev@example.com>" → "@dev"
    // 例: "alice.chen" (无邮箱) → 保持原样

    // 优先处理尖括号中的邮箱: "Name <local@domain>"
    if let Some(email_start) = raw_user.find('<') {
        if let Some(email_end) = raw_user.find('>') {
            if email_end > email_start + 1 {
                let email = &raw_user[email_start + 1..email_end];
                if let Some(local) = email.split('@').next() {
                    if !local.is_empty() {
                        return format!("@{}", local);
                    }
                }
            }
        }
    }

    // 直接邮箱格式: "local@domain" (非 HTML 格式)
    if raw_user.contains('@') {
        if let Some(local) = raw_user.split('@').next() {
            let local = local.trim();
            if !local.is_empty() {
                return format!("@{}", local);
            }
        }
    }

    // 无邮箱，直接使用原文
    raw_user.trim().to_string()
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn looks_like_vcs_path(path: &str) -> bool {
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
pub fn parse_simple_status_path(line: &str) -> Option<(char, String)> {
    let t = line.trim();
    let bytes = t.as_bytes();
    // Hg 状态码为 1-2 字符，后跟空格和路径
    // 如 "M src/main.rs" (1-char), "! docs/design.md" (1-char)
    // 兼容 "MM src/main.rs" (2-char) 场景
    if t.len() > 3 {
        // 2-char 状态码: bytes[2] == b' '
        if bytes[2] == b' ' {
            let status = bytes[0] as char;
            let path = t[3..].trim().to_string();
            if looks_like_vcs_path(&path) {
                return Some((status, path));
            }
        }
        // 1-char 状态码: bytes[1] == b' '
        if bytes[1] == b' ' {
            let status = bytes[0] as char;
            let path = t[2..].trim().to_string();
            if looks_like_vcs_path(&path) {
                return Some((status, path));
            }
        }
    }
    None
}

// --- Parse & format helpers ---

#[tracing::instrument(level = "debug", skip_all)]
pub fn parse_generic_patch_or_stat_line(
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

    if trimmed.contains(" | ") && (trimmed.contains('+') || trimmed.contains('-')) {
        records.push(VcsRecord::Stat(trimmed.to_string()));
        return true;
    }

    false
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_update_summary_line(line: &str) -> Option<String> {
    let mut compact = Vec::new();

    for segment in line.split(',') {
        let trimmed = segment.trim().trim_end_matches('.');
        if trimmed.is_empty() {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        let count = parts.next()?.parse::<usize>().ok()?;
        let noun = parts.next()?;
        let action = parts.next()?;
        if parts.next().is_some() {
            return None;
        }

        if !matches!(noun, "file" | "files") {
            return None;
        }

        if !matches!(action, "updated" | "merged" | "removed" | "unresolved") {
            return None;
        }

        compact.push(format!("{count} {action}"));
    }

    if compact.is_empty() {
        None
    } else {
        Some(compact.join(", "))
    }
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn strip_hg_field_prefix<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    line.strip_prefix(prefix).map(|value| value.trim_start())
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn collapse_inline_whitespace(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_branch_line(line: &str) -> String {
    line.strip_suffix(" (inactive)")
        .map(|prefix| format!("{prefix}~"))
        .unwrap_or_else(|| line.to_string())
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn split_on_repeated_whitespace(line: &str) -> Option<(&str, &str)> {
    let mut run_start: Option<usize> = None;
    let mut run_len = 0usize;

    for (idx, ch) in line.char_indices() {
        if ch.is_whitespace() {
            if run_len == 0 {
                run_start = Some(idx);
            }
            run_len += 1;
            if run_len >= 2 {
                let start = run_start?;
                let end = idx + ch.len_utf8();
                let left = line[..start].trim_end();
                let right = line[end..].trim_start();
                if !left.is_empty() && !right.is_empty() {
                    return Some((left, right));
                }
                return None;
            }
        } else {
            run_start = None;
            run_len = 0;
        }
    }
    None
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn hg_month_number(token: &str) -> Option<u32> {
    match token {
        "Jan" => Some(1),
        "Feb" => Some(2),
        "Mar" => Some(3),
        "Apr" => Some(4),
        "May" => Some(5),
        "Jun" => Some(6),
        "Jul" => Some(7),
        "Aug" => Some(8),
        "Sep" => Some(9),
        "Oct" => Some(10),
        "Nov" => Some(11),
        "Dec" => Some(12),
        _ => None,
    }
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn looks_like_hg_weekday(token: &str) -> bool {
    matches!(token, "Mon" | "Tue" | "Wed" | "Thu" | "Fri" | "Sat" | "Sun")
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn looks_like_hg_year(token: &str) -> bool {
    token.len() == 4 && token.chars().all(|ch| ch.is_ascii_digit())
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn looks_like_hg_time_token(token: &str) -> bool {
    let mut parts = token.split(':');
    let hour = parts.next().unwrap_or_default();
    let minute = parts.next().unwrap_or_default();
    let second = parts.next().unwrap_or_default();

    parts.next().is_none()
        && hour.len() == 2
        && minute.len() == 2
        && second.len() == 2
        && hour.chars().all(|ch| ch.is_ascii_digit())
        && minute.chars().all(|ch| ch.is_ascii_digit())
        && second.chars().all(|ch| ch.is_ascii_digit())
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_date_value(value: &str) -> Option<String> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }

    let (month_idx, day_idx, time_idx, year_idx) = match parts.as_slice() {
        [weekday, _, _, _, _, ..] if looks_like_hg_weekday(weekday) => (1, 2, 3, 4),
        [_, _, _, _, ..] => (0, 1, 2, 3),
        _ => return None,
    };

    let month = hg_month_number(parts[month_idx])?;
    let day = parts[day_idx].parse::<u32>().ok()?;
    let time = parts[time_idx];
    let year = parts[year_idx];

    if !looks_like_hg_time_token(time) || !looks_like_hg_year(year) {
        return None;
    }

    Some(format!("{}-{:02}-{:02} {}", year, month, day, time))
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn enforce_hg_changeset_boundaries(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 16);
    let mut i = 0usize;

    while let Some(rel) = input[i..].find("changeset:") {
        let idx = i + rel;
        out.push_str(&input[i..idx]);

        let needs_newline = idx > 0 && !matches!(input.as_bytes()[idx - 1], b'\n' | b'\r');
        let id = input[idx + "changeset:".len()..]
            .split_whitespace()
            .next()
            .unwrap_or_default();
        let looks_changeset_id = id.split_once(':').is_some_and(|(rev, hash)| {
            !rev.is_empty()
                && rev.chars().all(|ch| ch.is_ascii_digit())
                && !hash.is_empty()
                && hash.chars().all(|ch| ch.is_ascii_hexdigit())
        });

        if needs_newline && looks_changeset_id {
            out.push('\n');
        }

        out.push_str("changeset:");
        i = idx + "changeset:".len();
    }

    out.push_str(&input[i..]);
    out
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn parse_hg_changeset_like_records(raw: &str, command_prefixes: &[&str]) -> Vec<VcsRecord> {
    // 法则 D + HG-2: 将松散的多行 changeset 拍扁为单行 IR
    // 目标: CH:<id> BR:<branch> OW:<author> DT:<date> CM:<message>
    let mut records = Vec::new();
    let normalized = enforce_hg_changeset_boundaries(raw);

    // 收集当前 changeset 的各字段 parts，遇到新 changeset 时 flush
    let mut current_parts: Vec<String> = Vec::new();
    let mut in_changeset = false;

    // 辅助: flush 当前累积的 changeset 为单条 VcsRecord::Raw
    let flush_current = |parts: &mut Vec<String>, records: &mut Vec<VcsRecord>| {
        if !parts.is_empty() {
            records.push(VcsRecord::Raw(parts.join(" ")));
            parts.clear();
        }
    };

    for line in normalized.lines() {
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

        // 遇到新的 changeset: → flush 上一个 changeset，开始新的
        if let Some(changeset) = strip_hg_field_prefix(trimmed, "changeset:") {
            flush_current(&mut current_parts, &mut records);
            current_parts.push(format!("CH:{}", changeset));
            in_changeset = true;
            continue;
        }

        // changeset 内的各字段
        if in_changeset {
            if let Some(branch) = strip_hg_field_prefix(trimmed, "branch:") {
                current_parts.push(format!("BR:{}", branch));
                continue;
            }

            if let Some(user) = strip_hg_field_prefix(trimmed, "user:") {
                current_parts.push(format!("OW:{}", extract_hg_author(user)));
                continue;
            }

            if let Some(tag) = strip_hg_field_prefix(trimmed, "tag:") {
                current_parts.push(format!("tag:{}", tag));
                continue;
            }

            if let Some(date) = strip_hg_field_prefix(trimmed, "date:") {
                if let Some(compact) = compact_hg_date_value(date) {
                    current_parts.push(format!("DT:{}", compact));
                } else {
                    current_parts.push(format!("DT:{}", date));
                }
                continue;
            }

            if let Some(summary) = strip_hg_field_prefix(trimmed, "summary:") {
                current_parts.push(format!("CM:{}", summary));
                continue;
            }

            if let Some(parent) = strip_hg_field_prefix(trimmed, "parent:") {
                current_parts.push(format!("parent:{}", parent));
                continue;
            }

            if let Some(bookmark) = strip_hg_field_prefix(trimmed, "bookmark:") {
                current_parts.push(format!("bookmark:{}", bookmark));
                continue;
            }
        }

        // 法则 E: 过滤 Hg 网络操作噪音 (comparing with, searching for changes)
        // 这些噪音行不属于 changeset 字段，直接跳过
        {
            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("comparing with ")
                || lower == "searching for changes"
                || lower.starts_with("searching for changes")
            {
                continue;
            }
        }

        // 非 changeset 字段的行: flush changeset，作为独立记录处理
        flush_current(&mut current_parts, &mut records);
        in_changeset = false;

        // HG-7 + 法则 B/E: 处理嵌入的 diff 行 (hg log --patch 场景)
        // diff 头部降维: "diff -r ... -r ... path" → "DIFF://<path>"
        if trimmed.starts_with("diff -r ") {
            if let Some(path) = trimmed.split_whitespace().last() {
                let clean_path = path.trim();
                if looks_like_vcs_path(clean_path) {
                    records.push(VcsRecord::Raw(format!("DIFF://{}", clean_path)));
                    continue;
                }
            }
        }

        // 法则 B: 剥离 --- 和 +++ 行的时间戳 (Hg 格式: \tThu Apr 10 09:15:00 2026 +0800)
        if trimmed.starts_with("--- ") || trimmed.starts_with("+++ ") {
            if let Some((path_part, _)) = trimmed.split_once('\t') {
                records.push(VcsRecord::Raw(path_part.to_string()));
            } else {
                // 无 tab 分隔符: 提取前两个 token ("---/+++" + "a/b/path")
                let mut tokens = trimmed.split_whitespace();
                match (tokens.next(), tokens.next()) {
                    (Some(prefix), Some(path)) => {
                        records.push(VcsRecord::Raw(format!("{} {}", prefix, path)));
                    }
                    _ => {
                        records.push(VcsRecord::Raw(trimmed.to_string()));
                    }
                }
            }
            continue;
        }

        if looks_like_vcs_path(trimmed) && !trimmed.contains(':') && trimmed.len() < 200 {
            records.push(VcsRecord::File {
                status: None,
                path: trimmed.to_string(),
            });
            continue;
        }

        records.push(VcsRecord::Raw(trimmed.to_string()));
    }

    // flush 最后一个 changeset
    flush_current(&mut current_parts, &mut records);

    records
}

// --- Parsers ---

pub struct HgStatusParser;
impl VcsParser for HgStatusParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() {
                continue;
            }
            // 跳过所有 hg status/st 命令变体（含参数和路径）
            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("hg status") || lower == "hg st" || lower.starts_with("hg st ") {
                continue;
            }

            if let Some((status, path)) = parse_simple_status_path(trimmed) {
                records.push(VcsRecord::File {
                    status: Some(status),
                    path,
                });
                continue;
            }

            if looks_like_vcs_path(trimmed) {
                records.push(VcsRecord::File {
                    status: None,
                    path: trimmed.to_string(),
                });
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Status, records)
    }
}

pub struct HgDiffParser;
impl VcsParser for HgDiffParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let line = line.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // HG-7 + 法则 E: diff 头部降维
            // 格式 "diff -r 17:abc123def456 -r 18:eeff3344aa55 path" → "DIFF://path (17:abc... 18:eeff...)"
            if trimmed.starts_with("diff -r ") {
                let tokens: Vec<&str> = trimmed.split_whitespace().collect();
                let path = tokens.last().map(|s| s.trim()).unwrap_or("");
                // 提取 -r 后的 rev:hash 值 (tokens 中 "-r" 和值相邻)
                let rev_pairs: Vec<&str> = tokens
                    .windows(2)
                    .filter(|w| w[0] == "-r")
                    .map(|w| w[1])
                    .collect();
                if looks_like_vcs_path(path) {
                    if rev_pairs.is_empty() {
                        records.push(VcsRecord::Raw(format!("DIFF://{}", path)));
                    } else {
                        records.push(VcsRecord::Raw(format!(
                            "DIFF://{} ({})",
                            path,
                            rev_pairs.join(" ")
                        )));
                    }
                }
                continue;
            }
            // 格式 "diff --git a/path b/path" → "DIFF://path"
            if trimmed.starts_with("diff --git ") {
                if let Some(path_part) = trimmed.strip_prefix("diff --git ") {
                    if let Some(path) = path_part.split_whitespace().next() {
                        let clean_path = path.strip_prefix("a/").unwrap_or(path);
                        if looks_like_vcs_path(clean_path) {
                            records.push(VcsRecord::Raw(format!("DIFF://{}", clean_path)));
                            continue;
                        }
                    }
                }
            }

            if parse_generic_patch_or_stat_line(line, trimmed, &mut records) {
                continue;
            }

            if looks_like_vcs_path(trimmed) {
                records.push(VcsRecord::File {
                    status: None,
                    path: trimmed.to_string(),
                });
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Diff, records)
    }
}

pub struct HgLogParser;
impl VcsParser for HgLogParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let records = parse_hg_changeset_like_records(raw, &["hg log"]);
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgHeadsParser;
impl VcsParser for HgHeadsParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let records = parse_hg_changeset_like_records(raw, &["hg heads"]);
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgOutgoingParser;
impl VcsParser for HgOutgoingParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let records = parse_hg_changeset_like_records(raw, &["hg outgoing"]);
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgIncomingParser;
impl VcsParser for HgIncomingParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let records = parse_hg_changeset_like_records(raw, &["hg incoming"]);
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgParentsParser;
impl VcsParser for HgParentsParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let records = parse_hg_changeset_like_records(raw, &["hg parents"]);
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgBackoutParser;
impl VcsParser for HgBackoutParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut in_changeset = false;

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg backout") {
                continue;
            }

            if let Some(target) = trimmed.strip_prefix("backing out changeset ") {
                let target = target.trim();
                if !target.is_empty() {
                    records.push(VcsRecord::Raw(format!("backout target: {target}")));
                    continue;
                }
            }

            if trimmed.eq_ignore_ascii_case("changeset:") || trimmed.starts_with("changeset: ") {
                in_changeset = true;
                if let Some(id) = trimmed.strip_prefix("changeset:") {
                    let id = id.trim();
                    if !id.is_empty() {
                        records.push(VcsRecord::Commit(id.to_string()));
                    }
                }
                continue;
            }

            if in_changeset && trimmed.starts_with("summary: ") {
                if let Some(summary) = trimmed.strip_prefix("summary:") {
                    records.push(VcsRecord::Subject(summary.trim().to_string()));
                }
                in_changeset = false;
                continue;
            }

            if trimmed.starts_with("backed out changeset ")
                || trimmed.eq_ignore_ascii_case("changeset backed out")
            {
                records.push(VcsRecord::Raw("backed out".to_string()));
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgBookmarksParser;
impl VcsParser for HgBookmarksParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg bookmarks") {
                continue;
            }

            if trimmed.starts_with("no bookmarks set") {
                records.push(VcsRecord::Raw("no bookmarks".to_string()));
                continue;
            }

            let is_active = trimmed.starts_with('*');
            let content = trimmed.strip_prefix('*').unwrap_or(trimmed).trim();

            if let Some((name, revision)) = split_on_repeated_whitespace(content) {
                if !name.is_empty() && !revision.is_empty() {
                    let marker = if is_active { "*" } else { "" };
                    records.push(VcsRecord::Raw(format!("{marker}{name} {revision}")));
                    continue;
                }
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgBranchesParser;
impl VcsParser for HgBranchesParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg branches") {
                continue;
            }

            if let Some((branch, rest)) = split_on_repeated_whitespace(trimmed) {
                let compact_rest = collapse_inline_whitespace(rest);
                if !branch.is_empty() && !compact_rest.is_empty() {
                    records.push(VcsRecord::Raw(compact_hg_branch_line(&format!(
                        "{branch} {compact_rest}"
                    ))));
                    continue;
                }
            }

            records.push(VcsRecord::Raw(compact_hg_branch_line(
                &collapse_inline_whitespace(trimmed),
            )));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgCloneParser;
impl VcsParser for HgCloneParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg clone") {
                continue;
            }

            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("hg clone ") {
                continue;
            }

            if let Some(path) = trimmed.strip_prefix("destination directory:") {
                let path = path.trim();
                if !path.is_empty() {
                    records.push(VcsRecord::Raw(format!("dest: {}", path)));
                }
                continue;
            }

            if lower == "requesting all changes"
                || lower == "searching for changes"
                || lower == "adding changesets"
                || lower == "adding manifests"
                || lower == "adding file changes"
                || lower.starts_with("cloning from ")
            {
                continue;
            }

            if let Some(branch) = trimmed.strip_prefix("updating to branch ") {
                let branch = branch.trim();
                if !branch.is_empty() {
                    records.push(VcsRecord::Raw(format!("branch: {}", branch)));
                    continue;
                }
            }

            if let Some(compact) = compact_hg_update_summary_line(trimmed) {
                records.push(VcsRecord::Raw(compact));
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgCommitParser;
impl VcsParser for HgCommitParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut in_committing_files = false;

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg commit") {
                continue;
            }

            if trimmed.eq_ignore_ascii_case("committing files:") {
                in_committing_files = true;
                continue;
            }

            if let Some(changeset) = trimmed.strip_prefix("committed changeset ") {
                let changeset = changeset.trim();
                if !changeset.is_empty() {
                    records.push(VcsRecord::Commit(changeset.to_string()));
                }
                in_committing_files = false;
                continue;
            }

            if in_committing_files {
                if looks_like_vcs_path(trimmed) {
                    records.push(VcsRecord::File {
                        status: None,
                        path: trimmed.to_string(),
                    });
                } else {
                    records.push(VcsRecord::Raw(trimmed.to_string()));
                }
                continue;
            }

            if looks_like_vcs_path(trimmed) && !trimmed.contains(':') && trimmed.len() < 200 {
                records.push(VcsRecord::File {
                    status: None,
                    path: trimmed.to_string(),
                });
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgMergeParser;
impl VcsParser for HgMergeParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg merge") {
                continue;
            }

            if let Some(target) = trimmed.strip_prefix("merging with ") {
                let target = target.trim().trim_end_matches('.');
                if !target.is_empty() {
                    records.push(VcsRecord::Raw(format!("merge {target}")));
                    continue;
                }
            }

            if let Some(path) = trimmed.strip_prefix("Auto-merging ") {
                let path = path.trim();
                if !path.is_empty() {
                    records.push(VcsRecord::Raw(format!("Auto-merging {}", path)));
                    continue;
                }
            }

            if trimmed.eq_ignore_ascii_case("Merge completed") {
                continue;
            }

            if let Some(compact) = compact_hg_update_summary_line(trimmed) {
                records.push(VcsRecord::Raw(compact));
                continue;
            }

            if looks_like_vcs_path(trimmed) {
                records.push(VcsRecord::File {
                    status: None,
                    path: trimmed.to_string(),
                });
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgPhaseParser;
impl VcsParser for HgPhaseParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg phase") {
                continue;
            }

            if let Some(line) = trimmed.strip_suffix(')') {
                if let Some((revision, phase)) = line.rsplit_once(" (") {
                    let revision = revision.trim();
                    let phase = phase.trim();
                    if !revision.is_empty() && !phase.is_empty() {
                        records.push(VcsRecord::Raw(format!("{revision} {phase}")));
                        continue;
                    }
                }
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgPullParser;
impl VcsParser for HgPullParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut pending_no_changes = false;
        let mut saw_changeset_delta = false;

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg pull") {
                continue;
            }

            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("hg pull ") {
                continue;
            }

            if let Some(url) = trimmed.strip_prefix("pulling from ") {
                let url = url.trim();
                if !url.is_empty() {
                    records.push(VcsRecord::Raw(format!("pull: {}", url)));
                }
                continue;
            }

            if lower == "searching for changes" {
                continue;
            }

            if lower == "no changes found" {
                pending_no_changes = true;
                continue;
            }

            if lower.starts_with("added ") && lower.contains("changeset") {
                saw_changeset_delta = true;
            }

            if let Some(update) = trimmed.strip_prefix("updated to ") {
                let update = update.trim();
                if !update.is_empty() {
                    records.push(VcsRecord::Raw(format!("updated: {}", update)));
                    continue;
                }
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        if pending_no_changes && !saw_changeset_delta {
            records.push(VcsRecord::Raw("no changes".to_string()));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgPushParser;
impl VcsParser for HgPushParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut first_remote: Option<String> = None;

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg push") {
                continue;
            }

            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("hg push ") {
                continue;
            }

            if let Some(url) = trimmed.strip_prefix("pushing to ") {
                let url = url.trim();
                if !url.is_empty() && first_remote.is_none() {
                    first_remote = Some(url.to_string());
                }
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        if let Some(remote) = first_remote {
            records.insert(0, VcsRecord::Raw(format!("push: {}", remote)));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgRollbackParser;
impl VcsParser for HgRollbackParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut saw_target = false;

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg rollback") {
                continue;
            }

            if let Some(target) = trimmed.strip_prefix("rolling back to ") {
                let target = target.trim().trim_end_matches('.');
                if !target.is_empty() {
                    records.push(VcsRecord::Raw(format!("rollback {target}")));
                    saw_target = true;
                    continue;
                }
            }

            if trimmed.eq_ignore_ascii_case("rollback completed") {
                if !saw_target {
                    records.push(VcsRecord::Raw("rollback".to_string()));
                }
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgShelveParser;
impl VcsParser for HgShelveParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg shelve") {
                continue;
            }

            if let Some(name) = trimmed.strip_prefix("shelved as ") {
                let name = name.trim();
                if !name.is_empty() {
                    records.push(VcsRecord::Raw(format!("shelved: {name}")));
                    continue;
                }
            }

            if looks_like_vcs_path(trimmed) {
                records.push(VcsRecord::File {
                    status: None,
                    path: trimmed.to_string(),
                });
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgTagParser;
impl VcsParser for HgTagParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg tag") {
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("tag '") {
                if let Some((name, tail)) = rest.split_once('\'') {
                    let name = name.trim();
                    if !name.is_empty() {
                        let tail = tail.trim();
                        if tail.is_empty() {
                            records.push(VcsRecord::Raw(format!("tag: {name}")));
                        } else {
                            records.push(VcsRecord::Raw(format!("tag: {name} {tail}")));
                        }
                        continue;
                    }
                }
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgUpdateParser;
impl VcsParser for HgUpdateParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg update") {
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("updating to changeset ") {
                let changeset = rest.trim().trim_end_matches('.');
                if !changeset.is_empty() {
                    records.push(VcsRecord::Raw(format!("update {changeset}")));
                    continue;
                }
            }

            if let Some(compact) = compact_hg_update_summary_line(trimmed) {
                records.push(VcsRecord::Raw(compact));
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("working directory now at ") {
                let revision = rest.trim().trim_end_matches('.');
                if !revision.is_empty() {
                    records.push(VcsRecord::Raw(format!("wd {revision}")));
                    continue;
                }
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::Hg, VcsDocKind::Status, records)
    }
}

// --- "Other" 命令解析器 (HG-9: 零压缩命令优化) ---

pub struct HgCopyParser;
impl VcsParser for HgCopyParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg copy") {
                continue;
            }
            // "copying src/file.txt to dst/file.txt" → "copy src/file.txt -> dst/file.txt"
            if let Some(rest) = trimmed.strip_prefix("copying ") {
                if let Some((src, dst)) = rest.split_once(" to ") {
                    records.push(VcsRecord::Raw(format!(
                        "copy {} -> {}",
                        src.trim(),
                        dst.trim()
                    )));
                    continue;
                }
            }
            records.push(VcsRecord::Raw(trimmed.to_string()));
        }
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgMoveParser;
impl VcsParser for HgMoveParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg move") {
                continue;
            }
            // "moving src/old.txt to dst/new.txt" → "move src/old.txt -> dst/new.txt"
            if let Some(rest) = trimmed.strip_prefix("moving ") {
                if let Some((src, dst)) = rest.split_once(" to ") {
                    records.push(VcsRecord::Raw(format!(
                        "move {} -> {}",
                        src.trim(),
                        dst.trim()
                    )));
                    continue;
                }
            }
            records.push(VcsRecord::Raw(trimmed.to_string()));
        }
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgPurgeParser;
impl VcsParser for HgPurgeParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg purge") {
                continue;
            }
            // "removed: build/temp/file.txt" → "D build/temp/file.txt"
            if let Some(path) = trimmed.strip_prefix("removed:") {
                let path = path.trim();
                if !path.is_empty() && looks_like_vcs_path(path) {
                    records.push(VcsRecord::File {
                        status: Some('D'),
                        path: path.to_string(),
                    });
                    continue;
                }
            }
            records.push(VcsRecord::Raw(trimmed.to_string()));
        }
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Status, records)
    }
}

pub struct HgArchiveParser;
impl VcsParser for HgArchiveParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg archive") {
                continue;
            }
            // "archive created: /tmp/project-backup.zip" → "archive:/tmp/project-backup.zip"
            if let Some(path) = trimmed.strip_prefix("archive created:") {
                records.push(VcsRecord::Raw(format!("archive:{}", path.trim())));
                continue;
            }
            // "files: 123" → "files:123"
            if let Some(rest) = trimmed.strip_prefix("files:") {
                records.push(VcsRecord::Raw(format!("files:{}", rest.trim())));
                continue;
            }
            // "size: 2.3 MB" → "size:2.3MB"
            if let Some(rest) = trimmed.strip_prefix("size:") {
                records.push(VcsRecord::Raw(format!(
                    "size:{}",
                    collapse_inline_whitespace(rest)
                )));
                continue;
            }
            records.push(VcsRecord::Raw(trimmed.to_string()));
        }
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgVerifyParser;
impl VcsParser for HgVerifyParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut verified_count: Option<String> = None;
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg verify") {
                continue;
            }
            // "verified 456 changesets" → 摘取数量
            if let Some(rest) = trimmed.strip_prefix("verified ") {
                verified_count = Some(rest.trim().to_string());
            }
        }
        if let Some(count) = verified_count {
            records.push(VcsRecord::Raw(format!("verified {}", count)));
        } else {
            // 无 verified 行，保留原样
            for line in raw.lines() {
                let trimmed = line.trim_end_matches('\r').trim();
                if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg verify") {
                    continue;
                }
                records.push(VcsRecord::Raw(trimmed.to_string()));
            }
        }
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgIdentifyParser;
impl VcsParser for HgIdentifyParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg identify") {
                continue;
            }
            // 直接输出 changeset hash，不添加无意义的 "id" 前缀
            records.push(VcsRecord::Raw(trimmed.to_string()));
        }
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgPathsParser;
impl VcsParser for HgPathsParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg paths") {
                continue;
            }
            // "default = https://bitbucket.org/user/repo" → "default=<URL>"
            if let Some((name, url)) = trimmed.split_once('=') {
                let name = name.trim();
                let url = url.trim();
                if !name.is_empty() && !url.is_empty() {
                    records.push(VcsRecord::Raw(format!("{}={}", name, url)));
                    continue;
                }
            }
            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgConfigParser;
impl VcsParser for HgConfigParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg config") {
                continue;
            }
            // "[ui]" → section headers 保留
            // "username = alice.chen" → "username=alice.chen"
            if trimmed.starts_with('[') {
                records.push(VcsRecord::Raw(trimmed.to_string()));
                continue;
            }
            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                if !key.is_empty() {
                    records.push(VcsRecord::Raw(format!("{}={}", key, value)));
                    continue;
                }
            }
            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgSummarizeParser;
impl VcsParser for HgSummarizeParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg summarize") {
                continue;
            }
            // "Branch: main" → "BR:main"
            if let Some(rest) = trimmed.strip_prefix("Branch:") {
                records.push(VcsRecord::Raw(format!("BR:{}", rest.trim())));
                continue;
            }
            // "Parent:" → "parent:"
            if let Some(rest) = trimmed.strip_prefix("Parent:") {
                records.push(VcsRecord::Raw(format!(
                    "parent:{}",
                    collapse_inline_whitespace(rest)
                )));
                continue;
            }
            // 通用 key:value 处理
            if let Some((key, value)) = trimmed.split_once(':') {
                let key = key.trim();
                let value = collapse_inline_whitespace(value.trim());
                if !key.is_empty() {
                    records.push(VcsRecord::Raw(format!("{}={}", key, value)));
                    continue;
                }
            }
            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}

pub struct HgTransplantParser;
impl VcsParser for HgTransplantParser {
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg transplant") {
                continue;
            }
            // "transplanting abc1234:msg1" → "transplant abc1234"
            if let Some(rest) = trimmed.strip_prefix("transplanting ") {
                if let Some((rev, _msg)) = rest.split_once(':') {
                    records.push(VcsRecord::Raw(format!("transplant {}", rev.trim())));
                    continue;
                }
            }
            // "transplanted 3 revisions" → 保留数量摘要
            if trimmed.starts_with("transplanted ") {
                records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
                continue;
            }
            records.push(VcsRecord::Raw(trimmed.to_string()));
        }
        to_doc_if_any(VcsTool::Hg, VcsDocKind::Log, records)
    }
}
