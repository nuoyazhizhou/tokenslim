#![allow(dead_code)]
use std::collections::HashSet;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum VcsTool {
    Svn,
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
    /// 将 VcsRecord 格式化为可读字符串。
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
            VcsRecord::Subject(s) => write!(f, "{}", s),
            VcsRecord::Author(a) => {
                let extracted = extract_author_local(a);
                write!(f, "OW:@{}", extracted)
            }
            VcsRecord::Date(d) => {
                let normalized = normalize_git_datetime(d);
                write!(f, "DT:{}", normalized)
            }
            VcsRecord::Commit(c) => write!(f, "CH:{}", c),
            VcsRecord::Stat(s) => write!(f, "{}", s),
            VcsRecord::Raw(r) => write!(f, "{}", r),
            VcsRecord::Patch(p) => write!(f, "{}", p),
            VcsRecord::Hunk(p) => write!(f, "{}", p),
        }
    }
}

// --- 公共帮助函数 ---

/// 时间强规范化：剥离星期、月份英文、时区，转为 YYYY-MM-DD HH:MM:SS 紧凑格式
/// 输入: "Tue Mar 31 22:11:16 2026 +0800" → "2026-03-31 22:11:16"
fn normalize_git_datetime(raw: &str) -> String {
    let s = raw.trim();
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 5 {
        return s.to_string();
    }
    let month_map: std::collections::HashMap<&str, &str> = [
        ("Jan", "01"),
        ("Feb", "02"),
        ("Mar", "03"),
        ("Apr", "04"),
        ("May", "05"),
        ("Jun", "06"),
        ("Jul", "07"),
        ("Aug", "08"),
        ("Sep", "09"),
        ("Oct", "10"),
        ("Nov", "11"),
        ("Dec", "12"),
    ]
    .iter()
    .cloned()
    .collect();

    let month_idx = if parts[0].len() == 3 && month_map.contains_key(parts[0]) {
        0
    } else {
        1
    };
    let day_idx = month_idx + 1;
    let time_idx = month_idx + 2;
    let year_idx = month_idx + 3;
    if year_idx >= parts.len() {
        return s.to_string();
    }

    let month = month_map.get(parts[month_idx]).unwrap_or(&parts[month_idx]);
    let day = format!("{:0>2}", parts[day_idx].trim_end_matches(','));
    format!("{}-{}-{} {}", parts[year_idx], month, day, parts[time_idx])
}

/// 邮箱降维：提取 @ 前的完整用户名前缀
/// 输入: "Author: Alice <alice.chen@example.com>" → "alice.chen"
/// 输入: "alice.chen" → "alice.chen"
fn extract_author_local(raw: &str) -> String {
    let s = raw.trim_start_matches("Author:").trim();
    if let (Some(start), Some(end)) = (s.find('<'), s.find('>')) {
        let email = &s[start + 1..end];
        if let Some(at_pos) = email.find('@') {
            return email[..at_pos].to_string();
        }
        return email.to_string();
    }
    s.to_string()
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

/// 判断字符串是否为 VCS 路径形态。
pub fn looks_like_vcs_path(path: &str) -> bool {
    let trimmed = path.trim_matches('"');
    if trimmed.contains('@') || trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return false;
    }
    if trimmed.contains('(')
        || trimmed.contains(')')
        || trimmed.contains(';')
        || trimmed.contains('=')
        || trimmed.contains('<')
        || trimmed.contains('>')
    {
        return false;
    }
    let common_tlds = [
        ".com", ".org", ".net", ".io", ".edu", ".gov", ".mil", ".int",
    ];
    if common_tlds
        .iter()
        .any(|tld| trimmed.eq_ignore_ascii_case(tld))
    {
        return false;
    }
    trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.ends_with(".rs")
        || trimmed.ends_with(".md")
        || trimmed.ends_with(".lock")
        || (!trimmed.contains(' ')
            && trimmed.contains('.')
            && trimmed.chars().any(|c| c.is_ascii_alphabetic()))
        || trimmed.starts_with('.')
}

/// 非空记录列表包装为 VcsDocument；空列表返回 None。
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

pub struct SvnStatusParser;
pub struct SvnDiffParser;
pub struct SvnLogParser;
pub struct SvnBlameParser;
pub struct SvnListParser;
pub struct SvnPropParser;
pub struct SvnInfoParser;
pub struct SvnUpdateParser;
pub struct SvnSwitchParser;
pub struct SvnRelocateParser;
pub struct SvnMergeParser;
pub struct SvnLockParser;
pub struct SvnUnlockParser;
pub struct SvnRevertParser;
pub struct SvnCleanupParser;
pub struct SvnResolveParser;
pub struct SvnExportParser;

impl VcsParser for SvnStatusParser {
    /// 解析 `svn status` 输出：将单字符状态行归为状态文件，丢弃叙述性噪音（checked out revision/switched/relocated 等），输出 Status 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("svn status") {
                continue;
            }

            if let Some((status, path)) = parse_simple_status_path(trimmed) {
                records.push(VcsRecord::File {
                    status: Some(status),
                    path,
                });
                continue;
            }

            let lower = trimmed.to_ascii_lowercase();
            // 过滤 SVN 叙述性噪音（法则 E）
            if lower.contains("svn status:")
                || lower.contains("checked out revision")
                || lower.starts_with("updating '.':")
                || lower.contains("transmitting file data")
                || lower == "cleaned up."
                || lower == "cleanup completed."
                || lower == "export complete."
                || lower.starts_with("merged")
                || lower.starts_with("switched to ")
                || lower.starts_with("relocated ")
            {
                // 噪音行 → 直接丢弃，不加入 records
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

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Status, records)
    }
}

impl VcsParser for SvnDiffParser {
    /// 解析 `svn diff` 输出：将 Index 行降维为 DIFF: 头、跳过冗余 ---/+++ 行，diff --summarize 状态行归为文件，输出 Diff 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut skip_redundant_headers = false; // 已发 DIFF:，跳过冗余 ---/+++
        for line in raw.lines() {
            let line = line.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // 丢弃纯分隔线（= 或 - 组成，>= 3 字符）
            if trimmed.chars().all(|c| c == '-' || c == '=') && trimmed.len() >= 3 {
                continue;
            }

            // Index 行 → 统一 DIFF: 头部（法则 E Diff 文件头降维）
            if trimmed.starts_with("Index: ") {
                records.push(VcsRecord::Raw(format!("DIFF:{}", trimmed[7..].trim())));
                skip_redundant_headers = true;
                continue;
            }

            // 跳过冗余 ---/+++ 路径行（DIFF: 已标识文件）
            if skip_redundant_headers
                && (trimmed.starts_with("--- ") || trimmed.starts_with("+++ "))
            {
                continue;
            }

            // diff --summarize 格式: "M       path" → 压缩空格 + 提取状态
            if let Some((status, path)) = parse_simple_status_path(trimmed) {
                records.push(VcsRecord::File {
                    status: Some(status),
                    path,
                });
                continue;
            }

            // @@ hunk 头 → 清除冗余跳过标志（开始新的 diff 内容）
            if trimmed.starts_with("@@") {
                skip_redundant_headers = false;
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

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Diff, records)
    }
}

impl VcsParser for SvnLogParser {
    /// 解析 `svn log` 输出：将 `rN | author | date` 头与后续 message 拍扁合并为单条记录，跳过 changed paths 块，输出 Log 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        // 标志是否处于 `svn log -v` 的 Changed paths 块内。
        // 原实现直接跳过该块，导致变更文件全部丢失，触发语义门禁 G5 失败；
        // 现改为保留带状态字母的文件记录（见下方 parse_svn_changed_path_line）。
        let mut in_changed_paths = false;
        let mut pending_message: Vec<String> = Vec::new();
        let mut last_was_header = false;
        // 当前 revision 头在 records 中的索引：合并 CM 时必须锚定 header，
        // 不能依赖 records.last()（-v 时 Changed paths 的 File 记录会穿插在 header 与 message 之间）。
        let mut header_index: Option<usize> = None;
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() {
                continue;
            }

            if trimmed.starts_with("----") {
                if trimmed.chars().all(|c| c == '-') {
                    in_changed_paths = false;
                    continue;
                }
                records.push(VcsRecord::Raw(trimmed.to_string()));
                continue;
            }

            if trimmed.eq_ignore_ascii_case("changed paths:") {
                in_changed_paths = true;
                continue;
            }
            // svn log -v 块内：变更文件（状态字母 + /路径）是核心语义，保留为 File 记录。
            if in_changed_paths {
                if let Some((st, path)) = parse_svn_changed_path_line(trimmed) {
                    records.push(VcsRecord::File {
                        status: Some(st),
                        path,
                    });
                    continue;
                }
                if trimmed.starts_with('r') {
                    // 遇到下一条 revision 头（rN | ...）即结束本块，
                    // 不 continue，落回下方 strip_prefix('r') 流程处理该头。
                    in_changed_paths = false;
                } else if last_was_header && !trimmed.starts_with('(') {
                    // commit message 行：收集到 pending_message，交由 CM 合并逻辑拍扁进 header。
                    pending_message.push(trimmed.to_string());
                    continue;
                } else {
                    // 拷贝来源标记（"(from ...)"）等其余噪音行丢弃。
                    continue;
                }
            }

            if let Some(rest) = trimmed.strip_prefix('r') {
                // 拍扁：将上一个 revision 的 message 行合并到其 header
                if let Some(idx) = header_index {
                    if !pending_message.is_empty() {
                        if let Some(VcsRecord::Raw(h)) = records.get_mut(idx) {
                            h.push_str(" | CM:");
                            h.push_str(&pending_message.join(" "));
                        }
                        pending_message.clear();
                    }
                }
                if let Some((rev, author, date)) = parse_svn_log_header(rest) {
                    let idx = records.len();
                    records.push(VcsRecord::Raw(format!(
                        "r{} | {} | {}",
                        rev,
                        author,
                        compact_log_date_value(&date)
                    )));
                    header_index = Some(idx);
                    last_was_header = true;
                    continue;
                }
            }

            if last_was_header {
                pending_message.push(trimmed.to_string());
                continue;
            }

            if looks_like_vcs_path(trimmed) {
                records.push(VcsRecord::File {
                    status: None,
                    path: trimmed.to_string(),
                });
                continue;
            }

            records.push(VcsRecord::Subject(trimmed.to_string()));
        }
        // 处理最后一个 revision 的 message
        if let Some(idx) = header_index {
            if !pending_message.is_empty() {
                if let Some(VcsRecord::Raw(h)) = records.get_mut(idx) {
                    h.push_str(" | CM:");
                    h.push_str(&pending_message.join(" "));
                }
            }
        }
        to_doc_if_any(VcsTool::Svn, VcsDocKind::Log, records)
    }
}

impl VcsParser for SvnBlameParser {
    /// 解析 `svn blame` 输出：将每行 版本/作者 标注解析为记录（连续相同标注行折叠去重），输出 Show 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut last_revision: Option<String> = None;
        let mut last_author: Option<String> = None;

        for line in raw.lines() {
            let line = line.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.to_ascii_lowercase().starts_with("svn blame") {
                last_revision = None;
                last_author = None;
                continue;
            }

            if let Some(parsed) = parse_svn_blame_line(line) {
                let is_repeat = last_revision.as_deref() == Some(parsed.revision.as_str())
                    && last_author.as_deref() == Some(parsed.author.as_str());
                records.push(VcsRecord::Raw(if is_repeat {
                    parsed.blanked_rendered
                } else {
                    parsed.rendered
                }));
                last_revision = Some(parsed.revision);
                last_author = Some(parsed.author);
                continue;
            }

            last_revision = None;
            last_author = None;
            if looks_like_vcs_path(trimmed) {
                records.push(VcsRecord::File {
                    status: None,
                    path: trimmed.to_string(),
                });
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Show, records)
    }
}

impl VcsParser for SvnListParser {
    /// 解析 `svn list` 输出：将目录条目归为文件记录，输出 Show 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.to_ascii_lowercase().starts_with("svn list") {
                continue;
            }

            if let Some(path) = parse_svn_list_entry(trimmed) {
                records.push(VcsRecord::File { status: None, path });
                continue;
            }

            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Show, records)
    }
}

impl VcsParser for SvnPropParser {
    /// 解析 `svn propget/proplist/propset` 输出：将 `Properties on 'path'` 块内键值对拍扁为 PROPS:path 单行，set 反馈归为标签文件，输出 Show 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut current_path: Option<String> = None; // 当前文件路径
        let mut current_props: Vec<String> = Vec::new(); // 当前文件的属性 KV 对

        for line in raw.lines() {
            let line = line.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let lower = trimmed.to_ascii_lowercase();
            // 跳过 svn prop* 命令行（锚点守卫已保留，避免重复）
            if lower.starts_with("svn propget")
                || lower.starts_with("svn proplist")
                || lower.starts_with("svn propset")
            {
                continue;
            }

            // Properties on 'path': → 新文件块开始，先 flush 上一个
            if trimmed.starts_with("Properties on ") {
                flush_prop_block(&mut records, &mut current_path, &mut current_props);
                if let Some(p) = parse_svn_first_quoted_path(trimmed) {
                    current_path = Some(p);
                }
                continue;
            }

            // 压缩 propset 反馈行: property 'X' set on 'path' → [set X] path
            if let Some(rest) = trimmed.strip_prefix("property '") {
                if let Some((prop, rest)) = rest.split_once("' set on '") {
                    let path = rest.trim_end_matches('\'');
                    records.push(VcsRecord::LabeledFile {
                        label: format!("set {}", prop),
                        path: path.to_string(),
                    });
                    continue;
                }
            }

            // 属性行: "  svn:eol-style" 或 "    native"（续行值）
            // 有前导空白 → 属于当前属性块的键或值
            let has_indent = line
                .chars()
                .next()
                .map(|c| c.is_whitespace())
                .unwrap_or(false);
            if has_indent {
                if let Some((key, value)) = parse_svn_property_line(trimmed) {
                    // KV 对 → 累积到当前文件
                    let compacted_value = value.trim().to_string();
                    current_props.push(format!("{}={}", key, compacted_value));
                } else {
                    // 续行值 → 追加到上一个属性的值后面
                    if let Some(last) = current_props.last_mut() {
                        *last = format!("{} {}", last, trimmed);
                    }
                }
                continue;
            }

            // 非缩进行 → 可能是非标准输出
            records.push(VcsRecord::Raw(trimmed.to_string()));
        }

        // flush 最后一个文件块
        flush_prop_block(&mut records, &mut current_path, &mut current_props);

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Show, records)
    }
}

/// 将当前文件的累积属性拍扁为单行输出: PROPS:path | k1=v1; k2=v2
fn flush_prop_block(
    records: &mut Vec<VcsRecord>,
    current_path: &mut Option<String>,
    current_props: &mut Vec<String>,
) {
    if let Some(path) = current_path.take() {
        if !current_props.is_empty() {
            records.push(VcsRecord::Raw(format!(
                "PROPS:{} | {}",
                path,
                current_props.join("; ")
            )));
        }
        current_props.clear();
    }
}

impl VcsParser for SvnInfoParser {
    /// 解析 `svn info` 输出：委托紧凑信息记录构建，提取 URL/根/版本/最后修改等元数据，输出 Show 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let records = compact_svn_info_records(raw);
        to_doc_if_any(VcsTool::Svn, VcsDocKind::Show, records)
    }
}

impl VcsParser for SvnUpdateParser {
    /// 解析 `svn update` 输出：将更新路径归为 M 状态文件（U 保留给冲突），提取 Updated/At revision 计数为 Raw，输出 Status 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("svn update") {
                continue;
            }

            if trimmed == "Updating '.':" || trimmed == "Updating \".\":" {
                continue;
            }

            if let Some(path) = parse_svn_update_path(trimmed) {
                records.push(VcsRecord::File {
                    status: Some('M'), // 法则: U 专属 Conflict，update → M (Modified)
                    path,
                });
                continue;
            }

            // SVN update 原生状态行拦截: "U    path" / "C    path" / " U   path"
            // parse_simple_status_path 不允许 U（宪法保留给 Unmerged），因此手动映射
            if let Some((status, path)) = parse_svn_update_status_line(trimmed) {
                records.push(VcsRecord::File {
                    status: Some(status),
                    path,
                });
                continue;
            }

            if let Some((status, path)) = parse_simple_status_path(trimmed) {
                records.push(VcsRecord::File {
                    status: Some(status),
                    path,
                });
                continue;
            }

            if let Some(revision) = parse_svn_revision_line(trimmed, "Updated to revision ") {
                records.push(VcsRecord::Raw(revision));
                continue;
            }

            if let Some(revision) = parse_svn_revision_line(trimmed, "At revision ") {
                records.push(VcsRecord::Raw(revision));
                continue;
            }

            if let Some(summary) = compact_svn_file_count_line(trimmed, "Updated ") {
                records.push(VcsRecord::Raw(summary));
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Status, records)
    }
}

impl VcsParser for SvnSwitchParser {
    /// 解析 `svn switch` 输出：将更新路径归为 M 状态文件，提取 `Switched to new URL` 与目标，输出 Status 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let cmd_target = extract_svn_cmd_target(raw, "svn switch");

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty()
                || trimmed.eq_ignore_ascii_case("svn switch")
                || trimmed.to_ascii_lowercase().starts_with("svn switch ")
            {
                continue;
            }

            if let Some(path) = parse_svn_update_path(trimmed) {
                records.push(VcsRecord::File {
                    status: Some('M'), // 法则: U 专属 Conflict，switch → M (Modified)
                    path,
                });
                continue;
            }

            if let Some((status, path)) = parse_simple_status_path(trimmed) {
                records.push(VcsRecord::File {
                    status: Some(status),
                    path,
                });
                continue;
            }

            if let Some(revision) = parse_svn_revision_line(trimmed, "Updated to revision ") {
                records.push(VcsRecord::Raw(revision));
                continue;
            }

            if let Some(url) = trimmed.strip_prefix("Switched to new URL: ") {
                let url = url.trim();
                if Some(url) == cmd_target.as_deref() {
                    records.push(VcsRecord::Raw("switched".to_string()));
                } else {
                    records.push(VcsRecord::Raw(format!("switch-url: {url}")));
                }
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Status, records)
    }
}

impl VcsParser for SvnRelocateParser {
    /// 解析 `svn relocate` 输出：将 `Relocated 'path' to new URL` 归为记录（匹配命令目标时省略 URL），输出 Log 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let cmd_target = extract_svn_cmd_target(raw, "svn relocate");

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty()
                || trimmed.eq_ignore_ascii_case("svn relocate")
                || trimmed.to_ascii_lowercase().starts_with("svn relocate ")
            {
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("Relocated '") {
                if let Some((path, url)) = rest.split_once("' to new URL ") {
                    let path = path.trim();
                    let url = url.trim();
                    if Some(url) == cmd_target.as_deref() {
                        records.push(VcsRecord::Raw(format!("relocated: {path}")));
                    } else {
                        records.push(VcsRecord::Raw(format!("relocate: {path} -> {url}")));
                    }
                    continue;
                }
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Log, records)
    }
}

impl VcsParser for SvnMergeParser {
    /// 解析 `svn merge` 输出：将合并路径行归为 LabeledFile，合并版本摘要压缩，输出 Log 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("svn merge") {
                continue;
            }

            if trimmed.eq_ignore_ascii_case("Merging differences between repository URLs") {
                records.push(VcsRecord::Raw("repo-merge".to_string()));
                continue;
            }

            if let Some(summary) = compact_svn_merge_revision_line(trimmed) {
                records.push(VcsRecord::Raw(summary));
                continue;
            }

            if let Some((label, path)) = parse_svn_merge_path_line(trimmed) {
                records.push(VcsRecord::LabeledFile { label, path });
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

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Log, records)
    }
}

impl VcsParser for SvnLockParser {
    /// 解析 `svn lock` 输出：将引号路径按锁标签归为 LabeledFile，输出 Log 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("svn lock") {
                continue;
            }

            if let Some(path) = parse_svn_first_quoted_path(trimmed) {
                let label = parse_svn_lock_label(trimmed);
                records.push(VcsRecord::LabeledFile { label, path });
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Log, records)
    }
}

impl VcsParser for SvnUnlockParser {
    /// 解析 `svn unlock` 输出：将引号路径归为 unlock 标签文件，输出 Log 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("svn unlock") {
                continue;
            }

            if let Some(path) = parse_svn_first_quoted_path(trimmed) {
                records.push(VcsRecord::LabeledFile {
                    label: "unlock".to_string(),
                    path,
                });
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Log, records)
    }
}

impl VcsParser for SvnRevertParser {
    /// 解析 `svn revert` 输出：将 `Reverted path` 归为 R 状态文件，其余路径作兜底，输出 Status 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("svn revert") {
                continue;
            }

            if let Some(path) = parse_svn_path_after_prefix(trimmed, "Reverted ") {
                records.push(VcsRecord::File {
                    status: Some('R'),
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

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Status, records)
    }
}

impl VcsParser for SvnCleanupParser {
    /// 解析 `svn cleanup` 输出：将 `Cleaned up.` 去重为 ST:[CLEANED] 记录，输出 Log 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut emitted_cleanup = false;

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("svn cleanup") {
                continue;
            }

            if trimmed.eq_ignore_ascii_case("Cleaned up.")
                || trimmed.eq_ignore_ascii_case("Cleanup completed.")
            {
                if !emitted_cleanup {
                    records.push(VcsRecord::Raw("ST:[CLEANED]".to_string()));
                    emitted_cleanup = true;
                }
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Log, records)
    }
}

impl VcsParser for SvnResolveParser {
    /// 解析 `svn resolve` 输出：将 `Resolved conflict at`/`Resolved:` 路径归为 resolved 标签文件，输出 Status 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("svn resolve") {
                continue;
            }

            if let Some(path) = parse_svn_path_after_prefix(trimmed, "Resolved conflict at ") {
                records.push(VcsRecord::LabeledFile {
                    label: "resolved".to_string(),
                    path,
                });
                continue;
            }

            if let Some(path) = parse_svn_path_after_prefix(trimmed, "Resolved: ") {
                records.push(VcsRecord::LabeledFile {
                    label: "resolved".to_string(),
                    path,
                });
                continue;
            }

            if let Some(path) =
                parse_svn_path_after_prefix(trimmed, "Resolved conflicted state of ")
            {
                records.push(VcsRecord::LabeledFile {
                    label: "resolved".to_string(),
                    path,
                });
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Status, records)
    }
}

impl VcsParser for SvnExportParser {
    /// 解析 `svn export` 输出：将 `Exported revision`/`Exporting path` 归为记录，`Export complete.` 去重为 ST:[EXPORTED]，输出 Log 类文档。
    #[tracing::instrument(level = "debug", skip_all)]
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut emitted_export = false;

        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("svn export") {
                continue;
            }

            if trimmed.eq_ignore_ascii_case("Export complete.") {
                if !emitted_export {
                    records.push(VcsRecord::Raw("ST:[EXPORTED]".to_string()));
                    emitted_export = true;
                }
                continue;
            }

            if let Some(revision) = parse_svn_revision_line(trimmed, "Exported revision ") {
                records.push(VcsRecord::Raw(revision));
                continue;
            }

            if let Some(path) = parse_svn_path_after_prefix(trimmed, "Exporting ") {
                records.push(VcsRecord::File { status: None, path });
                continue;
            }

            records.push(VcsRecord::Raw(collapse_inline_whitespace(trimmed)));
        }

        to_doc_if_any(VcsTool::Svn, VcsDocKind::Log, records)
    }
}

/// 判断行是否为 git oneline 头。
fn is_git_oneline_header(line: &str) -> bool {
    let mut parts = line.splitn(2, ' ');
    let hash = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or_default().trim();

    !rest.is_empty()
        && (7..=40).contains(&hash.len())
        && hash.chars().all(|c| c.is_ascii_hexdigit())
}

/// 确保 commit 边界换行。
fn enforce_commit_boundaries(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 16);
    let mut i = 0usize;

    while let Some(rel) = input[i..].find("commit ") {
        let idx = i + rel;
        out.push_str(&input[i..idx]);

        let needs_newline = idx > 0 && input.as_bytes()[idx - 1] != b'\n';
        let hash = input[idx + "commit ".len()..]
            .split_whitespace()
            .next()
            .unwrap_or_default();
        let looks_hash = hash.len() >= 7 && hash.chars().all(|c| c.is_ascii_hexdigit());

        if needs_newline && looks_hash {
            out.push('\n');
        }

        out.push_str("commit ");
        i = idx + "commit ".len();
    }

    out.push_str(&input[i..]);
    out
}

/// 解析 git diff 头。
fn parse_diff_git_header(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("diff --git ")?;
    let (left, consumed_left) = parse_git_path_token(rest)?;
    let remaining = rest.get(consumed_left..)?.trim_start();
    let (right, _) = parse_git_path_token(remaining)?;
    Some((left, right))
}

/// 解析 git 路径 token。
fn parse_git_path_token(input: &str) -> Option<(String, usize)> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    if bytes[0] == b'"' {
        let mut out = String::new();
        let mut i = 1usize;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' if i + 1 < bytes.len() => {
                    out.push(bytes[i + 1] as char);
                    i += 2;
                }
                b'"' => return Some((out, i + 1)),
                c => {
                    out.push(c as char);
                    i += 1;
                }
            }
        }
        None
    } else {
        let end = input.find(char::is_whitespace).unwrap_or(input.len());
        if end == 0 {
            None
        } else {
            Some((input[..end].to_string(), end))
        }
    }
}

/// 解析 git 补丁/统计行。
fn parse_git_patch_or_stat_line(line: &str, trimmed: &str, records: &mut Vec<VcsRecord>) -> bool {
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

/// 通用 status 解析。
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

/// 通用 log 解析。
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

        // Ignore visual separator lines entirely to save tokens
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

        if let Some(rest) = trimmed.strip_prefix("revno:") {
            let id = rest.trim();
            if !id.is_empty() {
                records.push(VcsRecord::Commit(id.to_string()));
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

/// 通用 diff 解析。
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

        // Ignore visual separator lines entirely to save tokens
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

        if let Some(path) = parse_modified_file_header(trimmed) {
            records.push(VcsRecord::DiffFile {
                left: path.clone(),
                right: path,
            });
            continue;
        }

        if let Some(record) = parse_darcs_hunk_record(trimmed) {
            records.push(record);
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

/// 按长词状态前缀解析状态码与路径。
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

/// 解析 "Modified: path" 文件头行。
fn parse_modified_file_header(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("=== modified file") {
        let start = line.len() - rest.len();
        let suffix = line[start..].trim();
        let quoted = suffix.trim_matches('"').trim_matches('\'');
        if !quoted.is_empty() {
            return Some(quoted.to_string());
        }
    }
    None
}

/// 解析 darcs hunk 行。
fn parse_darcs_hunk_record(line: &str) -> Option<VcsRecord> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("hunk ")?;
    let (path, suffix) = split_first_token(rest)?;

    if path.is_empty() {
        return None;
    }

    let _ = suffix;

    Some(VcsRecord::File {
        status: None,
        path: path.to_string(),
    })
}

/// 判断是否为 diff stat 行。
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

/// 解析单字符状态码 + 路径。
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
        // Git porcelain can emit two-column status like "?? path" or "MM path".
        // Treat "??" as untracked, otherwise keep the first status column.
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

/// 按允许状态码集合解析单字符状态行。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 从 Checking in 行提取路径。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 判断是否为 CVS 修订号 token。
#[tracing::instrument(level = "debug", skip_all)]
fn looks_like_cvs_revision_token(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed.contains('.')
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '.' || ch == '_')
}

/// 从 cvs tag 命令行解析标签名。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 解析引号包裹的路径。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 解析 bzr 修订计数。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_bzr_revision_count_line(line: &str, prefix: &str) -> Option<usize> {
    let rest = line.strip_prefix(prefix)?.trim();
    let count = rest.split_whitespace().next()?.parse::<usize>().ok()?;
    Some(count)
}

/// 解析 bzr 总修订数。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_bzr_total_revisions_line(line: &str) -> Option<usize> {
    let rest = line.strip_prefix("Total ")?.trim();
    let count = rest.split_whitespace().next()?.parse::<usize>().ok()?;
    Some(count)
}

/// 解析 git remote 传输记录。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_git_remote_transfer_records(raw: &str, remote_prefix: &str) -> Vec<VcsRecord> {
    let mut records = Vec::new();
    let mut seen_remote_lines = HashSet::new();

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() || is_git_transfer_noise_line(trimmed) {
            continue;
        }

        if let Some(location) = trimmed.strip_prefix(remote_prefix) {
            let compact_location = compact_git_remote_location(location);
            if compact_location.is_empty() {
                continue;
            }

            let rendered = format!("{}{}", remote_prefix, compact_location);
            if seen_remote_lines.insert(rendered.clone()) {
                records.push(VcsRecord::Raw(rendered));
            }
            continue;
        }

        if trimmed.contains("->") {
            records.push(VcsRecord::Raw(compact_git_ref_update_line(trimmed)));
            continue;
        }

        records.push(VcsRecord::Raw(trimmed.to_string()));
    }

    records
}

/// 判断是否为 git 传输噪音行。
#[tracing::instrument(level = "debug", skip_all)]
fn is_git_transfer_noise_line(line: &str) -> bool {
    let lower = line.trim().to_ascii_lowercase();
    lower.starts_with("remote: ")
        || lower.starts_with("enumerating objects:")
        || lower.starts_with("counting objects:")
        || lower.starts_with("compressing objects:")
        || lower.starts_with("receiving objects:")
        || lower.starts_with("resolving deltas:")
        || lower.starts_with("unpacking objects:")
        || lower.starts_with("writing objects:")
        || lower.starts_with("delta compression using ")
        || lower.starts_with("total ")
}

/// 紧凑化 git remote 位置。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_git_remote_location(location: &str) -> String {
    let trimmed = location.trim();
    let without_protocol = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .or_else(|| trimmed.strip_prefix("ssh://"))
        .unwrap_or(trimmed);

    without_protocol
        .strip_prefix("git@")
        .unwrap_or(without_protocol)
        .to_string()
}

/// 紧凑化 git ref 更新行。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_git_ref_update_line(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 紧凑化 diff stat 行。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_diff_stat_line(line: &str) -> String {
    let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");

    // file-level stat: path | 10 +++++----
    // NOTE:
    //   `+++++----` is a visualization bar, not exact insert/delete counts.
    //   Do not derive `+X/-Y` from bar width because that can be semantically wrong.
    //   Keep only reliable numeric signal (`count`) for file-level stats.
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

    // summary line: 3 files changed, 8 insertions(+), 9 deletions(-)
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

/// 从 ref 更新行解析 git pull 目标。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_git_pull_target_from_ref_update(line: &str) -> Option<String> {
    let normalized = compact_git_ref_update_line(line);
    let (_, rhs) = normalized.split_once("->")?;
    let target = rhs.trim();
    if target.is_empty() {
        None
    } else {
        Some(target.to_string())
    }
}

/// 紧凑化 git pull 文件统计行。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_git_pull_file_stat_line(line: &str) -> Option<String> {
    let (path, rhs) = line.split_once('|')?;
    let path = path.trim();
    if path.is_empty() || !looks_like_vcs_path(path) {
        return None;
    }
    let count = rhs
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<usize>().ok())?;
    Some(format!("{path}:{count}"))
}

/// 解析 git pull 变更摘要。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_git_pull_change_summary(line: &str) -> Option<String> {
    let normalized = compact_diff_stat_line(line);
    let lower = normalized.to_ascii_lowercase();
    if !lower.contains("file changed") && !lower.contains("files changed") {
        return None;
    }

    let files = normalized
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<usize>().ok())?;

    // Prefer already-compacted numeric summary when available, e.g.:
    //   "1 file changed +10 -4"
    let mut compact_tokens = normalized.split_whitespace().peekable();
    let mut ins_compact: Option<usize> = None;
    let mut del_compact: Option<usize> = None;
    while let Some(tok) = compact_tokens.next() {
        if let Some(v) = tok.strip_prefix('+').and_then(|n| n.parse::<usize>().ok()) {
            ins_compact = Some(v);
            if del_compact.is_some() {
                break;
            }
            continue;
        }
        if let Some(v) = tok.strip_prefix('-').and_then(|n| n.parse::<usize>().ok()) {
            del_compact = Some(v);
            if ins_compact.is_some() {
                break;
            }
        }
    }

    let parse_count_before = |needle: &str| -> Option<usize> {
        let idx = lower.find(needle)?;
        let head = &normalized[..idx];
        head.split_whitespace().last()?.parse::<usize>().ok()
    };

    let ins = ins_compact.unwrap_or_else(|| {
        parse_count_before(" insertion(+)")
            .or_else(|| parse_count_before(" insertions(+)"))
            .unwrap_or(0)
    });
    let del = del_compact.unwrap_or_else(|| {
        parse_count_before(" deletion(-)")
            .or_else(|| parse_count_before(" deletions(-)"))
            .unwrap_or(0)
    });

    let noun = if files == 1 { "file" } else { "files" };
    Some(format!("{files} {noun} changed +{ins} -{del}"))
}

/// 紧凑化 git 汇总行。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_git_summary_line(line: &str) -> String {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("updating ")
        || lower.starts_with("already up to date")
        || lower.starts_with("reverting commit ")
        || lower.starts_with("reverted commit ")
        || lower.starts_with("automatic revert of commit ")
    {
        trimmed
            .trim_end_matches('.')
            .trim_end_matches(':')
            .to_string()
    } else {
        trimmed.to_string()
    }
}

/// 压缩 hg update 汇总行。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_hg_update_summary_line(line: &str) -> Option<String> {
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

/// 解析 git clean 动作行。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_git_clean_action_line(line: &str) -> Option<(String, String)> {
    if let Some(path) = line.strip_prefix("Removing ") {
        let path = path.trim();
        if !path.is_empty() {
            return Some(("Removing".to_string(), path.to_string()));
        }
    }

    if let Some(path) = line.strip_prefix("Would remove ") {
        let path = path.trim();
        if !path.is_empty() {
            return Some(("Would remove".to_string(), path.to_string()));
        }
    }

    None
}

/// 解析 git pull 记录。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_git_pull_records(raw: &str) -> Vec<VcsRecord> {
    let mut records = Vec::new();
    let mut seen_summary_lines = HashSet::new();
    let mut command: Option<String> = None;
    let mut pull_target: Option<String> = None;
    let mut update_range: Option<String> = None;

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() || is_git_transfer_noise_line(trimmed) {
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("git pull") {
            if command.is_none() {
                command = Some(compact_git_ref_update_line(trimmed));
            }
            continue;
        }
        if trimmed.starts_with("From ") {
            continue;
        }

        if trimmed.contains("->") {
            if pull_target.is_none() {
                pull_target = parse_git_pull_target_from_ref_update(trimmed);
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("Updating ") {
            let range = rest.trim().trim_end_matches('.');
            if !range.is_empty() {
                update_range = Some(range.to_string());
            }
            continue;
        }

        if lower == "fast-forward" {
            if let Some(range) = update_range.as_deref() {
                records.push(VcsRecord::Raw(format!("Fast-forward: {range}")));
            } else {
                records.push(VcsRecord::Raw("Fast-forward".to_string()));
            }
            continue;
        }

        if trimmed.contains(" | ") {
            if let Some(compact) = compact_git_pull_file_stat_line(trimmed) {
                records.push(VcsRecord::Raw(compact));
                continue;
            }
        }

        if let Some(summary) = parse_git_pull_change_summary(trimmed) {
            records.push(VcsRecord::Raw(summary));
            continue;
        }

        let compact = compact_git_summary_line(trimmed);
        if seen_summary_lines.insert(compact.clone()) {
            records.push(VcsRecord::Raw(compact));
        }
    }

    let command_line = match command {
        Some(cmd) if cmd.eq_ignore_ascii_case("git pull") => {
            if let Some(target) = pull_target {
                format!("git pull {target}")
            } else {
                cmd
            }
        }
        Some(cmd) => cmd,
        None => {
            if let Some(target) = pull_target {
                format!("git pull {target}")
            } else {
                "git pull".to_string()
            }
        }
    };
    records.insert(0, VcsRecord::Raw(command_line));

    records
}

/// 解析 git grep 匹配行。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_git_grep_match_line(line: &str) -> Option<(&str, &str, &str)> {
    // Accept both:
    //   path/to/file:123: matched text
    //   C:/path/file.rs:123: matched text
    let mut split = line.rsplitn(2, ':');
    let content = split.next()?.trim();
    let head = split.next()?;
    let mut split2 = head.rsplitn(2, ':');
    let line_no = split2.next()?.trim();
    let path = split2.next()?.trim();
    if path.is_empty() || line_no.is_empty() || !line_no.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((path, line_no, content))
}

/// 在连续空白处切分行。
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

struct SvnBlameLine {
    revision: String,
    author: String,
    rendered: String,
    blanked_rendered: String,
}

struct GitBlameLine {
    commit: String,
    author: String,
    date: String,
    rendered: String,
    blanked_rendered: String,
}

struct CvsAnnotateLine {
    revision: String,
    author: String,
    date: String,
    rendered: String,
    blanked_rendered: String,
}

/// 解析 svn blame 行。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_svn_blame_line(line: &str) -> Option<SvnBlameLine> {
    let line_trimmed_end = line.trim_end_matches('\r');

    let rev_start = line_trimmed_end.find(|c: char| !c.is_whitespace())?;
    let rev_end = line_trimmed_end[rev_start..]
        .find(char::is_whitespace)
        .map(|i| i + rev_start)
        .unwrap_or(line_trimmed_end.len());
    let revision = &line_trimmed_end[rev_start..rev_end];

    if revision != "-" && !revision.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let author_start_rel = line_trimmed_end[rev_end..].find(|c: char| !c.is_whitespace())?;
    let author_start = rev_end + author_start_rel;
    let author_end_rel = line_trimmed_end[author_start..]
        .find(char::is_whitespace)
        .unwrap_or(line_trimmed_end.len() - author_start);
    let author_end = author_start + author_end_rel;
    let author = &line_trimmed_end[author_start..author_end];

    if author.is_empty() {
        return None;
    }

    let prefix = &line_trimmed_end[rev_start..author_end];
    let content_raw = &line_trimmed_end[author_end..];

    let content = if content_raw.is_empty() {
        String::new()
    } else {
        compact_blame_content_after_prefix(content_raw)
    };

    Some(SvnBlameLine {
        revision: revision.to_string(),
        author: author.to_string(),
        rendered: render_compacted_blame_line(prefix, &content),
        blanked_rendered: blank_compacted_blame_line(prefix, &content),
    })
}

/// 解析 git blame 行。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_git_blame_line(line: &str) -> Option<GitBlameLine> {
    let line = line.trim_end_matches('\r');
    let commit_end = line.find(char::is_whitespace)?;
    let commit = &line[..commit_end];
    if commit.is_empty() || !commit.chars().all(|c| c.is_ascii_hexdigit() || c == '^') {
        return None;
    }

    let after_commit = &line[commit_end..];
    let rest = after_commit.trim_start();
    if !rest.starts_with('(') {
        return None;
    }

    let meta_end = rest.find(')')?;
    let meta = &rest[1..meta_end];
    let line_number_sep = meta.rfind(' ')?;
    let line_number = &meta[line_number_sep + 1..];
    if line_number.is_empty() || !line_number.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let before_line_number = &meta[..line_number_sep];
    if before_line_number.len() < 20 {
        return None;
    }

    let date_start = before_line_number.len() - 19;
    if date_start == 0 || !before_line_number.as_bytes()[date_start - 1].is_ascii_whitespace() {
        return None;
    }

    let date = &before_line_number[date_start..];
    if !looks_like_git_blame_datetime(date) {
        return None;
    }

    let author_field = &before_line_number[..date_start];
    let author = author_field.trim_end();
    if author.is_empty() {
        return None;
    }

    let rest_after_meta = &rest[meta_end + 1..];
    let content = if rest_after_meta.trim().is_empty() {
        String::new()
    } else {
        rest_after_meta.trim_start().to_string()
    };
    let prefix = format!("{} ({} {} {})", commit, author, date, line_number);
    let line_number_start = prefix.rfind(' ')?;
    let blanked_prefix = format!(
        "{}{}",
        " ".repeat(prefix[..line_number_start].chars().count()),
        &prefix[line_number_start..]
    );

    Some(GitBlameLine {
        commit: commit.to_string(),
        author: author.to_string(),
        date: date.to_string(),
        rendered: render_compacted_blame_line(&prefix, &content),
        blanked_rendered: render_compacted_blame_line(&blanked_prefix, &content),
    })
}

/// 解析 cvs annotate 行。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 判断是否形如 git blame 日期时间。
#[tracing::instrument(level = "debug", skip_all)]
fn looks_like_git_blame_datetime(date: &str) -> bool {
    date.len() == 19
        && date.chars().enumerate().all(|(idx, ch)| match idx {
            4 | 7 => ch == '-',
            10 => ch == ' ',
            13 | 16 => ch == ':',
            _ => ch.is_ascii_digit(),
        })
}

/// 渲染空白化 blame 行。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 将字节范围替换为空格。
#[tracing::instrument(level = "debug", skip_all)]
fn blank_byte_ranges_with_spaces(input: &str, ranges: &[(usize, usize)]) -> String {
    let mut chars: Vec<char> = input.chars().collect();
    for &(start, end) in ranges {
        let start_char = input[..start].chars().count();
        let end_char = input[..end].chars().count();
        for ch in chars.iter_mut().take(end_char).skip(start_char) {
            *ch = ' ';
        }
    }
    chars.into_iter().collect()
}

/// 渲染紧凑化 blame 行。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 解析 svn list 条目。
fn parse_svn_list_entry(line: &str) -> Option<String> {
    let mut tokens: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();
    if tokens.is_empty() {
        return None;
    }

    // Verbose svn list output (date author size path) can keep meaning with single-space columns.
    if (tokens[0] == "-" && tokens.len() >= 2)
        || (tokens.len() >= 4 && looks_like_iso_date_token(&tokens[0]))
    {
        compact_svn_list_size_field(&mut tokens);
        return Some(tokens.join(" "));
    }

    // `svn list --verbose` 列布局：revision author size Mon DD HH:MM path。
    // 大小位于 tokens[2]，仅折叠列宽并把大小转为人读缩写（日期时间保留原文）。
    if tokens.len() >= 6
        && tokens[0].chars().all(|c| c.is_ascii_digit())
        && tokens[2].parse::<u64>().is_ok()
        && tokens[3].len() == 3
        && tokens[4].len() == 2
        && looks_like_hhmm_token(&tokens[5])
    {
        if let Some(size) = compact_human_size_token(&tokens[2]) {
            tokens[2] = size;
        }
        return Some(tokens.join(" "));
    }

    if line.ends_with('/') || looks_like_vcs_path(line) {
        return Some(line.to_string());
    }

    let path = tokens.last()?.as_str();
    if path.ends_with('/') || looks_like_vcs_path(path) {
        Some(path.to_string())
    } else {
        None
    }
}

/// 紧凑化 svn list 大小字段。
fn compact_svn_list_size_field(tokens: &mut [String]) {
    // Common verbose form:
    // YYYY-MM-DD HH:MM author size path
    if tokens.len() >= 5 && looks_like_hhmm_token(&tokens[1]) {
        if let Some(size) = compact_human_size_token(&tokens[3]) {
            tokens[3] = size;
        }
        return;
    }

    // Fallback compact form:
    // YYYY-MM-DD author size path
    if tokens.len() >= 4 {
        if let Some(size) = compact_human_size_token(&tokens[2]) {
            tokens[2] = size;
        }
    }
}

/// 判断 token 是否为 HH:MM 时间。
fn looks_like_hhmm_token(token: &str) -> bool {
    let mut parts = token.split(':');
    let h = parts.next().unwrap_or_default();
    let m = parts.next().unwrap_or_default();
    parts.next().is_none()
        && h.len() == 2
        && m.len() == 2
        && h.chars().all(|c| c.is_ascii_digit())
        && m.chars().all(|c| c.is_ascii_digit())
}

/// 紧凑化人类可读大小 token。
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

/// 压缩大小类元数据值。
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

    // Accept plain-byte forms like "1536", "1536 bytes", "1536B".
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

/// 判断 token 是否为 ISO 日期。
fn looks_like_iso_date_token(token: &str) -> bool {
    let mut parts = token.split('-');
    let y = parts.next().unwrap_or_default();
    let m = parts.next().unwrap_or_default();
    let d = parts.next().unwrap_or_default();
    parts.next().is_none()
        && y.len() == 4
        && m.len() == 2
        && d.len() == 2
        && y.chars().all(|c| c.is_ascii_digit())
        && m.chars().all(|c| c.is_ascii_digit())
        && d.chars().all(|c| c.is_ascii_digit())
}

/// 压缩 blame 代码缩进。
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
    // Aggressive mode: preserve at most half of indentation levels (floor).
    // Examples: 2->1, 3->1, 4->2, 6->3.
    let kept_levels = if levels <= 1 { levels } else { levels / 2 };
    let kept_spaces = " ".repeat(kept_levels * unit);

    format!("{}{}", kept_spaces, body)
}

/// 压缩 blame 前缀后内容。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_blame_content_after_prefix(content: &str) -> String {
    let trimmed = content
        .strip_prefix(' ')
        .or_else(|| content.strip_prefix('\t'))
        .unwrap_or(content);
    compact_blame_code_indent(trimmed)
}

/// 从 svn prop 命令解析属性名。
fn parse_svn_property_name_from_command(line: &str) -> Option<String> {
    let mut parts = line.split_whitespace();
    let tool = parts.next()?;
    let cmd = parts.next()?;
    if !tool.eq_ignore_ascii_case("svn")
        || !(cmd.eq_ignore_ascii_case("propget") || cmd.eq_ignore_ascii_case("proplist"))
    {
        return None;
    }

    for part in parts {
        if part.starts_with('-') {
            continue;
        }
        return Some(part.to_string());
    }

    None
}

/// 解析 K-V 行。
fn parse_key_value_line(line: &str) -> Option<(String, String)> {
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() || value.is_empty() {
        return None;
    }
    Some((key.to_string(), value.to_string()))
}

/// 判断行是否为纯 URL。
fn is_url_only_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("https://")
        || trimmed.starts_with("http://")
        || trimmed.starts_with("svn://")
        || trimmed.starts_with("ssh://")
        || trimmed.starts_with("file://")
}

/// 解析 svn 首个引号路径。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_svn_first_quoted_path(line: &str) -> Option<String> {
    let start = line.find('\'')?;
    let rest = &line[start + 1..];
    let end = rest.find('\'')?;
    let path = rest[..end].trim();
    if path.is_empty() || !looks_like_vcs_path(path) {
        return None;
    }
    Some(path.to_string())
}

/// 解析 svn 前缀后路径。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_svn_path_after_prefix(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix)?.trim();
    let path = rest
        .trim_end_matches('.')
        .trim_end_matches(':')
        .trim()
        .trim_matches('\'')
        .trim_matches('"')
        .trim();
    if path.is_empty() || !looks_like_vcs_path(path) {
        return None;
    }
    Some(path.to_string())
}

/// 解析 svn update 路径。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_svn_update_path(line: &str) -> Option<String> {
    parse_svn_path_after_prefix(line, "Updating ")
}

/// SVN update 原生状态行解析: "U    path" → (M, path); "C    path" → (!, path)
/// 仅当状态符后紧跟空格且路径包含有效文件名字符时才匹配
fn parse_svn_update_status_line(line: &str) -> Option<(char, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let first = trimmed.chars().next()?;
    // 仅匹配已知 SVN update 状态码
    let code = match first {
        'U' | 'u' => 'M',
        'C' | 'c' => '!',
        'A' | 'a' => 'A',
        'D' | 'd' => 'D',
        'G' | 'g' => 'M',
        _ => return None,
    };
    // 状态符后必须紧跟空白，且剩余部分看起来像路径（不含标点符号）
    let rest = trimmed[first.len_utf8()..].trim_start();
    let path = rest.to_string();
    if path.is_empty() || path.contains('.') && !path.contains('/') {
        return None; // 拒绝单文件名的点号（如 "Updated." 中的 U）
    }
    // 拒绝明显不是路径的内容（含双空格、引号、括号、冒号）
    if path.contains("  ") || path.contains('"') || path.contains('(') || path.contains(':') {
        return None;
    }
    Some((code, path))
}

/// 提取 svn 命令目标路径。
#[tracing::instrument(level = "debug", skip_all)]
fn extract_svn_cmd_target(raw: &str, cmd_prefix: &str) -> Option<String> {
    let line = raw.lines().find(|l| !l.trim().is_empty())?.trim();
    let lower = line.to_ascii_lowercase();
    if !lower.starts_with(cmd_prefix) {
        return None;
    }
    let rest = line[cmd_prefix.len()..].trim();
    if rest.is_empty() {
        return None;
    }
    rest.split_whitespace().last().map(|s| s.to_string())
}

/// 解析 svn 修订行。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_svn_revision_line(line: &str, prefix: &str) -> Option<String> {
    let revision = line
        .strip_prefix(prefix)?
        .trim()
        .trim_end_matches('.')
        .trim();
    if revision.is_empty() {
        return None;
    }

    let revision = revision.strip_prefix('r').unwrap_or(revision);
    if revision.chars().all(|c| c.is_ascii_digit()) {
        Some(format!("r{}", revision))
    } else {
        None
    }
}

/// 紧凑化 svn 文件计数行。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_svn_file_count_line(line: &str, prefix: &str) -> Option<String> {
    let rest = line
        .strip_prefix(prefix)?
        .trim()
        .trim_end_matches('.')
        .trim();
    let mut parts = rest.split_whitespace();
    let count = parts.next()?;
    let noun = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    if !count.chars().all(|c| c.is_ascii_digit()) || !matches!(noun, "file" | "files") {
        return None;
    }

    Some(format!("{} {}", count, noun))
}

/// 紧凑化 svn merge 修订行。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_svn_merge_revision_line(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("Merging ")?
        .trim()
        .trim_end_matches('.')
        .trim();
    let (start, end) = rest.split_once(" through ")?;
    let start = start.trim();
    let end = end.trim();
    if start.is_empty() || end.is_empty() {
        return None;
    }
    Some(format!("merge {}..{}", start, end))
}

/// 解析 svn merge 路径行。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_svn_merge_path_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("-- ")?.trim();
    let (label, path) = rest.split_once(':')?;
    let path = path.trim().trim_matches('\'').trim_matches('"').trim();
    if path.is_empty() || !looks_like_vcs_path(path) {
        return None;
    }

    let normalized = label.trim().to_ascii_lowercase();
    let label = match normalized.as_str() {
        "non-conflicting" => "ok".to_string(),
        "merged" | "merging" => "merge".to_string(),
        "conflicted" | "conflict" => "conflict".to_string(),
        other => other.to_string(),
    };

    Some((label, path.to_string()))
}

/// 解析 svn lock 标签。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_svn_lock_label(line: &str) -> String {
    if let Some((_, tail)) = line.rsplit_once(" locked by user '") {
        if let Some(user) = tail.strip_suffix("'.") {
            let user = user.trim();
            if !user.is_empty() {
                return format!("lock({})", user);
            }
        }
    }
    "lock".to_string()
}

/// 判断是否为 `svn info --xml` 输出：首行命令含 xml 标志，或文本带 `<?xml`/`<info>` 标记。
fn looks_like_svn_info_xml(raw: &str) -> bool {
    let mut svn_info = false;
    let mut xml = false;
    for line in raw.lines() {
        let t = line.trim();
        if t.to_ascii_lowercase().starts_with("svn info") {
            if t.to_ascii_lowercase().contains("xml") {
                return true;
            }
            svn_info = true;
        } else if t.starts_with("<?xml") || t.starts_with("<info>") {
            xml = true;
        }
    }
    svn_info && xml
}

/// 解析 XML 开标签中的键值属性列表，正确处理含空白字符的引号值（如 wcroot 路径）。
fn parse_xml_attrs(tag: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut rest = tag.trim_start_matches('<').trim_start();
    // 跳过标签名，定位属性区
    if let Some(i) = rest.find(char::is_whitespace) {
        rest = &rest[i..];
    } else {
        return attrs;
    }
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].trim().to_string();
        let after_eq = rest[eq + 1..].trim_start();
        if let Some(after_quote) = after_eq.strip_prefix('"') {
            if let Some(close) = after_quote.find('"') {
                let val = after_quote[..close].to_string();
                if !key.is_empty() {
                    attrs.push((key, val));
                }
                rest = &after_quote[close + 1..];
                continue;
            }
        }
        break;
    }
    attrs
}

/// 提取单个标签开始与结束标记之间的文本内容。
fn xml_tag_text(line: &str, open: &str, close: &str) -> Option<String> {
    let start = line.find(open)? + open.len();
    let tail = &line[start..];
    let end = tail.find(close)?;
    Some(tail[..end].trim().to_string())
}

/// 判断行是否仅为纯 XML 标记（去掉标签后无任何文本内容），如容器标签/闭合标签/声明。
/// 这类行无语义价值，压缩时应丢弃。
fn is_pure_xml_markup(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || (t.starts_with("<?") && t.ends_with('>')) {
        return true;
    }
    let stripped: String = t
        .split('<')
        .filter_map(|seg| match seg.find('>') {
            Some(idx) => Some(&seg[idx + 1..]),
            None => Some(seg),
        })
        .collect();
    stripped.trim().is_empty()
}

/// 解析 `svn info --xml` 输出，抽取语义字段集合并折叠 XML 结构标记，与文本版 info 输出同构。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_svn_info_xml_records(raw: &str) -> Vec<VcsRecord> {
    let mut path = None;
    let mut node_kind = None;
    let mut revision = None;
    let mut wc_root = None;
    let mut url = None;
    let mut repo_root = None;
    let mut uuid = None;
    let mut depth = None;
    let mut commit_rev = None;
    let mut author = None;
    let mut date = None;
    let mut extras = Vec::new();

    for line in raw.lines() {
        let t = line.trim_end_matches('\r').trim();
        if t.is_empty() || t.to_ascii_lowercase().starts_with("svn info") {
            continue;
        }
        if t.starts_with("<entry") {
            for (k, v) in parse_xml_attrs(t) {
                match k.as_str() {
                    "path" => path = Some(v),
                    "kind" => node_kind = Some(v),
                    "revision" => revision = Some(v),
                    "wcroot" => wc_root = Some(v),
                    _ => {}
                }
            }
            continue;
        }
        if t.starts_with("<commit") {
            for (k, v) in parse_xml_attrs(t) {
                if k == "revision" {
                    commit_rev = Some(v);
                }
            }
            continue;
        }
        if let Some(v) = xml_tag_text(t, "<url>", "</url>") {
            url = Some(v);
            continue;
        }
        if let Some(v) = xml_tag_text(t, "<root>", "</root>") {
            repo_root = Some(v);
            continue;
        }
        if let Some(v) = xml_tag_text(t, "<uuid>", "</uuid>") {
            uuid = Some(v);
            continue;
        }
        if let Some(v) = xml_tag_text(t, "<wcroot>", "</wcroot>") {
            wc_root = Some(v);
            continue;
        }
        if let Some(v) = xml_tag_text(t, "<depth>", "</depth>") {
            depth = Some(v);
            continue;
        }
        if let Some(v) = xml_tag_text(t, "<author>", "</author>") {
            author = Some(v);
            continue;
        }
        if let Some(v) = xml_tag_text(t, "<date>", "</date>") {
            date = Some(v);
            continue;
        }
        // 丢弃无文本内容的纯结构标记（容器/闭合/声明），其子内容已在前序分支捕获。
        if is_pure_xml_markup(t) {
            continue;
        }
        extras.push(t.to_string());
    }

    let mut records = Vec::new();
    if let Some(path) = path {
        records.push(VcsRecord::LabeledFile {
            label: "Path".to_string(),
            path,
        });
    }
    if let Some(wc) = wc_root {
        records.push(VcsRecord::LabeledFile {
            label: "WCRoot".to_string(),
            path: wc,
        });
    }

    let mut location_parts = Vec::new();
    if let Some(url) = url {
        location_parts.push(format!("URL: {}", url));
    }
    if let Some(repo_root) = repo_root {
        location_parts.push(format!("Repo: {}", repo_root));
    }
    push_compact_raw_line(&mut records, location_parts);

    let mut meta_parts = Vec::new();
    if let Some(revision) = revision {
        meta_parts.push(format!("Rev: {}", revision));
    }
    if let Some(node_kind) = node_kind {
        meta_parts.push(format!("Kind: {}", node_kind));
    }
    if let Some(depth) = depth {
        meta_parts.push(format!("Depth: {}", depth));
    }
    if let Some(uuid) = uuid {
        if let Some(short) = uuid.get(..8) {
            meta_parts.push(format!("UUID: {}", short));
        }
    }
    push_compact_raw_line(&mut records, meta_parts);

    let mut commit_parts = Vec::new();
    if let Some(commit_rev) = commit_rev {
        commit_parts.push(format!("CommitRev: {}", commit_rev));
    }
    if let Some(author) = author {
        commit_parts.push(format!("Author: {}", author));
    }
    if let Some(date) = date {
        commit_parts.push(format!("Date: {}", date));
    }
    push_compact_raw_line(&mut records, commit_parts);
    push_compact_raw_line(&mut records, extras);

    records
}

/// 紧凑化 svn info 记录。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_svn_info_records(raw: &str) -> Vec<VcsRecord> {
    // `svn info --xml` 走专属 XML 解析，文本形态走键值行解析。
    if looks_like_svn_info_xml(raw) {
        return compact_svn_info_xml_records(raw);
    }
    let mut path = None;
    let mut wc_root = None;
    let mut url = None;
    let mut rel_url = None;
    let mut repo_root = None;
    let mut revision = None;
    let mut node_kind = None;
    let mut schedule = None;
    let mut author = None;
    let mut last_rev = None;
    let mut date = None;
    let mut extras = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() || trimmed.to_ascii_lowercase().starts_with("svn info") {
            continue;
        }
        if is_url_only_line(trimmed) {
            extras.push(trimmed.to_string());
            continue;
        }

        if let Some((key, value)) = parse_key_value_line(trimmed) {
            match key.as_str() {
                "Path" => path = Some(value),
                "Working Copy Root Path" => wc_root = Some(value),
                "URL" => url = Some(value),
                "Relative URL" => rel_url = Some(value),
                "Repository Root" => repo_root = Some(value),
                "Repository UUID" => {}
                "Revision" => revision = Some(value),
                "Node Kind" => node_kind = compact_svn_node_kind(&value),
                "Schedule" => schedule = compact_svn_schedule(&value),
                "Last Changed Author" => author = Some(value),
                "Last Changed Rev" => last_rev = Some(value),
                "Last Changed Date" => date = Some(compact_info_timestamp(&value)),
                _ => extras.push(format!(
                    "{}: {}",
                    shorten_svn_info_key(&key),
                    compact_info_value(&key, &value)
                )),
            }
            continue;
        }

        extras.push(trimmed.to_string());
    }

    let mut records = Vec::new();

    if let Some(path) = path {
        records.push(VcsRecord::LabeledFile {
            label: "Path".to_string(),
            path,
        });
    }
    if let Some(path) = wc_root {
        records.push(VcsRecord::LabeledFile {
            label: "WCRoot".to_string(),
            path,
        });
    }

    let mut location_parts = Vec::new();
    if let Some(rel_url) = rel_url {
        location_parts.push(format!("RelURL: {}", rel_url));
    } else if let Some(url) = url {
        location_parts.push(format!("URL: {}", url));
    }
    if let Some(repo_root) = repo_root {
        location_parts.push(format!("Repo: {}", repo_root));
    }
    push_compact_raw_line(&mut records, location_parts);

    let mut meta_parts = Vec::new();
    if let Some(revision) = revision {
        meta_parts.push(format!("Rev: {}", revision));
    }
    if let Some(node_kind) = node_kind {
        meta_parts.push(format!("Kind: {}", node_kind));
    }
    if let Some(schedule) = schedule {
        meta_parts.push(format!("Sched: {}", schedule));
    }
    if let Some(author) = author {
        meta_parts.push(format!("Author: {}", author));
    }
    if let Some(last_rev) = last_rev {
        meta_parts.push(format!("LastRev: {}", last_rev));
    }
    if let Some(date) = date {
        meta_parts.push(format!("Date: {}", date));
    }
    push_compact_raw_line(&mut records, meta_parts);
    push_compact_raw_line(&mut records, extras);

    records
}

/// 紧凑化 p4 info 记录。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 将零件列表压入记录并过滤空行。
fn push_compact_raw_line(records: &mut Vec<VcsRecord>, parts: Vec<String>) {
    let compact: Vec<String> = parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect();
    if !compact.is_empty() {
        records.push(VcsRecord::Raw(compact.join(" | ")));
    }
}

/// 紧凑化 info 时间戳。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_info_timestamp(value: &str) -> String {
    if value.len() >= 19 {
        let candidate = &value[..19];
        if looks_like_compact_info_timestamp(candidate) {
            return candidate.to_string();
        }
    }
    value.trim().to_string()
}

/// 判断值是否为紧凑时间戳。
fn looks_like_compact_info_timestamp(value: &str) -> bool {
    value.len() == 19
        && value.chars().enumerate().all(|(idx, ch)| match idx {
            4 | 7 => ch == '-' || ch == '/',
            10 => ch == ' ',
            13 | 16 => ch == ':',
            _ => ch.is_ascii_digit(),
        })
}

/// 紧凑化 svn node kind 值。
fn compact_svn_node_kind(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "file" => None,
        "directory" | "dir" => Some("dir".to_string()),
        "symlink" | "link" => Some("link".to_string()),
        _ => Some(normalized),
    }
}

/// 紧凑化 svn schedule 值。
fn compact_svn_schedule(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("normal") || trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// 按键名压缩 info 值。
fn compact_info_value(key: &str, value: &str) -> String {
    if is_info_date_key(key) {
        compact_info_timestamp(value)
    } else if let Some(compact) = compact_size_metadata_value(key, value) {
        compact
    } else {
        value.trim().to_string()
    }
}

/// 判断键是否为日期类键。
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

/// 缩写 svn info 键名。
fn shorten_svn_info_key(key: &str) -> String {
    match key {
        "Working Copy Root Path" => "WCRoot".to_string(),
        "Relative URL" => "RelURL".to_string(),
        "Repository Root" => "Repo".to_string(),
        "Revision" => "Rev".to_string(),
        "Node Kind" => "Kind".to_string(),
        "Schedule" => "Sched".to_string(),
        "Last Changed Author" => "Author".to_string(),
        "Last Changed Rev" => "LastRev".to_string(),
        "Last Changed Date" => "Date".to_string(),
        _ => key.to_string(),
    }
}

/// 缩写 p4 info 键名。
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

/// 解析 svn log -v 的 Changed paths 变更行，如 `   A /trunk/src/foo.c`，返回 (状态字母, 路径)。
/// 要求路径以 `/` 开头以区分正文行（正文首字母单词不以 `/` 开头）。
fn parse_svn_changed_path_line(trimmed: &str) -> Option<(char, String)> {
    let t = trimmed.trim();
    let mut chars = t.chars();
    let st = chars.next()?;
    if !st.is_ascii_alphabetic() {
        return None;
    }
    let rest = chars.as_str().trim_start();
    if !rest.starts_with('/') {
        return None;
    }
    Some((st, rest.to_string()))
}

/// 解析 svn log 头。
fn parse_svn_log_header(rest: &str) -> Option<(String, String, String)> {
    let mut parts = rest.split('|').map(|s| s.trim());
    let rev = parts.next()?.to_string();
    let author = parts.next()?.to_string();
    let date = parts.next()?.to_string();
    if rev.is_empty() || author.is_empty() || date.is_empty() {
        return None;
    }
    Some((rev, author, date))
}

/// 紧凑化日志日期值。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_log_date_value(value: &str) -> String {
    // 裁剪掉 "(Mon, 14 Apr 2026)" 部分
    let trimmed = value
        .split_once(" (")
        .map(|(head, _)| head)
        .unwrap_or(value)
        .trim();

    if let Some(compact) = compact_hg_date_value(trimmed) {
        return compact;
    }

    // ISO 8601 UTC: "2026-04-14T15:00:00.123456Z" → "2026-04-14 15:00"
    if let Some(prefix) = trimmed.strip_suffix('Z') {
        let head = if let Some((h, frac)) = prefix.rsplit_once('.') {
            if !frac.is_empty() && frac.chars().all(|c| c.is_ascii_digit()) {
                h.to_string()
            } else {
                prefix.to_string()
            }
        } else {
            prefix.to_string()
        };
        return strip_seconds(&head);
    }

    // 通用格式: "2026-04-14 15:00:00 +0000" → "2026-04-14 15:00"（剥离时区 + 秒）
    let without_tz = if let Some((head, _tz)) = trimmed.rsplit_once(" +") {
        head.trim().to_string()
    } else if let Some((head, _tz)) = trimmed.rsplit_once(" -") {
        if head.len() >= 16 {
            head.trim().to_string()
        } else {
            trimmed.to_string()
        }
    } else {
        trimmed.to_string()
    };

    // 裁剪微秒部分
    let without_frac = if let Some((head, _frac)) = without_tz.rsplit_once('.') {
        if head.len() >= 16 {
            head.to_string()
        } else {
            without_tz
        }
    } else {
        without_tz
    };

    strip_seconds(&without_frac)
}

/// 剥离秒级精度: "2026-04-14 15:00:00" → "2026-04-14 15:00"
fn strip_seconds(value: &str) -> String {
    // 匹配末尾 ":SS" 或 ":SS.sss" 模式
    if let Some((head, tail)) = value.rsplit_once(':') {
        // tail 应该是 "00" 或 "00.123" 等
        if tail.len() >= 2 && tail.chars().take(2).all(|c| c.is_ascii_digit()) {
            return head.to_string();
        }
    }
    value.to_string()
}

/// 将 hg 日期规范化为 YYYY-MM-DD HH:MM:SS。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_hg_date_value(value: &str) -> Option<String> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }

    let (month_idx, day_idx, time_idx, year_idx) = match parts.as_slice() {
        [weekday, _, _, _, _, ..] if looks_like_hg_weekday(weekday) => {
            (1usize, 2usize, 3usize, 4usize)
        }
        [_, _, _, _, ..] => (0usize, 1usize, 2usize, 3usize),
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

/// 判断 token 是否为英文星期缩写。
#[tracing::instrument(level = "debug", skip_all)]
fn looks_like_hg_weekday(token: &str) -> bool {
    matches!(token, "Mon" | "Tue" | "Wed" | "Thu" | "Fri" | "Sat" | "Sun")
}

/// 判断 token 是否为 4 位年份。
#[tracing::instrument(level = "debug", skip_all)]
fn looks_like_hg_year(token: &str) -> bool {
    token.len() == 4 && token.chars().all(|ch| ch.is_ascii_digit())
}

/// 判断 token 是否为 HH:MM:SS 时间。
#[tracing::instrument(level = "debug", skip_all)]
fn looks_like_hg_time_token(token: &str) -> bool {
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

/// 将英文月份缩写映射为数字。
#[tracing::instrument(level = "debug", skip_all)]
fn hg_month_number(token: &str) -> Option<u32> {
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

/// 解析 p4 depot 路径。
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

/// 解析 p4 depot spec。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 解析 p4 opened 行。
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

/// 解析 p4 action-path 行。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 解析 p4 sync 行。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 紧凑化 p4 sync 预览汇总。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 解析 p4 change 编号。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 紧凑化 p4 files 汇总。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 解析 p4 resolve 行。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 紧凑化 p4 resolve 汇总。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 解析 p4 revert 行。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_p4_revert_line(line: &str) -> Option<String> {
    let (path, action) = parse_p4_action_path_line(line)?;
    if action.eq_ignore_ascii_case("reverted") {
        Some(path)
    } else {
        None
    }
}

/// 解析 p4 edit 行。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_p4_edit_line(line: &str) -> Option<String> {
    let (path, action) = parse_p4_action_path_line(line)?;
    if action.eq_ignore_ascii_case("opened for edit") {
        Some(path)
    } else {
        None
    }
}

/// 解析 p4 add 行。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_p4_add_line(line: &str) -> Option<String> {
    let (path, action) = parse_p4_action_path_line(line)?;
    if action.eq_ignore_ascii_case("added for add") {
        Some(path)
    } else {
        None
    }
}

/// 解析 p4 delete 行。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_p4_delete_line(line: &str) -> Option<String> {
    let (path, action) = parse_p4_action_path_line(line)?;
    if action.eq_ignore_ascii_case("deleted for delete") {
        Some(path)
    } else {
        None
    }
}

/// 解析 hg changeset 类记录。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_hg_changeset_like_records(raw: &str, command_prefixes: &[&str]) -> Vec<VcsRecord> {
    let mut records = Vec::new();
    let normalized = enforce_hg_changeset_boundaries(raw);

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

        if let Some(changeset) = strip_hg_field_prefix(trimmed, "changeset:") {
            records.push(VcsRecord::Commit(changeset.to_string()));
            continue;
        }

        if let Some(branch) = strip_hg_field_prefix(trimmed, "branch:") {
            records.push(VcsRecord::Branch(branch.to_string()));
            continue;
        }

        if let Some(user) = strip_hg_field_prefix(trimmed, "user:") {
            records.push(VcsRecord::Author(format!("Author: {user}")));
            continue;
        }

        if let Some(tag) = strip_hg_field_prefix(trimmed, "tag:") {
            records.push(VcsRecord::Raw(format!("tag: {tag}")));
            continue;
        }

        if let Some(date) = strip_hg_field_prefix(trimmed, "date:") {
            records.push(VcsRecord::Date(compact_log_date_value(date)));
            continue;
        }

        if let Some(summary) = strip_hg_field_prefix(trimmed, "summary:") {
            records.push(VcsRecord::Subject(summary.to_string()));
            continue;
        }

        if let Some(parent) = strip_hg_field_prefix(trimmed, "parent:") {
            records.push(VcsRecord::Raw(format!("parent: {parent}")));
            continue;
        }

        if let Some(bookmark) = strip_hg_field_prefix(trimmed, "bookmark:") {
            records.push(VcsRecord::Raw(format!("bookmark: {bookmark}")));
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

    records
}

/// 确保 hg changeset 边界换行。
#[tracing::instrument(level = "debug", skip_all)]
fn enforce_hg_changeset_boundaries(input: &str) -> String {
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

/// 剥离 hg 字段前缀。
fn strip_hg_field_prefix<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    line.strip_prefix(prefix).map(|value| value.trim_start())
}

/// 折叠行内连续空白。
fn collapse_inline_whitespace(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 紧凑化 hg branch 行。
fn compact_hg_branch_line(line: &str) -> String {
    line.strip_suffix(" (inactive)")
        .map(|prefix| format!("{prefix}~"))
        .unwrap_or_else(|| line.to_string())
}

/// 在连续空白处切分行。
fn split_on_repeated_whitespace(line: &str) -> Option<(&str, &str)> {
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

/// 解析 p4 元数据行。
fn parse_p4_metadata_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("... ")?;
    let (key, value) = split_first_token(rest)?;
    Some((key.to_string(), value.to_string()))
}

/// 判断键是否为 p4 路径元数据键。
#[tracing::instrument(level = "debug", skip_all)]
fn is_p4_path_metadata_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.ends_with("file") || lower.ends_with("path") || lower == "path"
}

/// 判断值是否为 p4 路径值。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 解析 p4 where 行。
fn parse_p4_where_line(line: &str) -> Option<(String, String, String)> {
    let (depot, rest) = split_first_token(line)?;
    let (client, local) = split_first_token(rest)?;
    if !depot.starts_with("//") || !client.starts_with("//") || local.is_empty() {
        return None;
    }

    Some((depot.to_string(), client.to_string(), local.to_string()))
}

/// 解析 svn property 行。
fn parse_svn_property_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if !trimmed.contains(':') {
        return None;
    }

    // svn:keywords Author Date Id Revision
    // properties may have colons in their names like "svn:keywords"
    if let Some((key, value)) = trimmed.split_once(':') {
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() {
            return Some((key.to_string(), value.to_string()));
        }
    }

    None
}

/// 解析 p4 label 行。
#[tracing::instrument(level = "debug", skip_all)]
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

/// 判断 token 是否为 p4 时间格式。
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

/// 解析 diff 的 index/---/+++/@@ 行与 +/- 补丁行。
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
