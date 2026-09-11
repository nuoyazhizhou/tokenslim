use super::parser::*;
use crate::core::plugin_config_loader::parse_vcs_command_words_from_line;

/// 用 VcsParser 解析文本并渲染记录，强制保留首行命令锚点。
#[tracing::instrument(level = "debug", skip_all)]
pub fn process_parser(parser: &dyn VcsParser, raw: &str) -> String {
    if let Some(doc) = parser.parse(raw) {
        let mut out = String::new();
        for record in doc.records {
            out.push_str(&format!("{}\n", record));
        }
        let body = out.trim().to_string();
        if body.is_empty() {
            raw.to_string()
        } else {
            // 法则 0: 绝对锚点守卫 - 保留原始输入的第一行命令锚点
            let cmd_line = first_non_empty_line(raw);
            if !cmd_line.is_empty() && !body.starts_with(cmd_line) {
                format!("{}\n{}", cmd_line, body)
            } else {
                body
            }
        }
    } else {
        raw.to_string()
    }
}

/// 压缩 hg status 输出：用 HgStatusParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_status_for_ai(raw: &str) -> String {
    let out = process_parser(&HgStatusParser, raw);
    out
}

/// 压缩 hg diff 输出：用 HgDiffParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_diff_for_ai(raw: &str) -> String {
    let out = process_parser(&HgDiffParser, raw);
    out
}

/// 压缩 hg log 输出：用 HgLogParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_log_for_ai(raw: &str) -> String {
    let out = process_parser(&HgLogParser, raw);
    out
}

/// 压缩 hg heads 输出：用 HgHeadsParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_heads_for_ai(raw: &str) -> String {
    process_parser(&HgHeadsParser, raw)
}

/// 压缩 hg outgoing 输出：用 HgOutgoingParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_outgoing_for_ai(raw: &str) -> String {
    process_parser(&HgOutgoingParser, raw)
}

/// 压缩 hg incoming 输出：用 HgIncomingParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_incoming_for_ai(raw: &str) -> String {
    process_parser(&HgIncomingParser, raw)
}

/// 压缩 hg parents 输出：用 HgParentsParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_parents_for_ai(raw: &str) -> String {
    process_parser(&HgParentsParser, raw)
}

/// hg 其他输出入口：按子命令分派到 copy/move/purge/verify/identify 等专用 parser。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_other_for_ai(raw: &str) -> String {
    // 统一走 route parser，避免被全局参数或多空白干扰子命令识别。
    match hg_subcommand_from_raw(raw).as_deref().unwrap_or_default() {
        "copy" => process_parser(&HgCopyParser, raw),
        "move" => process_parser(&HgMoveParser, raw),
        "purge" => process_parser(&HgPurgeParser, raw),
        "archive" => process_parser(&HgArchiveParser, raw),
        "verify" => process_parser(&HgVerifyParser, raw),
        "identify" => process_parser(&HgIdentifyParser, raw),
        "paths" => process_parser(&HgPathsParser, raw),
        "config" => process_parser(&HgConfigParser, raw),
        "summarize" => process_parser(&HgSummarizeParser, raw),
        "transplant" => process_parser(&HgTransplantParser, raw),
        _ => raw.to_string(),
    }
}

/// 返回文本中第一个非空行。
#[tracing::instrument(level = "debug", skip_all)]
pub fn first_non_empty_line(raw: &str) -> &str {
    raw.lines().find(|l| !l.trim().is_empty()).unwrap_or("")
}

/// hg log 家族压缩入口：按块类型分派到 heads/outgoing/incoming/parents/merge 等。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_log_family_for_ai(raw: &str) -> String {
    if is_hg_heads_block(raw) {
        compact_hg_heads_for_ai(raw)
    } else if is_hg_outgoing_block(raw) {
        compact_hg_outgoing_for_ai(raw)
    } else if is_hg_incoming_block(raw) {
        compact_hg_incoming_for_ai(raw)
    } else if is_hg_parents_block(raw) {
        compact_hg_parents_for_ai(raw)
    } else if is_hg_merge_like_block(raw) {
        process_parser(&HgMergeParser, raw)
    } else if is_hg_rollback_like_block(raw) {
        process_parser(&HgRollbackParser, raw)
    } else if is_hg_backout_like_block(raw) {
        process_parser(&HgBackoutParser, raw)
    } else if is_hg_shelve_like_block(raw) {
        process_parser(&HgShelveParser, raw)
    } else if is_hg_phase_like_block(raw) {
        process_parser(&HgPhaseParser, raw)
    } else if is_hg_bookmarks_like_block(raw) {
        process_parser(&HgBookmarksParser, raw)
    } else if is_hg_tag_like_block(raw) {
        process_parser(&HgTagParser, raw)
    } else if is_hg_other_like_block(raw) {
        // HG-9: 零压缩命令路由到 compact_hg_other_for_ai
        compact_hg_other_for_ai(raw)
    } else {
        compact_hg_log_for_ai(raw)
    }
}

/// 判断是否为 hg 其他类命令块（copy/move/purge/archive/verify 等）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_other_like_block(raw: &str) -> bool {
    matches!(
        hg_subcommand_from_raw(raw).as_deref().unwrap_or_default(),
        "copy"
            | "move"
            | "purge"
            | "archive"
            | "verify"
            | "identify"
            | "paths"
            | "config"
            | "summarize"
            | "transplant"
    )
}

/// 判断是否为 hg status 块（status/st/summary/update 或 M/A/? 开头行）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_status_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let is_hg = matches!(
        hg_subcommand_from_raw(text).as_deref(),
        Some("status") | Some("st") | Some("summary") | Some("summarize") | Some("update")
    ) || lower.starts_with("m ")
        || lower.starts_with("a ")
        || lower.starts_with("? ");
    is_hg && !is_hg_diff_block(text)
}

/// 判断是否为 hg diff 块（diff 子命令或含 "diff -r "）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_diff_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    matches!(hg_subcommand_from_raw(text).as_deref(), Some("diff")) || lower.contains("diff -r ")
}

/// 判断是否为 hg log 块（log 子命令或含 changeset:）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_log_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    matches!(hg_subcommand_from_raw(text).as_deref(), Some("log")) || lower.contains("changeset:")
}

/// 判断是否为 hg heads 块（heads 子命令或 changeset: 开头）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_heads_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    matches!(hg_subcommand_from_raw(text).as_deref(), Some("heads"))
        || lower.starts_with("changeset:")
}

/// 判断是否为 hg outgoing 块（outgoing 子命令或含 searching for changes）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_outgoing_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    matches!(hg_subcommand_from_raw(text).as_deref(), Some("outgoing"))
        || lower.contains("searching for changes")
}

/// 判断是否为 hg incoming 块（incoming 子命令或含 comparing with）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_incoming_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    matches!(hg_subcommand_from_raw(text).as_deref(), Some("incoming"))
        || lower.contains("comparing with ")
}

/// 判断是否为 hg parents 块（parents 子命令或 changeset: 开头）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_parents_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    matches!(hg_subcommand_from_raw(text).as_deref(), Some("parents"))
        || lower.starts_with("changeset:")
}

/// 判断是否为 hg merge 类块（含 merging with/auto-merging 等）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_merge_like_block(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    lower.contains("merging with ")
        || lower.contains("auto-merging ")
        || lower.contains("merge completed")
}

/// 判断是否为 hg rollback 类块（含 rollback completed 等）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_rollback_like_block(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    lower.contains("rollback completed") || lower.contains("rolling back to ")
}

/// 判断是否为 hg backout 类块（含 backing out changeset 等）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_backout_like_block(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    lower.contains("backing out changeset ") || lower.contains("backed out changeset ")
}

/// 判断是否为 hg shelve 类块（含 shelved as）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_shelve_like_block(raw: &str) -> bool {
    raw.to_ascii_lowercase().contains("shelved as ")
}

/// 判断是否为 hg phase 类块（含 (public)/(draft)）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_phase_like_block(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    lower.contains(" (public)") || lower.contains(" (draft)")
}

/// 判断是否为 hg bookmarks 类块（含 * 或 @ 书签行）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_bookmarks_like_block(raw: &str) -> bool {
    let mut saw_bookmark_line = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("hg bookmarks") {
            continue;
        }
        if trimmed.starts_with('*') || trimmed.contains(" @ ") {
            saw_bookmark_line = true;
            continue;
        }
        if saw_bookmark_line && !trimmed.contains(':') {
            return true;
        }
    }
    saw_bookmark_line
}

/// 判断是否为 hg tag 类块（含 "tag '"）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_hg_tag_like_block(raw: &str) -> bool {
    raw.to_ascii_lowercase().contains("tag '")
}

/// 使用统一 argv 解析提取 hg 子命令，避免 split_whitespace 误判。
fn hg_subcommand_from_raw(raw: &str) -> Option<String> {
    let first = first_non_empty_line(raw).trim();
    let (tool, words) = parse_vcs_command_words_from_line(first)?;
    if tool != "hg" {
        return None;
    }
    words.first().cloned()
}

// ==================== Phase 2 P2 增强功能 ====================

/// 压缩 hg shelve --list 输出：折叠冗长的 shelve 列表
///
/// # 示例
/// ```text
/// 输入:
/// hg shelve --list
/// default         (10 files, 2 days ago)
/// feature-auth    (5 files, 1 week ago)
/// ... (10 shelves)
///
/// 输出:
/// hg shelve --list
/// [SHELVE] 10 shelves (first 5 shown, 5 omitted)
/// default         (10 files, 2 days ago)
/// ... (first 5 shelves)
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn compress_shelve_list(text: &str) -> String {
    const MAX_SHELVES: usize = 5;

    let mut shelves = Vec::new();
    let mut first_line = String::new();

    for (idx, line) in text.lines().enumerate() {
        // 保留第一行命令
        if idx == 0 {
            first_line = line.to_string();
            continue;
        }

        let trimmed = line.trim();

        // 跳过空行
        if trimmed.is_empty() {
            continue;
        }

        // 检测 shelve 条目（格式：name (N files, time ago)）
        if trimmed.contains(" files, ") || trimmed.contains(" file, ") {
            shelves.push(line.to_string());
        }
    }

    // 如果 shelve 数量超过阈值，折叠
    if shelves.len() > MAX_SHELVES {
        let mut result = first_line.clone();
        result.push('\n');
        result.push_str(&format!(
            "[SHELVE] {} shelves (first {} shown, {} omitted)\n",
            shelves.len(),
            MAX_SHELVES,
            shelves.len() - MAX_SHELVES
        ));

        for shelve in shelves.iter().take(MAX_SHELVES) {
            result.push_str(shelve);
            result.push('\n');
        }

        result.trim().to_string()
    } else {
        text.to_string()
    }
}

/// 压缩 hg graft 输出：折叠详细的 graft 信息
///
/// # 示例
/// ```text
/// 输入:
/// hg graft 123 456 789
/// grafting 123:abc1234 "feat: Add feature"
/// merging src/main.py
/// grafting 456:def5678 "fix: Fix bug"
/// merging src/utils.py
/// ...
///
/// 输出:
/// hg graft 123 456 789
/// [GRAFT] 3 changesets grafted
/// 123:abc1234 "feat: Add feature"
/// 456:def5678 "fix: Fix bug"
/// ...
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn compress_graft(text: &str) -> String {
    let mut changesets = Vec::new();
    let mut first_line = String::new();
    let mut graft_count = 0;

    for (idx, line) in text.lines().enumerate() {
        // 保留第一行命令
        if idx == 0 {
            first_line = line.to_string();
            continue;
        }

        let trimmed = line.trim();

        // 跳过空行
        if trimmed.is_empty() {
            continue;
        }

        // 检测 grafting 行（格式：grafting REV:HASH "message"）
        if trimmed.starts_with("grafting ") {
            graft_count += 1;
            // 提取 changeset 信息（去除 "grafting " 前缀）
            if let Some(info) = trimmed.strip_prefix("grafting ") {
                changesets.push(info.to_string());
            }
            continue;
        }

        // 跳过 merging 行（这些是详细信息，可以省略）
        if trimmed.starts_with("merging ") {
            continue;
        }
    }

    // 如果检测到 graft 操作，添加摘要
    if graft_count > 0 {
        let mut result = first_line.clone();
        result.push('\n');
        result.push_str(&format!("[GRAFT] {} changesets grafted\n", graft_count));

        for changeset in &changesets {
            result.push_str(changeset);
            result.push('\n');
        }

        result.trim().to_string()
    } else {
        text.to_string()
    }
}

/// 压缩 hg histedit 输出：折叠注释行，只保留命令
///
/// # 示例
/// ```text
/// 输入:
/// hg histedit
/// # Edit history between abc1234 and def5678
/// # Commands:
/// # p, pick = use commit
/// ...
/// pick abc1234 feat: Add feature
///
/// 输出:
/// hg histedit
/// [HISTEDIT] 20 changesets (interactive mode, help suppressed)
/// pick abc1234 feat: Add feature
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn compress_histedit(text: &str) -> String {
    let mut result = String::new();
    let mut command_count = 0;
    let mut has_histedit_header = false;
    let mut first_line = String::new();

    for (idx, line) in text.lines().enumerate() {
        // 保留第一行命令
        if idx == 0 {
            first_line = line.to_string();
            result.push_str(line);
            result.push('\n');
            continue;
        }

        let trimmed = line.trim();

        // 检测 histedit 头部
        if trimmed.starts_with("# Edit history between ") {
            has_histedit_header = true;
            continue;
        }

        // 跳过所有注释行
        if trimmed.starts_with('#') {
            continue;
        }

        // 保留命令行（pick, edit, fold, etc.）
        if !trimmed.is_empty() {
            // 统计命令数量
            if trimmed.starts_with("pick ")
                || trimmed.starts_with("edit ")
                || trimmed.starts_with("fold ")
                || trimmed.starts_with("roll ")
                || trimmed.starts_with("drop ")
                || trimmed.starts_with("mess ")
                || trimmed.starts_with("base ")
                || trimmed.starts_with("p ")
                || trimmed.starts_with("e ")
                || trimmed.starts_with("f ")
                || trimmed.starts_with("r ")
                || trimmed.starts_with("d ")
                || trimmed.starts_with("m ")
                || trimmed.starts_with("b ")
            {
                command_count += 1;
            }
            result.push_str(line);
            result.push('\n');
        }
    }

    // 如果检测到 histedit 头部，在第一行后添加摘要
    if has_histedit_header && command_count > 0 {
        let summary = format!(
            "[HISTEDIT] {} changesets (interactive mode, help suppressed)\n",
            command_count
        );
        // 在第一行命令后插入摘要
        let mut final_result = first_line.clone();
        final_result.push('\n');
        final_result.push_str(&summary);
        // 添加剩余的命令行
        for line in result.lines().skip(1) {
            final_result.push_str(line);
            final_result.push('\n');
        }
        final_result.trim().to_string()
    } else {
        result.trim().to_string()
    }
}

/// 增强的 hg shelve 压缩：应用 shelve list 压缩
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_shelve_enhanced(raw: &str) -> String {
    // 检测是否为 shelve --list 命令
    let cmd_lower = raw.lines().next().unwrap_or("").trim().to_ascii_lowercase();
    let is_list = cmd_lower.contains("--list") || cmd_lower.contains("-l");

    if is_list {
        // 先应用 shelve list 压缩
        let with_list = compress_shelve_list(raw);
        // ROI 门控：确保不扩展
        crate::core::utils::roi::prefer_non_expanding(raw, with_list)
    } else {
        // 普通 shelve，使用基础压缩
        let basic_compact = process_parser(&HgShelveParser, raw);
        crate::core::utils::roi::prefer_non_expanding(raw, basic_compact)
    }
}

/// 增强的 hg graft 压缩：应用 graft 输出折叠
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_graft_enhanced(raw: &str) -> String {
    // 先应用 graft 压缩
    let with_graft = compress_graft(raw);
    // ROI 门控：确保不扩展
    crate::core::utils::roi::prefer_non_expanding(raw, with_graft)
}

/// 增强的 hg histedit 压缩：应用 histedit 输出折叠
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_hg_histedit_enhanced(raw: &str) -> String {
    // 先应用 histedit 压缩
    let with_histedit = compress_histedit(raw);
    // ROI 门控：确保不扩展
    crate::core::utils::roi::prefer_non_expanding(raw, with_histedit)
}
