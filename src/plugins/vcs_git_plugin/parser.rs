// --- IR 通用定义 (内联隔离) ---
#[derive(Debug, PartialEq, Eq)]
pub enum VcsTool {
    Git,
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
    File {
        status: Option<String>,
        path: String,
    },
    LabeledFile {
        label: String,
        path: String,
    },
    DiffFile {
        left: String,
        right: String,
    },
    Subject(String),
    Author(String),
    Date(String),
    Commit(String),
    Stat(String),
    Raw(String),
    /// 预格式化紧凑输出行，直接原样输出
    CompactLine(String),
    /// 拍扁后的单行 Commit 记录：CH:+OW:+DT:+CM:
    CommitFlattened {
        hash: String,
        author: String,
        date: String,
        message: String,
    },
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
                // 邮箱降维：提取 @ 前的完整前缀
                let extracted = extract_author_local(a);
                write!(f, "OW:@{}", extracted)
            }
            VcsRecord::Date(d) => {
                let normalized = normalize_git_datetime(d);
                write!(f, "DT:{}", normalized)
            }
            VcsRecord::Commit(c) => write!(f, "CH:{}", shorten_hash(c)),
            VcsRecord::Stat(s) => write!(f, "{}", s),
            VcsRecord::Raw(r) => write!(f, "{}", r),
            VcsRecord::CompactLine(s) => write!(f, "{}", s),
            VcsRecord::CommitFlattened {
                hash,
                author,
                date,
                message,
            } => {
                write!(
                    f,
                    "CH:{} OW:@{} DT:{} CM:{}",
                    shorten_hash(hash),
                    author,
                    date,
                    message.trim()
                )
            }
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
    /// VcsParser 解析入口：子类实现具体解析逻辑。
    fn parse(&self, raw: &str) -> Option<VcsDocument>;
}

// --- 公共帮助函数 (独立化) ---

/// 截短 commit hash 至 10 字符（默认更稳妥的最短唯一前缀长度）
fn shorten_hash(hash: &str) -> &str {
    if hash.len() > 10 {
        &hash[..10]
    } else {
        hash
    }
}

/// 折叠字符串中连续 2 个及以上空格为单个空格（用于 branch -v 多列对齐压缩）。
fn collapse_multi_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_run = false;
    for c in s.chars() {
        if c == ' ' {
            if !in_run {
                out.push(' ');
                in_run = true;
            }
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out
}

/// 计算指定哈希在候选集合中的最短唯一前缀长度（从 min_len 起逐步加长）。
///
/// P2-69（D-1）：oneline 模式把多字节首词（如中文 commit 首 token）放入候选集，
/// 旧版按字节递增 `&cur[..l]` 会切断 UTF-8 字符生产 panic。改经 `char_indices`
/// 遍历字符边界，避免在多字节字符中间切片。
pub(super) fn shortest_unique_prefix_len(values: &[String], idx: usize, min_len: usize) -> usize {
    let cur = &values[idx];
    // 字节长度仅用于封顶范围；实际切片一律落在 char 边界上
    let start = min_len.min(cur.len()).max(1);
    // 收集所有 ≤cur 的字符边界字节偏移（含 0 与 cur.len()）
    let mut boundaries = vec![0usize];
    for (byte_off, _) in cur.char_indices() {
        if byte_off > 0 {
            boundaries.push(byte_off);
        }
    }
    if !boundaries.contains(&cur.len()) {
        boundaries.push(cur.len());
    }
    let start_off = match boundaries.iter().position(|&b| b >= start) {
        Some(p) => boundaries[p],
        None => cur.len(),
    };
    for &l in boundaries.iter().filter(|&&b| b >= start_off) {
        let p = &cur[..l];
        let mut unique = true;
        for (j, other) in values.iter().enumerate() {
            if j == idx {
                continue;
            }
            if other.starts_with(p) {
                unique = false;
                break;
            }
        }
        if unique {
            return l;
        }
    }
    cur.len()
}

/// 为去重后的哈希集合构建「哈希→最短唯一前缀」映射（冲突时加长）。
pub(super) fn build_shortest_unique_prefix_map(
    hashes: &[String],
    default_len: usize,
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut unique_hashes: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for h in hashes {
        if seen.insert(h.clone()) {
            unique_hashes.push(h.clone());
        }
    }

    for i in 0..unique_hashes.len() {
        let h = &unique_hashes[i];
        let dynamic = shortest_unique_prefix_len(&unique_hashes, i, 7);
        let target_len = dynamic.max(default_len.min(h.len()));
        // P2-69（D-1）：target_len 可能落在多字节字符中间，用 get(..) 保底回退整串
        let short = h.get(..target_len).unwrap_or(h).to_string();
        map.insert(h.clone(), short);
    }
    map
}

/// 从 git log 输出收集提交哈希候选（commit/reflog/oneline 三种形态）。
fn collect_git_log_hash_candidates(raw: &str) -> Vec<String> {
    let mut hashes = Vec::new();
    let is_oneline_mode = raw.to_ascii_lowercase().contains("--oneline");
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(rest) = t.strip_prefix("commit ") {
            let hash_end = rest.find(' ').unwrap_or(rest.len());
            let hash = &rest[..hash_end];
            if hash.len() >= 6 {
                hashes.push(hash.to_string());
            }
            continue;
        }

        // reflog: "<hash> HEAD@{n}: ..."
        if let Some(sp) = t.find(' ') {
            let first = &t[..sp];
            let rest = &t[sp + 1..];
            if rest.starts_with("HEAD@{") && first.len() >= 6 {
                hashes.push(first.to_string());
                continue;
            }
        }

        // oneline: "<hash> message"
        if let Some(sp) = t.find(' ') {
            let first = &t[..sp];
            let looks_like = first.len() >= 6
                && (first.chars().all(|c| c.is_ascii_hexdigit()) || is_oneline_mode);
            if looks_like {
                hashes.push(first.to_string());
            }
        }
    }
    hashes
}

/// 从 git blame 输出收集行首哈希候选。
fn collect_git_blame_hash_candidates(raw: &str) -> Vec<String> {
    let mut hashes = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() || t.to_ascii_lowercase().starts_with("git blame") {
            continue;
        }
        if let Some(space_pos) = t.find(' ') {
            let hash = &t[..space_pos];
            if hash.len() >= 7 {
                hashes.push(hash.to_string());
            }
        }
    }
    hashes
}

/// 时间强规范化：剥离星期、月份英文、时区，转为 YYYY-MM-DD HH:MM:SS 紧凑格式
/// 输入: "Date:   Fri Apr 15 10:30:00 2024 +0800" → "2024-04-15 10:30:00"
/// 输入: "Date:   Tue Mar 31 22:11:16 2026 +0800" → "2026-03-31 22:11:16"
#[tracing::instrument(level = "debug", skip_all)]
fn normalize_git_datetime(raw: &str) -> String {
    // 剥离 "Date:" 前缀和多余空白
    let s = raw.trim_start_matches("Date:").trim();
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 5 {
        return s.to_string();
    }
    // parts[0]=星期(可选), parts[1]=月份, parts[2]=日期, parts[3]=时间, parts[4]=年份, parts[5..]=时区(可选)
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

    // 找到月份和年份的位置
    let mut month_idx = 0;
    let mut day_idx = 1;
    let mut time_idx = 2;
    let mut year_idx = 3;
    // 如果第一个 token 是星期名（3个字母），则偏移
    if parts[0].len() == 3 && month_map.contains_key(parts[0]) {
        // 第一个是月份，正常
    } else {
        // 第一个是星期，从第二个开始是月份
        month_idx = 1;
        day_idx = 2;
        time_idx = 3;
        year_idx = 4;
    }
    if year_idx >= parts.len() {
        return s.to_string();
    }

    let month = month_map.get(parts[month_idx]).unwrap_or(&parts[month_idx]);
    let day = format!("{:0>2}", parts[day_idx].trim_end_matches(','));

    format!("{}-{}-{} {}", parts[year_idx], month, day, parts[time_idx])
}

/// 邮箱降维：提取 @ 前的完整用户名前缀，保留完整前缀以防重名碰撞
/// 输入: "Author: Alice <alice.chen@example.com>" → "alice.chen"
/// 输入: "Author: nuoyazhizhou" → "nuoyazhizhou"
#[tracing::instrument(level = "debug", skip_all)]
fn extract_author_local(raw: &str) -> String {
    let s = raw.trim_start_matches("Author:").trim();
    // 尝试提取尖括号中的邮箱
    if let (Some(start), Some(end)) = (s.find('<'), s.find('>')) {
        let email = &s[start + 1..end];
        if let Some(at_pos) = email.find('@') {
            return email[..at_pos].to_string();
        }
        return email.to_string();
    }
    // 无尖括号，直接返回去除空白后的结果
    s.to_string()
}

/// 压缩 git log 的 decoration/ref 装饰信息
/// 输入: "(HEAD -> main, origin/main, tag: v1.0)" → "[HEAD->main,o/main,t:v1.0]"
#[tracing::instrument(level = "debug", skip_all)]
fn compact_decoration(line: &str) -> String {
    let s = line
        .replace("origin/", "o/")
        .replace("tag: ", "t:")
        .replace('(', "[")
        .replace(')', "]");
    s
}

/// 判断是否为 Git 网络传输噪音行（fetch/push/pull 进度输出）
#[tracing::instrument(level = "debug", skip_all)]
fn is_git_transfer_noise(trimmed: &str) -> bool {
    let noise_patterns = [
        "Counting objects:",
        "Compressing objects:",
        "Receiving objects:",
        "Resolving deltas:",
        "Writing objects:",
        "Total ",
        "remote: Counting",
        "remote: Compressing",
        "remote: Total",
    ];
    noise_patterns.iter().any(|p| trimmed.starts_with(p))
}

/// 错误/异常输出标准化
/// "fatal: ..." → "!fatal:..."
/// "error: ..." → "!error:..."
/// "CONFLICT (content): ..." → "!CONFLICT:..."
#[tracing::instrument(level = "debug", skip_all)]
fn normalize_error_line(trimmed: &str) -> String {
    if let Some(rest) = trimmed.strip_prefix("fatal: ") {
        format!("!fatal:{}", rest)
    } else if let Some(rest) = trimmed.strip_prefix("error: ") {
        format!("!error:{}", rest)
    } else if let Some(rest) = trimmed.strip_prefix("CONFLICT") {
        // 提取冲突路径: "CONFLICT (content): Merge conflict in X" → "!CONFLICT:X"
        if let Some(path) = rest.rsplit(" in ").next() {
            format!("!CONFLICT:{}", path.trim())
        } else {
            format!(
                "!CONFLICT:{}",
                rest.trim_start_matches(" (content):").trim()
            )
        }
    } else {
        trimmed.to_string()
    }
}

/// 判断是否为 Git hint/advice 教学废话行
#[tracing::instrument(level = "debug", skip_all)]
fn is_git_hint_line(trimmed: &str) -> bool {
    trimmed.starts_with("hint: ")
        || trimmed.starts_with("(use \"git")
        || trimmed.starts_with("(use \"git")
}

/// 非空记录列表包装为 VcsDocument；空列表返回 None。
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

/// 判断字符串是否为 VCS 路径形态。
pub fn looks_like_vcs_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    path.contains('/')
        || path.contains('\\')
        || lower.ends_with(".rs")
        || lower.ends_with(".md")
        || lower.ends_with(".txt")
}

/// 解析状态码前缀（XY 双字符）与路径。
pub fn parse_simple_status_path(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    // 格式 1：XY path（porcelain 双字符，如 " M src/file.rs" 或 "A  src/file.rs"）
    if t.len() > 3 && t.as_bytes()[2] == b' ' {
        let raw0 = t.as_bytes()[0];
        let raw1 = t.as_bytes()[1];
        let status = if raw0 == b' ' {
            raw1 as char
        } else {
            raw0 as char
        };
        let path = t[3..].trim().to_string();
        if looks_like_vcs_path(&path) {
            return Some((status.to_string(), path));
        }
    }
    // 格式 2：X path（单字符状态 + 空格 + 路径，如 "M src/file.rs"）
    if t.len() > 2 && t.as_bytes()[1] == b' ' {
        let status = t.as_bytes()[0] as char;
        let path = t[2..].trim().to_string();
        if looks_like_vcs_path(&path) {
            return Some((status.to_string(), path));
        }
    }
    None
}

/// 紧凑化 git 汇总行（折叠空白）。
pub fn compact_git_summary_line(line: &str) -> String {
    let s = line
        .replace(" files changed", " files")
        .replace(" file changed", " file")
        .replace(" insertions(+)", " ins")
        .replace(" insertion(+)", " ins")
        .replace(" deletions(-)", " del")
        .replace(" deletion(-)", " del");
    s.trim().to_string()
}

/// 判断是否为 diff stat 行（含 " | " 分隔）。
pub fn looks_like_diff_stat_line(line: &str) -> bool {
    line.contains('|') && (line.contains('+') || line.contains('-'))
}

/// 紧凑化 diff stat 行：折叠连续空白。
pub fn compact_diff_stat_line(line: &str) -> String {
    line.trim().to_string()
}

/// 压缩 git reflog 行：checkout: moving from A to B 映射为 co:A->B。
fn compact_git_reflog_line(line: &str) -> String {
    let s = line.trim();
    let Some((head, rest)) = s.split_once(": ") else {
        return s.to_string();
    };
    let checkout_prefix = "checkout: moving from ";
    if let Some(tail) = rest.strip_prefix(checkout_prefix) {
        if let Some((from, to)) = tail.split_once(" to ") {
            return format!("{}: co:{}->{}", head, from.trim(), to.trim());
        }
    }
    s.to_string()
}

/// 解析 shortlog 头部行（"Name (N):" 格式）。
fn parse_git_shortlog_header(line: &str) -> Option<String> {
    let s = line.trim();
    if !s.ends_with("):") {
        return None;
    }
    let open = s.rfind('(')?;
    if open == 0 {
        return None;
    }
    let name = s[..open].trim();
    let count = &s[open + 1..s.len() - 2];
    if name.is_empty() || !count.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("{}({}):", name, count))
}

/// 将 shortlog 桶冲刷为 CompactLine 记录（头部 + 消息合并）。
fn flush_git_shortlog_bucket(
    records: &mut Vec<VcsRecord>,
    shortlog_header: &mut String,
    shortlog_msgs: &mut Vec<String>,
) {
    if shortlog_header.is_empty() {
        return;
    }
    if shortlog_msgs.is_empty() {
        records.push(VcsRecord::CompactLine(shortlog_header.clone()));
    } else {
        records.push(VcsRecord::CompactLine(format!(
            "{} {}",
            shortlog_header,
            shortlog_msgs.join(" | ")
        )));
    }
    shortlog_header.clear();
    shortlog_msgs.clear();
}

/// 解析 git remote 传输记录：保留 keyword 行与 ".. ->" 行，过滤传输噪音。
pub fn parse_git_remote_transfer_records(raw: &str, keyword: &str) -> Vec<VcsRecord> {
    let mut records = Vec::new();
    let mut saw_remote = false;
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.to_ascii_lowercase().starts_with("git ") {
            continue;
        }
        // 显式过滤网络传输噪音（GIT-9）
        if is_git_transfer_noise(t) {
            continue;
        }
        if t.starts_with(keyword) && !saw_remote {
            records.push(VcsRecord::CompactLine(t.to_string()));
            saw_remote = true;
            continue;
        }
        if t.contains("..") && t.contains(" -> ") {
            records.push(VcsRecord::CompactLine(t.to_string()));
            continue;
        }
    }
    records
}

/// 解析 git pull 记录：过滤命令行与传输噪音。
pub fn parse_git_pull_records(raw: &str) -> Vec<VcsRecord> {
    let mut records = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() || t.to_ascii_lowercase().starts_with("git pull") {
            continue;
        }
        // 显式过滤网络传输噪音（GIT-9）
        if is_git_transfer_noise(t) {
            continue;
        }
        // 折叠 "Updating X..Y" 中间进度行：无尾随句号的中间进度与结尾完成行语义重复，
        // 且 range 已由上方的 "X..Y  main -> origin/main" 摘要行承载，直接丢弃避免 hash 区间
        // 三处重复造成 token 冗余；仅保留带句号的完成态行（"Updating X..Y."）。
        if let Some(rest) = t.strip_prefix("Updating ") {
            if !rest.ends_with('.') {
                continue;
            }
        }
        records.push(VcsRecord::CompactLine(t.to_string()));
    }
    records
}

// --- Git Parsers ---

pub struct GitStatusParser;
impl VcsParser for GitStatusParser {
    /// 解析 `git status` 输出：识别分支（On branch / ##）、ahead/behind 计数与未跟踪/已忽略/变更/未合并区块，将文件状态行归为记录，输出 Status 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut in_untracked = false;
        let mut in_ignored = false;
        let mut in_changes = false;
        let mut in_unmerged = false;

        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(branch) = trimmed.strip_prefix("On branch ") {
                records.push(VcsRecord::Branch(branch.trim().to_string()));
                continue;
            }
            // --- git status --branch 格式：## <branch>...<upstream> / ## ahead N / ## behind N
            if trimmed.starts_with("## ") {
                let rest = &trimmed[3..];
                if let Some(ahead_start) = rest.strip_prefix("ahead ") {
                    if let Ok(n) = ahead_start.trim().parse::<u32>() {
                        records.push(VcsRecord::CompactLine(format!("AHEAD:{}", n)));
                    }
                } else if let Some(behind_start) = rest.strip_prefix("behind ") {
                    if let Ok(n) = behind_start.trim().parse::<u32>() {
                        records.push(VcsRecord::CompactLine(format!("BEHIND:{}", n)));
                    }
                } else if let Some(pos) = rest.find("...") {
                    let branch = &rest[..pos];
                    records.push(VcsRecord::Branch(branch.to_string()));
                }
                continue;
            }
            if trimmed == "Changes not staged for commit:" {
                in_untracked = false;
                in_ignored = false;
                in_changes = true;
                in_unmerged = false;
                continue;
            }
            if trimmed == "Untracked files:" {
                in_untracked = true;
                in_ignored = false;
                in_changes = false;
                in_unmerged = false;
                continue;
            }
            if trimmed == "Ignored files:" {
                in_untracked = false;
                in_ignored = true;
                in_changes = false;
                in_unmerged = false;
                continue;
            }
            if trimmed.starts_with("(use \"")
                || trimmed.starts_with("no changes added to commit")
                || trimmed.starts_with("nothing ")
            {
                continue;
            }
            if let Some(path) = trimmed.strip_prefix("modified:") {
                records.push(VcsRecord::File {
                    status: Some("M".to_string()),
                    path: path.trim().to_string(),
                });
                continue;
            }
            if let Some(path) = trimmed.strip_prefix("new file:") {
                records.push(VcsRecord::File {
                    status: Some("A".to_string()),
                    path: path.trim().to_string(),
                });
                continue;
            }
            if let Some(path) = trimmed.strip_prefix("deleted:") {
                records.push(VcsRecord::File {
                    status: Some("D".to_string()),
                    path: path.trim().to_string(),
                });
                continue;
            }
            if let Some(path) = trimmed.strip_prefix("renamed:") {
                records.push(VcsRecord::File {
                    status: Some("R".to_string()),
                    path: path.trim().to_string(),
                });
                continue;
            }
            if let Some(path) = trimmed.strip_prefix("copied:") {
                records.push(VcsRecord::File {
                    status: Some("C".to_string()),
                    path: path.trim().to_string(),
                });
                continue;
            }
            if trimmed.starts_with("Dropped ") && trimmed.contains("refs/stash@{") {
                records.push(VcsRecord::Raw(trimmed.to_string()));
                continue;
            }
            if trimmed == "Changes to be committed:" {
                in_untracked = false;
                in_ignored = false;
                in_changes = true;
                in_unmerged = false;
                continue;
            }
            // --- git status 在 merge/rebase/cherry-pick 冲突时的 Unmerged paths 区块
            // 章节内含 both modified: / added by us: / added by them: / deleted by us: / deleted by them:
            // 五种冲突标记，必须全部按 U=Unmerged 状态保留（压缩协议符号表）
            if trimmed == "Unmerged paths:" {
                in_untracked = false;
                in_ignored = false;
                in_changes = false;
                in_unmerged = true;
                continue;
            }
            if in_ignored {
                records.push(VcsRecord::File {
                    status: Some("I".to_string()),
                    path: trimmed.to_string(),
                });
                continue;
            }
            if in_untracked {
                records.push(VcsRecord::File {
                    status: Some("?".to_string()),
                    path: trimmed.to_string(),
                });
                continue;
            }
            if in_unmerged {
                // Unmerged 区块的 5 种冲突标记 → git porcelain v1 双字符状态码
                // (LLM 期望: 必须保留冲突类型区分, 不能合并为单一 U)
                // both modified: UU, added by us: AU, added by them: UA
                // deleted by us: DU, deleted by them: UD
                let conflict_status_map = [
                    ("both modified:", "UU"),
                    ("added by us:", "AU"),
                    ("added by them:", "UA"),
                    ("deleted by us:", "DU"),
                    ("deleted by them:", "UD"),
                ];
                for (prefix, status_code) in &conflict_status_map {
                    if let Some(path) = trimmed.strip_prefix(prefix) {
                        records.push(VcsRecord::File {
                            status: Some(status_code.to_string()),
                            path: path.trim().to_string(),
                        });
                        break;
                    }
                }
                continue;
            }
            if in_changes {
                if let Some(path) = trimmed.strip_prefix("both modified:") {
                    records.push(VcsRecord::File {
                        status: Some("M".to_string()),
                        path: path.trim().to_string(),
                    });
                    continue;
                }
            }
            if let Some((status, path)) = parse_simple_status_path(trimmed) {
                records.push(VcsRecord::File {
                    status: Some(status),
                    path,
                });
                continue;
            }
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Status, records)
    }
}

pub struct GitLogParser;
impl VcsParser for GitLogParser {
    /// 解析 git log 输出，将多行 Commit 记录拍扁为单行 CompactLine
    /// 支持：标准 log、--oneline、--graph、--stat、--patch、reflog、shortlog 等
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let cmd_lower = raw
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let is_shortlog_cmd = cmd_lower.starts_with("git shortlog");
        let is_reflog_cmd = cmd_lower.starts_with("git reflog");
        let hash_candidates = collect_git_log_hash_candidates(raw);
        let hash_prefix_map = build_shortest_unique_prefix_map(&hash_candidates, 10);
        let mut in_commit = false;
        // 缓存当前 Commit 的字段
        let mut cur_hash = String::new();
        let mut cur_author = String::new();
        let mut cur_date = String::new();
        let mut cur_msg = String::new();
        let mut merge_parents = String::new();
        let mut decoration = String::new();
        let mut shortlog_header = String::new();
        let mut shortlog_msgs: Vec<String> = Vec::new();
        let mut skip_blank = false; // 跳过 Date 行后的空行

        /// 缓存中有有效 Commit 数据时刷新为 CompactLine
        fn flush_commit(
            records: &mut Vec<VcsRecord>,
            hash: &mut String,
            author: &mut String,
            date: &mut String,
            msg: &mut String,
            merge: &mut String,
            decoration: &mut String,
            hash_prefix_map: &std::collections::HashMap<String, String>,
        ) {
            if hash.is_empty() {
                return;
            }
            let short_hash = hash_prefix_map
                .get(hash)
                .map(String::as_str)
                .unwrap_or_else(|| shorten_hash(hash));
            let author_local = extract_author_local(author);
            let dt = normalize_git_datetime(date);
            let mut plain_parts: Vec<String> = vec![short_hash.to_string()];
            if !author_local.is_empty() {
                plain_parts.push(format!("@{}", author_local));
            }
            if !dt.is_empty() {
                plain_parts.push(dt.clone());
            }
            if !decoration.is_empty() {
                plain_parts.push(decoration.clone());
            }
            if !merge.is_empty() {
                plain_parts.push(format!("merge={}", merge));
            }
            if !msg.is_empty() {
                plain_parts.push(msg.clone());
            }
            records.push(VcsRecord::CompactLine(plain_parts.join(" ")));
            hash.clear();
            author.clear();
            date.clear();
            msg.clear();
            merge.clear();
            decoration.clear();
        }

        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() {
                // 空行可能是 Date 行后的空行，也可能是 message 中的空行
                if skip_blank {
                    skip_blank = false;
                    continue;
                }
                continue;
            }

            // 跳过命令行和进度噪音
            let lower = t.to_ascii_lowercase();
            if lower.starts_with("git log")
                || lower.starts_with("git reflog")
                || lower.starts_with("git shortlog")
            {
                continue;
            }

            // --graph 模式：剥离装饰前缀（| 、* 、/ 、\ ）
            let t = if t.starts_with("| ")
                || t.starts_with("* ")
                || t.starts_with("\\ ")
                || t.starts_with("/ ")
            {
                if t.len() > 2 {
                    t[2..].to_string()
                } else {
                    continue;
                }
            } else {
                t.to_string()
            };
            let t = t.trim();

            if is_shortlog_cmd {
                if let Some(header) = parse_git_shortlog_header(t) {
                    flush_git_shortlog_bucket(
                        &mut records,
                        &mut shortlog_header,
                        &mut shortlog_msgs,
                    );
                    shortlog_header = header;
                    continue;
                }
                if !shortlog_header.is_empty() {
                    shortlog_msgs.push(t.to_string());
                } else {
                    records.push(VcsRecord::CompactLine(t.to_string()));
                }
                continue;
            }

            // 新的 commit 行
            if let Some(rest) = t.strip_prefix("commit ") {
                // 先刷新上一个 commit
                flush_commit(
                    &mut records,
                    &mut cur_hash,
                    &mut cur_author,
                    &mut cur_date,
                    &mut cur_msg,
                    &mut merge_parents,
                    &mut decoration,
                    &hash_prefix_map,
                );
                in_commit = true;
                skip_blank = false;
                // 提取 hash，注意可能有 decoration 信息
                let hash_end = rest.find(' ').unwrap_or(rest.len());
                cur_hash = rest[..hash_end].to_string();
                // 检查 decoration
                let rest_after_hash = rest[hash_end..].trim();
                if let Some(dec_start) = rest_after_hash.find('(') {
                    if let Some(dec_end) = rest_after_hash.rfind(')') {
                        decoration = compact_decoration(&rest_after_hash[dec_start..=dec_end]);
                    }
                }
                continue;
            }

            // oneline 格式：hash(7+) + 空格 + message
            // 检测 oneline 模式：命令行为 --oneline 时放宽 hex 校验
            let is_oneline_mode = raw.to_ascii_lowercase().contains("--oneline");
            if !in_commit && t.len() > 8 {
                if let Some(space_pos) = t.find(' ') {
                    let hash_part = &t[..space_pos];
                    if hash_part.len() >= 6 {
                        let looks_like_hash =
                            hash_part.chars().all(|c| c.is_ascii_hexdigit()) || is_oneline_mode;
                        if looks_like_hash {
                            // 刷新上一个 oneline commit（先清空旧状态）
                            flush_commit(
                                &mut records,
                                &mut cur_hash,
                                &mut cur_author,
                                &mut cur_date,
                                &mut cur_msg,
                                &mut merge_parents,
                                &mut decoration,
                                &hash_prefix_map,
                            );
                            // 法则 A ROI 门控：oneline 格式无额外元数据时直接透传，避免 CH:/CM: 前缀膨胀
                            let msg = t[space_pos + 1..].trim().to_string();
                            let short_hash = hash_prefix_map
                                .get(hash_part)
                                .map(String::as_str)
                                .unwrap_or(hash_part);
                            records.push(VcsRecord::CompactLine(format!("{} {}", short_hash, msg)));
                            continue;
                        }
                    }
                }
            }

            // Reflog 带 hash 特殊行："<hash> HEAD@{n}: ..."
            if let Some(sp) = t.find(' ') {
                let first = &t[..sp];
                let rest = &t[sp + 1..];
                if rest.starts_with("HEAD@{") {
                    let short_hash = hash_prefix_map
                        .get(first)
                        .map(String::as_str)
                        .unwrap_or_else(|| shorten_hash(first));
                    let reflog_rest = if is_reflog_cmd {
                        compact_git_reflog_line(rest)
                    } else {
                        rest.to_string()
                    };
                    records.push(VcsRecord::CompactLine(format!(
                        "{} {}",
                        short_hash, reflog_rest
                    )));
                    continue;
                }
            }

            // Reflog 无 hash 行： "HEAD@{n}: ..."
            if t.starts_with("HEAD@{") {
                if is_reflog_cmd {
                    records.push(VcsRecord::CompactLine(compact_git_reflog_line(t)));
                } else {
                    records.push(VcsRecord::CompactLine(t.to_string()));
                }
                continue;
            }

            if !in_commit {
                continue;
            }

            // Merge: 行
            if t.starts_with("Merge: ") {
                merge_parents = t[7..].trim().replace(' ', "+");
                continue;
            }

            // Author: 行
            if t.starts_with("Author:") {
                cur_author = t.to_string();
                continue;
            }

            // Date: 行
            if let Some(_date_val) = t.strip_prefix("Date:") {
                cur_date = t.to_string();
                skip_blank = true; // Date 行后通常有空行
                continue;
            }

            // --graph 的装饰线（包含 * | / \ 等字符）
            if t.chars().all(|c| matches!(c, '*' | '|' | '/' | '\\' | ' ')) {
                continue;
            }

            // stat 行（文件变更统计）
            if t.contains(" | ") && (t.contains('+') || t.contains('-')) {
                records.push(VcsRecord::CompactLine(compact_diff_stat_line(t)));
                continue;
            }

            // 摘要行（N files changed, N insertions(+), N deletions(-)）
            if t.contains("file")
                && (t.contains("changed") || t.contains("insertion") || t.contains("deletion"))
            {
                records.push(VcsRecord::CompactLine(compact_git_summary_line(t)));
                continue;
            }

            // Reflog 特殊行
            if t.starts_with("Reflog:") || t.starts_with("HEAD@{") {
                if is_reflog_cmd {
                    records.push(VcsRecord::CompactLine(compact_git_reflog_line(t)));
                } else {
                    records.push(VcsRecord::CompactLine(t.to_string()));
                }
                continue;
            }
            // Commit message 行
            // 过滤 diff/patch 行，防止混入 commit message（如 git log --patch）
            // 覆盖：diff 头、hunk 头、文件元数据、补丁内容
            if t.starts_with("diff --git ")
                || t.starts_with("--- ")
                || t.starts_with("+++ ")
                || t.starts_with("@@ ")
                || t.starts_with("index ")
                || t.starts_with("new file mode")
                || t.starts_with("deleted file mode")
                || t.starts_with("similarity index")
                || t.starts_with("rename from")
                || t.starts_with("rename to")
                || t.starts_with("copy from")
                || t.starts_with("copy to")
                || t.starts_with("old mode")
                || t.starts_with("new mode")
                || t.starts_with('+')
                || t.starts_with('-')
            {
                continue;
            }
            if !cur_msg.is_empty() {
                cur_msg.push(' ');
            }
            cur_msg.push_str(t.trim());
        }

        // 刷新最后一个 commit
        flush_commit(
            &mut records,
            &mut cur_hash,
            &mut cur_author,
            &mut cur_date,
            &mut cur_msg,
            &mut merge_parents,
            &mut decoration,
            &hash_prefix_map,
        );
        flush_git_shortlog_bucket(&mut records, &mut shortlog_header, &mut shortlog_msgs);

        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}

pub struct GitDiffParser;
impl VcsParser for GitDiffParser {
    /// 解析 git diff 输出：diff --git 头、--stat、--name-status、--name-only
    /// 法则 F：单文件 Diff 超 MAX_DIFF_LINES（100）行时强制截断并追加 <TRUNCATED>
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        const MAX_DIFF_LINES: usize = 100;
        let mut diff_line_count: usize = 0;
        let mut in_diff_block: bool = false;
        let mut truncated: bool = false;
        let cmd_lower = raw.lines().next().unwrap_or("").trim().to_ascii_lowercase();
        let is_name_only = cmd_lower.contains("--name-only");
        let is_name_status = cmd_lower.contains("--name-status");

        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.to_ascii_lowercase().starts_with("git diff") {
                continue;
            }

            // diff --git 头行 → DIFF 标准化，重置 diff 行计数器
            if t.starts_with("diff --git ") {
                // 提交流程：如果上一个 diff block 被截断，追加 <TRUNCATED>
                if truncated {
                    records.push(VcsRecord::CompactLine("<TRUNCATED>".to_string()));
                    truncated = false;
                }
                diff_line_count = 0;
                in_diff_block = true;
                // 提取路径: "diff --git a/path b/path" → "DIFF:path"
                if let Some(b_pos) = t.find(" b/") {
                    let path = t[b_pos + 3..].trim().to_string();
                    records.push(VcsRecord::CompactLine(format!("DIFF://{}", path)));
                }
                continue;
            }

            // 跳过 index 行
            if t.starts_with("index ") {
                continue;
            }

            // --name-status 模式：单字符状态 + 路径
            if is_name_status || is_name_only {
                let parts: Vec<&str> = t.split_whitespace().collect();
                if parts.len() == 2 && parts[0].len() == 1 {
                    if let Some(status) = parts[0].chars().next() {
                        if status.is_ascii_uppercase() {
                            records.push(VcsRecord::File {
                                status: Some(status.to_string()),
                                path: parts[1].to_string(),
                            });
                            continue;
                        }
                    }
                } else if parts.len() == 3 && parts[0].starts_with('R') {
                    records.push(VcsRecord::File {
                        status: Some("R".to_string()),
                        path: format!("{} -> {}", parts[1], parts[2]),
                    });
                    continue;
                } else if is_name_only && parts.len() == 1 {
                    records.push(VcsRecord::File {
                        status: None,
                        path: t.to_string(),
                    });
                    continue;
                }
            }

            // --stat 摘要行
            if t.contains("file")
                && (t.contains("changed") || t.contains("insertion") || t.contains("deletion"))
            {
                records.push(VcsRecord::CompactLine(compact_git_summary_line(t)));
                continue;
            }

            // 统计行：path | N +++ ---
            if t.contains(" | ") && (t.contains('+') || t.contains('-')) {
                records.push(VcsRecord::CompactLine(compact_diff_stat_line(t)));
                continue;
            }

            // 二进制文件差异
            if t.starts_with("Binary files ") {
                records.push(VcsRecord::CompactLine(t.to_string()));
                continue;
            }

            // --- / +++ 行计入 diff 行数但不输出（法则 E：降维为 DIFF://<路径>）
            // @@ 行保留为 hunk 信息
            if t.starts_with("--- ") || t.starts_with("+++ ") {
                diff_line_count += 1;
                if diff_line_count > MAX_DIFF_LINES {
                    truncated = true;
                }
                continue;
            }
            if t.starts_with("@@ ") {
                diff_line_count += 1;
                if diff_line_count > MAX_DIFF_LINES {
                    truncated = true;
                    continue;
                }
                records.push(VcsRecord::CompactLine(t.to_string()));
                continue;
            }

            // 补丁内容行（计入 diff 行数）
            if in_diff_block {
                diff_line_count += 1;
                if diff_line_count > MAX_DIFF_LINES {
                    truncated = true;
                    continue;
                }
            }
            records.push(VcsRecord::CompactLine(t.to_string()));
        }
        // 法则 F：如果最后一块 diff 被截断，追加 <TRUNCATED>
        if truncated {
            records.push(VcsRecord::CompactLine("<TRUNCATED>".to_string()));
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Diff, records)
    }
}

// ... 以此类推集成其它 Git 特有 Parser，无需改动其核心行为
pub struct GitAddParser;
impl VcsParser for GitAddParser {
    /// 解析 `git add` 输出：将 `add 'path'`/`new file: path` 等归为 A 状态文件（HashSet 去重），交互模式 `-p` 下提取 diff 路径，输出 Status 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        // HashSet 去重：同一路径的相同状态只保留一次
        let mut seen: std::collections::HashSet<(char, String)> = std::collections::HashSet::new();
        // 交互模式检测（git add -p 等）
        let is_interactive = raw.to_ascii_lowercase().contains(" -p")
            || raw.to_ascii_lowercase().contains(" --patch");
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty()
                || t.starts_with("git add")
                || t == "Changes to be committed:"
                || t.starts_with("(use \"git")
            {
                continue;
            }
            // 交互模式：处理 diff 和提示行
            if is_interactive {
                if t.starts_with("Stage this hunk") || t.starts_with("(1/") {
                    continue;
                }
                if t.starts_with("diff --git ") {
                    if let Some(b_pos) = t.find(" b/") {
                        let path = t[b_pos + 3..].trim().to_string();
                        records.push(VcsRecord::CompactLine(format!("DIFF://{}", path)));
                    }
                    continue;
                }
                if t.starts_with("--- ")
                    || t.starts_with("+++ ")
                    || t.starts_with("index ")
                    || t.starts_with("@@ ")
                {
                    continue;
                }
                if t.starts_with('+') || t.starts_with('-') {
                    records.push(VcsRecord::CompactLine(t.to_string()));
                    continue;
                }
            }
            if let Some(path) = t.strip_prefix("add '") {
                if let Some(p) = path.strip_suffix('\'') {
                    if seen.insert(('A', p.to_string())) {
                        records.push(VcsRecord::File {
                            status: Some("A".to_string()),
                            path: p.to_string(),
                        });
                    }
                    continue;
                }
            }
            if let Some(path) = t.strip_prefix("Adding file: ") {
                if seen.insert(('A', path.to_string())) {
                    records.push(VcsRecord::File {
                        status: Some("A".to_string()),
                        path: path.to_string(),
                    });
                }
                continue;
            }
            if let Some(path) = t.strip_prefix("new file:") {
                if seen.insert(('A', path.trim().to_string())) {
                    records.push(VcsRecord::File {
                        status: Some("A".to_string()),
                        path: path.trim().to_string(),
                    });
                }
                continue;
            }
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Status, records)
    }
}
pub struct GitRmParser;
impl VcsParser for GitRmParser {
    /// 解析 git rm 输出：
    /// "rm 'path'" → File{D, path} / "deleted: path" → File{D, path}
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        // HashSet 去重：同一路径的相同状态只保留一次
        let mut seen: std::collections::HashSet<(char, String)> = std::collections::HashSet::new();
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.to_ascii_lowercase().starts_with("git rm") {
                continue;
            }
            // 过滤 hint/advice
            if is_git_hint_line(t) {
                continue;
            }
            // "rm 'path'" 格式
            if let Some(rest) = t.strip_prefix("rm '") {
                if let Some(path) = rest.strip_suffix('\'') {
                    if seen.insert(('D', path.to_string())) {
                        records.push(VcsRecord::File {
                            status: Some("D".to_string()),
                            path: path.to_string(),
                        });
                    }
                    continue;
                }
            }
            // "remove 'path'" 格式
            if let Some(rest) = t.strip_prefix("remove '") {
                if let Some(path) = rest.strip_suffix('\'') {
                    if seen.insert(('D', path.to_string())) {
                        records.push(VcsRecord::File {
                            status: Some("D".to_string()),
                            path: path.to_string(),
                        });
                    }
                    continue;
                }
            }
            // "deleted: path" 格式
            if let Some(path) = t.strip_prefix("deleted:") {
                if seen.insert(('D', path.trim().to_string())) {
                    records.push(VcsRecord::File {
                        status: Some("D".to_string()),
                        path: path.trim().to_string(),
                    });
                }
                continue;
            }
            records.push(VcsRecord::CompactLine(t.to_string()));
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Status, records)
    }
}
pub struct GitTagParser;
impl VcsParser for GitTagParser {
    /// 解析 git tag 输出：tag 列表或 tag 创建消息
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.to_ascii_lowercase().starts_with("git tag") {
                continue;
            }
            records.push(VcsRecord::CompactLine(t.to_string()));
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}
/// git remote -v 单行解析结果：remote 名 + URL + 角色(fetch/push)。
enum GitRemoteRow {
    /// 形如 `name url (fetch)`，同 name+url 的 fetch/push 行可合并。
    Remote(String, String, Vec<String>),
    /// 其它行（如 `git remote` 裸列表）：原样保留。
    Plain(String),
}

/// 解析 `name url (role)` 单行；无法识别返回 None。
fn parse_git_remote_row(s: &str) -> Option<(String, String, String)> {
    let s = s.trim();
    let rest = s.strip_suffix(')')?.trim_end();
    let (head, role) = rest.rsplit_once('(')?;
    let role = role.trim();
    let mut it = head.split_whitespace();
    let name = it.next()?;
    let url: Vec<&str> = it.collect();
    if url.is_empty() {
        return None;
    }
    Some((name.to_string(), url.join(" "), role.to_string()))
}

pub struct GitRemoteParser;
impl VcsParser for GitRemoteParser {
    /// 解析 git remote -v 输出：
    /// - 合并同一 remote 同 name+url 的 `(fetch)/(push)` 两行为单行 `name url (fetch/push)`
    /// - 其余行压缩连续空白为单空格
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut combined: Vec<GitRemoteRow> = Vec::new();
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.to_ascii_lowercase().starts_with("git remote") {
                continue;
            }
            let compacted = t.split_whitespace().collect::<Vec<_>>().join(" ");
            if let Some((name, url, role)) = parse_git_remote_row(&compacted) {
                // 若已存在同 name+url 的 Remote 行，追加角色；否则新建
                let found = combined.iter_mut().find_map(|item| match item {
                    GitRemoteRow::Remote(n, u, roles) if *n == name && *u == url => Some(roles),
                    _ => None,
                });
                match found {
                    Some(roles) => {
                        if !roles.contains(&role) {
                            roles.push(role);
                        }
                    }
                    None => combined.push(GitRemoteRow::Remote(name, url, vec![role])),
                }
            } else {
                combined.push(GitRemoteRow::Plain(compacted));
            }
        }
        let records: Vec<VcsRecord> = combined
            .into_iter()
            .map(|item| match item {
                GitRemoteRow::Remote(name, url, roles) => {
                    VcsRecord::CompactLine(format!("{} {} ({})", name, url, roles.join("/")))
                }
                GitRemoteRow::Plain(s) => VcsRecord::CompactLine(s),
            })
            .collect();
        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}

// --- Git Merge / Rebase / Bisect 扩展 ---
pub struct GitMergeParser;
impl VcsParser for GitMergeParser {
    /// 解析 `git merge` 输出：过滤 hint 教学行、标准化错误（fatal/error/CONFLICT），提取 `Auto-merging`/`Merge made by` 等，输出 Log 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.to_ascii_lowercase().starts_with("git merge") {
                continue;
            }
            // 过滤 hint/advice 教学废话
            if is_git_hint_line(trimmed) {
                continue;
            }
            // 错误标准化
            if trimmed.starts_with("fatal:")
                || trimmed.starts_with("error:")
                || trimmed.starts_with("CONFLICT")
            {
                records.push(VcsRecord::CompactLine(normalize_error_line(trimmed)));
                continue;
            }
            if trimmed.starts_with("Merge made by the '") {
                records.push(VcsRecord::CompactLine(trimmed.to_string()));
                continue;
            }
            if let Some(path) = trimmed.strip_prefix("Auto-merging ") {
                records.push(VcsRecord::CompactLine(format!(
                    "Auto-merging {}",
                    path.trim()
                )));
                continue;
            }
            if trimmed.starts_with("Already up to date") || trimmed.starts_with("Updating ") {
                records.push(VcsRecord::CompactLine(trimmed.to_string()));
                continue;
            }
            records.push(VcsRecord::CompactLine(trimmed.to_string()));
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}

pub struct GitRebaseParser;
impl VcsParser for GitRebaseParser {
    /// 解析 `git rebase` 输出：过滤 hint 与 `#` 教学注释行，压缩 `Applying:`/`Successfully rebased` 等，输出 Log 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.to_ascii_lowercase().starts_with("git rebase") {
                continue;
            }
            // 过滤 hint/advice 教学废话
            if is_git_hint_line(trimmed) {
                continue;
            }
            // 过滤 rebase -i 教学注释行（# Rebase ..., # Commands:, # p, pick = use commit ...）
            if trimmed.starts_with('#') {
                continue;
            }
            // 错误标准化
            if trimmed.starts_with("fatal:")
                || trimmed.starts_with("error:")
                || trimmed.starts_with("CONFLICT")
            {
                records.push(VcsRecord::CompactLine(normalize_error_line(trimmed)));
                continue;
            }
            // 压缩 rebase 输出
            if let Some(msg) = trimmed.strip_prefix("Applying: ") {
                records.push(VcsRecord::CompactLine(format!("AP:{}", msg.trim())));
                continue;
            }
            if trimmed.starts_with("Successfully rebased") {
                records.push(VcsRecord::CompactLine("OK:rebase".to_string()));
                continue;
            }
            if trimmed.starts_with("Current branch")
                || trimmed.starts_with("First, rewinding")
                || trimmed.starts_with("Auto-merging")
                || trimmed.starts_with("Using index info")
                || trimmed.starts_with("Falling back")
            {
                records.push(VcsRecord::CompactLine(trimmed.to_string()));
                continue;
            }
            records.push(VcsRecord::CompactLine(trimmed.to_string()));
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}

pub struct GitFetchParser;
impl VcsParser for GitFetchParser {
    /// 解析 `git fetch` 输出：委托远端传输记录解析（以 `From ` 识别源），输出 Log 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let records = parse_git_remote_transfer_records(raw, "From ");
        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}

pub struct GitPushParser;
impl VcsParser for GitPushParser {
    /// 解析 `git push` 输出：委托远端传输记录解析（以 `To ` 识别目标），输出 Log 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let records = parse_git_remote_transfer_records(raw, "To ");
        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}

pub struct GitPullParser;
impl VcsParser for GitPullParser {
    /// 解析 `git pull` 输出：委托 pull 记录解析，将拉取结果归并为 Log 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let records = parse_git_pull_records(raw);
        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}

pub struct GitResetParser;
impl VcsParser for GitResetParser {
    /// 解析 `git reset` 输出：将 `Commit 'hash'` 归为 Commit 记录，其余状态行归为文件记录，输出 Log 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with("git reset") {
                continue;
            }
            if let Some(rest) = t.strip_prefix("Commit '") {
                let commit = rest.split('\'').next().unwrap_or("").to_string();
                records.push(VcsRecord::Commit(commit));
                continue;
            } else if t.starts_with("Unstaged changes after reset:") {
                continue;
            } else if let Some((status, path)) = parse_simple_status_path(t) {
                records.push(VcsRecord::File {
                    status: Some(status),
                    path,
                });
                continue;
            }
            records.push(VcsRecord::Raw(t.to_string()));
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}

pub struct GitRestoreParser;
impl VcsParser for GitRestoreParser {
    /// 解析 `git restore` 输出：将 `Restored path:`/`Discarded changes in` 归为单行记录，输出 Status 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with("git restore") {
                continue;
            }
            if let Some(path) = t.strip_prefix("Restored path:") {
                records.push(VcsRecord::CompactLine(format!("Restored: {}", path.trim())));
                continue;
            }
            if let Some(path) = t.strip_prefix("Discarded changes in") {
                records.push(VcsRecord::CompactLine(format!(
                    "Discarded: {}",
                    path.trim()
                )));
                continue;
            }
            records.push(VcsRecord::Raw(t.to_string()));
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Status, records)
    }
}

pub struct GitSwitchParser;
impl VcsParser for GitSwitchParser {
    /// 解析 git switch 输出，标准化分支状态
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.to_ascii_lowercase().starts_with("git switch") {
                continue;
            }
            if let Some(rest) = t.strip_prefix("Switched to branch '") {
                let branch = rest.strip_suffix('\'').unwrap_or(rest);
                records.push(VcsRecord::Branch(branch.to_string()));
                continue;
            }
            if let Some(rest) = t.strip_prefix("Switched to a new branch '") {
                let branch = rest.strip_suffix('\'').unwrap_or(rest);
                records.push(VcsRecord::Branch(format!("*{}", branch)));
                continue;
            }
            // 分离 HEAD 状态标准化
            if t.starts_with("Previous HEAD position was") {
                // "Previous HEAD position was abc1234 Old feature" → BR:prev@abc1234
                if let Some(rest) = t.strip_prefix("Previous HEAD position was ") {
                    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                    if parts.len() >= 1 {
                        records.push(VcsRecord::CompactLine(format!("BR:prev@{}", parts[0])));
                        continue;
                    }
                }
                records.push(VcsRecord::CompactLine(t.to_string()));
                continue;
            }
            if t.starts_with("HEAD is now at") {
                records.push(VcsRecord::CompactLine(t.to_string()));
                continue;
            }
            records.push(VcsRecord::CompactLine(t.to_string()));
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Status, records)
    }
}

pub struct GitBisectParser;
impl VcsParser for GitBisectParser {
    /// 解析 git bisect 输出：
    /// branch 行提取 BR: 前缀，其余 CompactLine 透传
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.to_ascii_lowercase().starts_with("git bisect") {
                continue;
            }
            if is_git_hint_line(t) {
                continue;
            }
            // 提取 branch 信息
            if t.starts_with("Bisecting: ") || t.starts_with("We are now on branch ") {
                if let Some(branch) = t.split("branch '").nth(1) {
                    if let Some(name) = branch.strip_suffix('\'') {
                        records.push(VcsRecord::CompactLine(format!("BR:{}", name)));
                        continue;
                    }
                }
                // Bisecting 模板行压缩：折叠重复模板文字，仅保留剩余修订数 N 与预估步数 M
                records.push(VcsRecord::CompactLine(compact_bisecting_line(t)));
                continue;
            }
            if t.starts_with("HEAD is now at") {
                records.push(VcsRecord::CompactLine(t.to_string()));
                continue;
            }
            // 提取 bisect 状态
            if t.contains("remaining") || t.contains("steps") {
                records.push(VcsRecord::CompactLine(t.to_string()));
                continue;
            }
            records.push(VcsRecord::CompactLine(t.to_string()));
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}

/// 压缩 `Bisecting: N revisions left to test after this (roughly M steps)` 模板行。
///
/// 该行是 git bisect 的固定文案，重复出现，仅剩余修订数 N 与预估步数 M 是
/// 决策相关数字；折叠模板文字仅保留两者以去冗余。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_bisecting_line(line: &str) -> String {
    let body = line.strip_prefix("Bisecting: ").unwrap_or(line);
    // 剩余修订数：第一个独立的十进制数字
    let n = body.split_whitespace().find_map(|w| w.parse::<u64>().ok());
    // 预估步数：`roughly` 后的第一个十进制数字
    let m = body
        .split("roughly ")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|w| w.parse::<u64>().ok());
    match (n, m) {
        (Some(n), Some(m)) => format!("Bisecting: {} left (~{})", n, m),
        // 无法安全提取两个数字时不改写，保留原文
        _ => line.to_string(),
    }
}

pub struct GitCleanParser;
impl VcsParser for GitCleanParser {
    /// 解析 git clean 输出：
    /// "Removing path" / "Would remove path" → CompactLine ST:D/ST:? path
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.to_ascii_lowercase().starts_with("git clean") {
                continue;
            }
            if is_git_hint_line(t) {
                continue;
            }
            if let Some(path) = t.strip_prefix("Removing ") {
                records.push(VcsRecord::CompactLine(format!("D {}", path.trim())));
                continue;
            }
            if let Some(path) = t.strip_prefix("Would remove ") {
                records.push(VcsRecord::CompactLine(format!("? {}", path.trim())));
                continue;
            }
            if t.starts_with("Dry run complete") {
                continue; // 过滤教学 hint
            }
            if t.starts_with("Removed ") {
                records.push(VcsRecord::CompactLine(t.to_string()));
                continue;
            }
            records.push(VcsRecord::CompactLine(t.to_string()));
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Status, records)
    }
}

pub struct GitSubmoduleParser;
impl VcsParser for GitSubmoduleParser {
    /// 解析 git submodule 输出，限制 commit message 行数（GIT-6）
    /// Submodule 路径将被 pipeline 中的路径字典压缩为 $P Token
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut submodule_msg_count = 0;
        let max_sub_msg = 5; // GIT-6: 最多保留 5 行 submodule 的 commit message
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.to_ascii_lowercase().starts_with("git submodule") {
                continue;
            }
            if is_git_hint_line(t) {
                continue;
            }
            if t.starts_with("Submodule ") {
                submodule_msg_count = 0;
                // "Submodule path 'libs/foo': checked out 'abc1234'"
                if let Some(rest) = t.strip_prefix("Submodule path '") {
                    if let Some((path, rest)) = rest.split_once("': checked out '") {
                        let hash = rest.trim_end_matches('\'');
                        records.push(VcsRecord::CompactLine(format!("{} @{}", path, hash)));
                        continue;
                    }
                }
                // "Submodule 'libs/foo' (https://...)"
                if let Some(rest) = t.strip_prefix("Submodule '") {
                    if let Some(end) = rest.find('\'') {
                        let path = &rest[..end];
                        let url = rest[end..].trim().trim_start_matches('\'');
                        records.push(VcsRecord::CompactLine(format!("{} {}", path, url)));
                        continue;
                    }
                }
                records.push(VcsRecord::CompactLine(t.to_string()));
                continue;
            }
            // Submodule commit message 行（以 > 开头的缩进行）
            if t.starts_with(">") || t.starts_with("<") {
                submodule_msg_count += 1;
                if submodule_msg_count > max_sub_msg {
                    if submodule_msg_count == max_sub_msg + 1 {
                        records.push(VcsRecord::CompactLine("<TRUNCATED>".to_string()));
                    }
                    continue;
                }
                records.push(VcsRecord::CompactLine(t.to_string()));
                continue;
            }
            records.push(VcsRecord::CompactLine(t.to_string()));
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}

pub struct GitCherryPickParser;
impl VcsParser for GitCherryPickParser {
    /// 解析 git cherry-pick 输出：
    /// 网络传输行（From/To + ref 更新）、CONFLICT 行、hint 行过滤
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut saw_remote = false;
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.to_ascii_lowercase().starts_with("git cherry-pick") {
                continue;
            }
            // 过滤 hint/advice 教学废话
            if is_git_hint_line(t) {
                continue;
            }
            // 过滤网络传输噪音
            if is_git_transfer_noise(t) {
                continue;
            }
            // 错误标准化
            if t.starts_with("fatal:") || t.starts_with("error:") || t.starts_with("CONFLICT") {
                records.push(VcsRecord::CompactLine(normalize_error_line(t)));
                continue;
            }
            // From/To 远程地址（仅首次）
            if (t.starts_with("From ") || t.starts_with("To ")) && !saw_remote {
                records.push(VcsRecord::CompactLine(t.to_string()));
                saw_remote = true;
                continue;
            }
            // Ref 更新行（包含 .. 和 ->）
            if t.contains("..") && t.contains(" -> ") {
                records.push(VcsRecord::CompactLine(t.to_string()));
                continue;
            }
            // Cherry-pick 特有的 [branch hash] 格式
            if t.starts_with("[") && t.contains("] ") {
                records.push(VcsRecord::CompactLine(t.to_string()));
                continue;
            }
            // Auto-merging
            if let Some(path) = t.strip_prefix("Auto-merging ") {
                records.push(VcsRecord::CompactLine(format!(
                    "Auto-merging {}",
                    path.trim()
                )));
                continue;
            }
            // 其他有意义行
            if t.starts_with("Finished one cherry-pick")
                || t.starts_with("The previous cherry-pick")
                || t.starts_with("After resolving")
                || t.starts_with("On branch")
            {
                records.push(VcsRecord::CompactLine(t.to_string()));
                continue;
            }
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}

pub struct GitRevertParser;
impl VcsParser for GitRevertParser {
    /// 解析 git revert 输出：revert commit 信息、变更摘要
    ///
    /// 将三段等价描述合并为两行保语义：保留 `Reverting commit` / `Reverted commit`
    /// 短语，删除冗余的整段全 hash 行与 "completed successfully" 样板句，并把日期/作者
    /// /parent 折叠进 `Reverted commit` 行。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        // revert 缓冲：跨行累积冗余信息，待回填到合并行
        let mut committed_info: Option<String> = None; // "20260401 by alice.chen"
                                                       // parent 出现在 "Reverted commit" 之后，需预扫描提前捕获
        let mut parent: Option<String> = None;
        for l in raw.lines() {
            if let Some(p) = l.split("with parent ").nth(1) {
                if let Some(pid) = p.split_whitespace().next() {
                    parent = Some(pid.to_string());
                }
            }
        }
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.to_ascii_lowercase().starts_with("git revert") {
                continue;
            }
            if is_git_hint_line(t) {
                continue;
            }
            // 标准化错误/fatal
            if t.starts_with("fatal:") || t.starts_with("error:") {
                records.push(VcsRecord::CompactLine(normalize_error_line(t)));
                continue;
            }
            // 起始行：Reverting commit <short>——保留短语本身，等待后续补充信息
            if let Some(short) = t.strip_prefix("Reverting commit ") {
                let short = short.trim_end_matches(':').trim();
                records.push(VcsRecord::CompactLine(format!(
                    "Reverting commit {}",
                    short
                )));
                continue;
            }
            // 冗余描述行：This reverts commit <full hash>——与 <short> 语义重复，丢弃
            if t.starts_with("This reverts commit ") || t.starts_with("This reverts the commit ") {
                continue;
            }
            // 提交时间/作者行：折叠进合并行（日期符号化去 '-'）
            if let Some(rest) = t.strip_prefix("which was originally committed on ") {
                committed_info = Some(rest.trim().replace('-', ""));
                continue;
            }
            // 自动 revert 样板句：parent 已预扫描，该行整句冗余，直接丢弃
            if t.starts_with("Automatic revert of commit") {
                continue;
            }
            // 结束行：Reverted commit <short>——把日期/作者/parent 折叠成一行输出
            if let Some(short) = t.strip_prefix("Reverted commit ") {
                let short = short.trim_end_matches('.').trim();
                let mut line = format!("Reverted commit {}", short);
                if let Some(ci) = committed_info.take() {
                    let mut extra = format!(" (was {}", ci);
                    if let Some(p) = parent.take() {
                        extra.push_str(&format!("; parent {}", p));
                    }
                    extra.push(')');
                    line.push_str(&extra);
                } else if let Some(p) = parent.take() {
                    line.push_str(&format!(" (parent {})", p));
                }
                records.push(VcsRecord::CompactLine(line));
                continue;
            }
            // 摘要行
            if t.contains("file")
                && (t.contains("changed") || t.contains("insertion") || t.contains("deletion"))
            {
                records.push(VcsRecord::CompactLine(compact_git_summary_line(t)));
                continue;
            }
            // 统计行
            if t.contains(" | ") && (t.contains('+') || t.contains('-')) {
                records.push(VcsRecord::CompactLine(compact_diff_stat_line(t)));
                continue;
            }
            records.push(VcsRecord::CompactLine(t.to_string()));
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}

pub struct GitBranchParser;
impl VcsParser for GitBranchParser {
    /// 解析 git branch 输出：
    /// "* master" → Branch("*master") / "  feature" → CompactLine
    /// "remotes/origin/HEAD -> origin/master" → CompactLine
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.to_ascii_lowercase().starts_with("git branch") {
                continue;
            }
            if is_git_hint_line(t) {
                continue;
            }
            // 当前活跃分支：* 开头
            if t.starts_with("* ") {
                // 分支列表场景保留星标，但不引入 BR: 标签，避免 *BR:main 这种别扭输出
                records.push(VcsRecord::CompactLine(format!(
                    "* {}",
                    collapse_multi_spaces(&t[2..].trim())
                )));
                continue;
            }
            // 普通分支行：折叠列对齐的多空格（branch -v 等多列场景），不丢任何语义 token
            records.push(VcsRecord::CompactLine(collapse_multi_spaces(&t)));
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}

pub struct GitCheckoutParser;
impl VcsParser for GitCheckoutParser {
    /// 解析 `git checkout` 输出：识别 `Switched to branch`/分离 HEAD，将分支切换归为 Branch 记录，输出 Status 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.to_ascii_lowercase().starts_with("git checkout") {
                continue;
            }
            if is_git_hint_line(t) {
                continue;
            }
            // 分支切换
            if let Some(rest) = t.strip_prefix("Switched to branch '") {
                let branch = rest.strip_suffix('\'').unwrap_or(rest);
                records.push(VcsRecord::Branch(branch.to_string()));
                continue;
            }
            if let Some(rest) = t.strip_prefix("Switched to a new branch '") {
                let branch = rest.strip_suffix('\'').unwrap_or(rest);
                records.push(VcsRecord::Branch(format!("*{}", branch)));
                continue;
            }
            // 分离 HEAD
            if t.starts_with("Previous HEAD position was") {
                if let Some(rest) = t.strip_prefix("Previous HEAD position was ") {
                    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                    if parts.len() >= 1 {
                        records.push(VcsRecord::CompactLine(format!("BR:prev@{}", parts[0])));
                        continue;
                    }
                }
            }
            if t.starts_with("HEAD is now at") {
                records.push(VcsRecord::CompactLine(t.to_string()));
                continue;
            }
            records.push(VcsRecord::CompactLine(t.to_string()));
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Status, records)
    }
}

/// 刷新 git show 当前 commit 缓存为单行记录
#[tracing::instrument(level = "debug", skip_all)]
fn flush_git_show_commit(
    records: &mut Vec<VcsRecord>,
    current_hash: &mut String,
    current_author: &mut String,
    current_date: &mut String,
    current_message_parts: &mut Vec<String>,
    hash_prefix_map: &std::collections::HashMap<String, String>,
) {
    if current_hash.is_empty() {
        return;
    }
    let message = current_message_parts.join(" ").trim().to_string();
    let short_hash = hash_prefix_map
        .get(current_hash)
        .map(String::as_str)
        .unwrap_or_else(|| shorten_hash(current_hash));
    let mut parts = vec![short_hash.to_string()];
    if !current_author.is_empty() {
        parts.push(format!("@{}", current_author));
    }
    if !current_date.is_empty() {
        parts.push(current_date.clone());
    }
    if !message.is_empty() {
        parts.push(message);
    }
    records.push(VcsRecord::CompactLine(parts.join(" ")));
    current_hash.clear();
    current_author.clear();
    current_date.clear();
    current_message_parts.clear();
}

pub struct GitShowParser;
impl VcsParser for GitShowParser {
    /// 解析 `git show` 输出：收集提交哈希并构建最短唯一前缀，将提交信息（hash/author/date/message）归并为单行记录，输出 Show 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let hash_candidates = collect_git_log_hash_candidates(raw);
        let hash_prefix_map = build_shortest_unique_prefix_map(&hash_candidates, 10);
        let mut current_hash = String::new();
        let mut current_author = String::new();
        let mut current_date = String::new();
        let mut current_message_parts: Vec<String> = Vec::new();

        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.to_ascii_lowercase().starts_with("git show") {
                continue;
            }
            if let Some(rest) = t.strip_prefix("commit ") {
                flush_git_show_commit(
                    &mut records,
                    &mut current_hash,
                    &mut current_author,
                    &mut current_date,
                    &mut current_message_parts,
                    &hash_prefix_map,
                );
                let hash_end = rest.find(' ').unwrap_or(rest.len());
                let hash = &rest[..hash_end];
                current_hash = hash.to_string();
                continue;
            }
            if t.starts_with("Author:") {
                current_author = extract_author_local(t);
                continue;
            }
            if t.starts_with("Date:") {
                current_date = normalize_git_datetime(t);
                continue;
            }
            if t.starts_with("diff --git ") {
                flush_git_show_commit(
                    &mut records,
                    &mut current_hash,
                    &mut current_author,
                    &mut current_date,
                    &mut current_message_parts,
                    &hash_prefix_map,
                );
                if let Some(b_pos) = t.find(" b/") {
                    let path = t[b_pos + 3..].trim().to_string();
                    records.push(VcsRecord::CompactLine(format!("DIFF://{}", path)));
                }
                continue;
            }
            // 法则 E：降维为 DIFF://<路径> 后抑制随后的 ---/+++ 和 index 行
            if t.starts_with("--- ") || t.starts_with("+++ ") || t.starts_with("index ") {
                continue;
            }
            if !current_hash.is_empty() && !t.starts_with("@@ ") {
                current_message_parts.push(t.to_string());
                continue;
            }
            records.push(VcsRecord::CompactLine(t.to_string()));
        }
        flush_git_show_commit(
            &mut records,
            &mut current_hash,
            &mut current_author,
            &mut current_date,
            &mut current_message_parts,
            &hash_prefix_map,
        );
        to_doc_if_any(VcsTool::Git, VcsDocKind::Show, records)
    }
}

pub struct GitBlameParser;
impl VcsParser for GitBlameParser {
    /// 解析 git blame 输出，对连续相同 hash+author 的行进行去重压缩
    /// 首现完整：CH:hash OW:@author DT:date lineno code
    /// 同 hash 去重：^ lineno code
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let hash_candidates = collect_git_blame_hash_candidates(raw);
        let hash_prefix_map = build_shortest_unique_prefix_map(&hash_candidates, 10);
        let mut prev_hash = String::new();
        let mut prev_author = String::new();

        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.to_ascii_lowercase().starts_with("git blame") {
                continue;
            }

            // 解析 blame 行格式: <hash> (author date time tz lineno) code
            // 例: "8b3c2d1e (alice.chen 2026-04-03 14:32:11 +0800  1) fn main()"
            let (hash, author, date, lineno, code) = if let Some(space_pos) = t.find(' ') {
                let hash_part = &t[..space_pos];
                let rest = t[space_pos + 1..].trim();
                if rest.starts_with('(') {
                    if let Some(paren_end) = rest.find(')') {
                        let meta = &rest[1..paren_end]; // author ... 2026-04-03 14:32:11 +0800  1
                        let code = rest[paren_end + 1..].trim().to_string();
                        let mut parts: Vec<&str> = meta.split_whitespace().collect();
                        if parts.len() >= 4 {
                            let lineno = parts.pop().unwrap_or("").to_string();
                            if parts
                                .last()
                                .map(|tz| {
                                    tz.len() == 5
                                        && (tz.starts_with('+') || tz.starts_with('-'))
                                        && tz[1..].chars().all(|c| c.is_ascii_digit())
                                })
                                .unwrap_or(false)
                            {
                                let _ = parts.pop();
                            }
                            let time = parts.pop().unwrap_or("");
                            let date_only = parts.pop().unwrap_or("");
                            let author = parts.join(" ");
                            let date = format!("{} {}", date_only, time);
                            (hash_part.to_string(), author, date, lineno, code)
                        } else {
                            records.push(VcsRecord::Raw(t.to_string()));
                            continue;
                        }
                    } else {
                        records.push(VcsRecord::Raw(t.to_string()));
                        continue;
                    }
                } else {
                    records.push(VcsRecord::Raw(t.to_string()));
                    continue;
                }
            } else {
                records.push(VcsRecord::Raw(t.to_string()));
                continue;
            };

            // 邮箱降维
            let compact_author = if let Some(at_pos) = author.find('@') {
                author[..at_pos].to_string()
            } else {
                author.clone()
            };

            if hash == prev_hash && compact_author == prev_author {
                // 去重：仅保留 ^ 标记
                // 空行守卫：若 code 为空，保留 <BLANK> 占位符
                let display_code = if code.trim().is_empty() {
                    "<BLANK>".to_string()
                } else {
                    code
                };
                records.push(VcsRecord::CompactLine(format!(
                    "^ {} {}",
                    lineno, display_code
                )));
            } else {
                // 首现：完整保留
                let display_code = if code.trim().is_empty() {
                    "<BLANK>".to_string()
                } else {
                    code
                };
                records.push(VcsRecord::CompactLine(format!(
                    "{} @{} {} {} {}",
                    hash_prefix_map
                        .get(&hash)
                        .map(String::as_str)
                        .unwrap_or_else(|| shorten_hash(&hash)),
                    compact_author,
                    date,
                    lineno,
                    display_code
                )));
                prev_hash = hash;
                prev_author = compact_author;
            }
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Show, records)
    }
}

pub struct GitStashParser;
impl VcsParser for GitStashParser {
    /// 解析 `git stash` 输出：将 `Saved working directory...` 归为 Subject、diffstat 行压缩，其余回退给 `GitStatusParser` 复用，输出 Log 类文档。
    fn parse(&self, raw: &str) -> Option<VcsDocument> {
        let mut records = Vec::new();
        let mut mock_status = String::new();
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with("git stash") {
                continue;
            }
            if t.starts_with("Saved working directory and index state") {
                records.push(VcsRecord::Subject(t.to_string()));
            } else if looks_like_diff_stat_line(t) {
                records.push(VcsRecord::CompactLine(compact_diff_stat_line(t)));
            } else if t.contains("file")
                && (t.contains("changed") || t.contains("insertion") || t.contains("deletion"))
            {
                records.push(VcsRecord::CompactLine(compact_git_summary_line(t)));
            } else {
                mock_status.push_str(line);
                mock_status.push('\n');
            }
        }
        if let Some(doc) = GitStatusParser.parse(&mock_status) {
            records.extend(doc.records);
        }
        to_doc_if_any(VcsTool::Git, VcsDocKind::Log, records)
    }
}
