use super::parser::*;
use crate::core::plugin_config_loader::parse_vcs_command_words_from_line;
use crate::core::tree_restructure::{restructure_as_tree, TreeConfig};

/// 【法则 0：绝对锚点守卫】— 保留原始触发命令作为 IR 输出绝对第一行
#[tracing::instrument(level = "debug", skip_all)]
pub fn process_parser(parser: &dyn VcsParser, raw: &str) -> String {
    // 提取原始输入的第一行触发命令作为锚点
    let anchor_line = first_non_empty_line(raw);

    if let Some(doc) = parser.parse(raw) {
        let mut out = String::new();
        // 锚点守卫：原始命令必须作为输出的绝对第一行
        out.push_str(anchor_line);
        out.push('\n');
        for record in doc.records {
            out.push_str(&format!("{}\n", record));
        }
        if out.trim().is_empty() {
            raw.to_string()
        } else {
            out.trim().to_string()
        }
    } else {
        raw.to_string()
    }
}

/// Git status 压缩 - 支持树结构重组
///
/// # 树结构重组
/// 当文件列表满足以下条件时，自动重组为树结构：
/// - 至少 4 个文件
/// - 至少 1 层共享目录深度
///
/// # 示例
/// ```text
/// 原始输出:
/// M  src/core/mod.rs
/// A  src/core/types.rs
/// M  src/main.rs
///
/// 树结构输出:
/// src/
/// ├─ core/
/// │  ├─ M mod.rs
/// │  └─ A types.rs
/// └─ M main.rs
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_status_for_ai(raw: &str) -> String {
    let compacted = process_parser(&GitStatusParser, raw);

    // 应用树结构重组（使用默认配置）
    let config = TreeConfig::default();
    restructure_as_tree(&compacted, &config)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_checkout_for_ai(raw: &str) -> String {
    process_parser(&GitCheckoutParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_log_for_ai(raw: &str) -> String {
    let out = process_parser(&GitLogParser, raw);
    if out == raw {
        return raw.to_string();
    }
    let rebased = compact_git_rebase_todo_for_ai(&out);
    compact_redundant_git_tag_commands_for_ai(&rebased)
        .trim()
        .to_string()
}

/// Git diff 压缩 - 支持树结构重组
///
/// # 树结构重组
/// 对于 --name-only 和 --name-status 模式，当文件列表满足条件时自动重组为树结构
///
/// # 示例
/// ```text
/// git diff --name-only
/// src/core/mod.rs
/// src/core/types.rs
/// src/main.rs
///
/// 树结构输出:
/// git diff --name-only
/// src/
/// ├─ core/
/// │  ├─ mod.rs
/// │  └─ types.rs
/// └─ main.rs
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_diff_for_ai(raw: &str) -> String {
    let compacted = process_parser(&GitDiffParser, raw);

    // 检查是否为 name-only 或 name-status 模式
    let cmd_lower = raw.lines().next().unwrap_or("").trim().to_ascii_lowercase();
    let is_name_mode = cmd_lower.contains("--name-only") || cmd_lower.contains("--name-status");

    // 仅对 name-only/name-status 模式应用树结构重组
    if is_name_mode {
        let config = TreeConfig::default();
        restructure_as_tree(&compacted, &config)
    } else {
        compacted
    }
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_add_for_ai(raw: &str) -> String {
    process_parser(&GitAddParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_stash_for_ai(raw: &str) -> String {
    process_parser(&GitStashParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_reset_for_ai(raw: &str) -> String {
    process_parser(&GitResetParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_switch_for_ai(raw: &str) -> String {
    process_parser(&GitSwitchParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_merge_for_ai(raw: &str) -> String {
    process_parser(&GitMergeParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_restore_for_ai(raw: &str) -> String {
    process_parser(&GitRestoreParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_clean_for_ai(raw: &str) -> String {
    process_parser(&GitCleanParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_show_for_ai(raw: &str) -> String {
    process_parser(&GitShowParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_blame_for_ai(raw: &str) -> String {
    process_parser(&GitBlameParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_revert_for_ai(raw: &str) -> String {
    process_parser(&GitRevertParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_cherry_pick_for_ai(raw: &str) -> String {
    process_parser(&GitCherryPickParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_branch_for_ai(raw: &str) -> String {
    process_parser(&GitBranchParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_remote_for_ai(raw: &str) -> String {
    process_parser(&GitRemoteParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_tag_for_ai(raw: &str) -> String {
    process_parser(&GitTagParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_rm_for_ai(raw: &str) -> String {
    process_parser(&GitRmParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_fetch_for_ai(raw: &str) -> String {
    process_parser(&GitFetchParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_push_for_ai(raw: &str) -> String {
    process_parser(&GitPushParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_pull_for_ai(raw: &str) -> String {
    process_parser(&GitPullParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_bisect_for_ai(raw: &str) -> String {
    process_parser(&GitBisectParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_submodule_for_ai(raw: &str) -> String {
    process_parser(&GitSubmoduleParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_rebase_for_ai(raw: &str) -> String {
    process_parser(&GitRebaseParser, raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_other_for_ai(raw: &str) -> String {
    let first = first_non_empty_line(raw).trim();
    let lower_first = first.to_ascii_lowercase();
    if lower_first == "git"
        || lower_first == "git --help"
        || lower_first == "git -h"
        || lower_first.starts_with("git help")
    {
        return compact_git_help_for_ai(raw);
    }
    if let Some((tool, words)) = parse_vcs_command_words_from_line(first) {
        if tool == "git" {
            if words.is_empty() || words.first().is_some_and(|w| w == "--help" || w == "-h") {
                return compact_git_help_for_ai(raw);
            }
            match words.first().map(String::as_str) {
                Some("blame") => return compact_git_blame_for_ai(raw),
                Some("revert") => return compact_git_revert_for_ai(raw),
                Some("cherry-pick") => return compact_git_cherry_pick_for_ai(raw),
                Some("branch") => return compact_git_branch_for_ai(raw),
                Some("stash") => return compact_git_stash_for_ai(raw),
                Some("remote") => return compact_git_remote_for_ai(raw),
                Some("tag") => return compact_git_tag_for_ai(raw),
                Some("rm") => return compact_git_rm_for_ai(raw),
                Some("fetch") => return compact_git_fetch_for_ai(raw),
                Some("push") => return compact_git_push_for_ai(raw),
                Some("pull") => return compact_git_pull_for_ai(raw),
                Some("bisect") => return compact_git_bisect_for_ai(raw),
                Some("submodule") => return compact_git_submodule_for_ai(raw),
                Some("rebase") => return compact_git_rebase_for_ai(raw),
                Some("worktree") => return compact_git_worktree_for_ai(raw),
                Some("grep") => return compact_git_grep_for_ai(raw),
                _ => {}
            }
        }
    }
    raw.to_string()
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_help_for_ai(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_blank = false;

    for line in raw.lines() {
        let trimmed_end = line.trim_end();

        if trimmed_end.trim().is_empty() {
            if !prev_blank {
                out.push('\n');
                prev_blank = true;
            }
            continue;
        }
        prev_blank = false;

        // 仅压缩“命令列表行”的对齐空格，避免破坏 usage 多行排版
        let normalized = if trimmed_end.starts_with("   ") && !trimmed_end.starts_with("      [") {
            compact_command_list_alignment(trimmed_end)
        } else {
            trimmed_end.to_string()
        };

        out.push_str(&normalized);
        out.push('\n');
    }

    out.trim_end_matches('\n').to_string()
}

fn compact_command_list_alignment(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut i = 0usize;
    let bytes = line.as_bytes();
    while i < bytes.len() {
        let b = bytes[i];
        if b == b' ' {
            let start = i;
            while i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
            let count = i - start;
            if count >= 2 {
                result.push_str("  ");
            } else {
                result.push(' ');
            }
            continue;
        }
        result.push(b as char);
        i += 1;
    }
    result
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_rebase_todo_for_ai(input: &str) -> String {
    // Rebase TODO logic implementation
    let mut out = String::new();
    for line in input.lines() {
        if line.trim().starts_with('#') {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if out.is_empty() {
        input.to_string()
    } else {
        out
    }
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_redundant_git_tag_commands_for_ai(input: &str) -> String {
    let mut out = String::new();
    for line in input.lines() {
        if line.trim().eq_ignore_ascii_case("git tag") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if out.is_empty() {
        input.to_string()
    } else {
        out
    }
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_worktree_for_ai(raw: &str) -> String {
    let mut out = String::from("git worktree list\n");
    for line in raw.lines() {
        if line.trim().is_empty() || line.trim().eq_ignore_ascii_case("git worktree list") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim_end_matches('\n').to_string()
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_grep_for_ai(raw: &str) -> String {
    let anchor = first_non_empty_line(raw).trim();
    let mut out = String::new();
    if !anchor.is_empty() {
        out.push_str(anchor);
        out.push('\n');
    }
    let mut skipped_anchor = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if !skipped_anchor && trimmed == anchor {
            skipped_anchor = true;
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if out.is_empty() {
        raw.to_string()
    } else {
        out.trim_end_matches('\n').to_string()
    }
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn first_non_empty_line(raw: &str) -> &str {
    raw.lines().find(|l| !l.trim().is_empty()).unwrap_or("")
}

// ==================== Phase 2 P2 增强功能 ====================

/// 压缩 merge conflict 标记：将冗长的 conflict 信息折叠为摘要
///
/// # 示例
/// ```text
/// 输入:
/// CONFLICT (content): Merge conflict in src/main.rs
/// CONFLICT (content): Merge conflict in src/utils.rs
///
/// 输出:
/// [CONFLICT] 2 files: src/main.rs, src/utils.rs
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn compress_merge_conflicts(text: &str) -> String {
    use std::collections::HashSet;

    let mut conflicts: HashSet<String> = HashSet::new();
    let mut other_lines = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();

        // 检测 CONFLICT 行
        if trimmed.starts_with("CONFLICT (content): Merge conflict in ") {
            if let Some(path) = trimmed.strip_prefix("CONFLICT (content): Merge conflict in ") {
                conflicts.insert(path.trim().to_string());
                continue;
            }
        } else if trimmed.starts_with("!CONFLICT:") {
            // 已经是压缩格式，提取路径
            if let Some(path) = trimmed.strip_prefix("!CONFLICT:") {
                conflicts.insert(path.trim().to_string());
                continue;
            }
        }

        // 保留其他行
        if !trimmed.is_empty() {
            other_lines.push(line.to_string());
        }
    }

    // 组装输出
    let mut result = String::new();

    // 先输出其他行
    for line in &other_lines {
        result.push_str(line);
        result.push('\n');
    }

    // 如果有 conflicts，添加摘要
    if !conflicts.is_empty() {
        let mut conflict_list: Vec<_> = conflicts.into_iter().collect();
        conflict_list.sort();

        if conflict_list.len() == 1 {
            result.push_str(&format!("[CONFLICT] {}\n", conflict_list[0]));
        } else if conflict_list.len() <= 3 {
            result.push_str(&format!(
                "[CONFLICT] {} files: {}\n",
                conflict_list.len(),
                conflict_list.join(", ")
            ));
        } else {
            // 超过 3 个文件，只显示前 3 个
            result.push_str(&format!(
                "[CONFLICT] {} files: {}, ... ({} more)\n",
                conflict_list.len(),
                conflict_list[..3].join(", "),
                conflict_list.len() - 3
            ));
        }
    }

    result.trim().to_string()
}

/// 压缩 rebase 交互式输出：折叠注释行，只保留命令
///
/// # 示例
/// ```text
/// 输入:
/// # Rebase abc..def onto xyz (10 commands)
/// # Commands:
/// # p, pick = use commit
/// ...
/// pick abc1234 feat: Add feature
///
/// 输出:
/// [REBASE] 10 commits (interactive mode, help suppressed)
/// pick abc1234 feat: Add feature
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn compress_rebase_interactive(text: &str) -> String {
    let mut result = String::new();
    let mut command_count = 0;
    let mut has_rebase_header = false;
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

        // 检测 rebase 头部
        if trimmed.starts_with("# Rebase ") && trimmed.contains(" commands)") {
            has_rebase_header = true;
            // 提取命令数量
            if let Some(count_str) = trimmed.split('(').nth(1) {
                if let Some(num_str) = count_str.split_whitespace().next() {
                    command_count = num_str.parse::<usize>().unwrap_or(0);
                }
            }
            continue;
        }

        // 跳过所有注释行
        if trimmed.starts_with('#') {
            continue;
        }

        // 保留命令行
        if !trimmed.is_empty() {
            result.push_str(line);
            result.push('\n');
        }
    }

    // 如果检测到 rebase 头部，在第一行后添加摘要
    if has_rebase_header && command_count > 0 {
        let summary = format!(
            "[REBASE] {} commits (interactive mode, help suppressed)\n",
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

/// 压缩 git log --graph 输出：折叠 ASCII 图形，保留提交信息
///
/// # 示例
/// ```text
/// 输入:
/// * abc1234 Merge branch 'feature'
/// |\  
/// | * def5678 feat: Add feature
/// |/  
/// * ghi9012 Initial commit
///
/// 输出:
/// [GRAPH] 3 commits (use --no-graph for details)
/// abc1234 Merge branch 'feature'
/// def5678 feat: Add feature
/// ghi9012 Initial commit
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn compress_log_graph(text: &str) -> String {
    let mut commits = Vec::new();
    let mut has_graph = false;
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

        // 检测图形字符（只检测行首的图形）
        let line_start = line.trim_start();
        if line_start.starts_with('*')
            || line_start.starts_with('|')
            || line_start.starts_with('/')
            || line_start.starts_with('\\')
        {
            has_graph = true;

            // 提取提交信息（去除图形字符）
            let cleaned = trimmed
                .replace('*', "")
                .replace('|', "")
                .replace('/', "")
                .replace('\\', "")
                .trim()
                .to_string();

            // 只保留包含提交信息的行（至少有 hash 和 message）
            if !cleaned.is_empty() && cleaned.split_whitespace().count() >= 2 {
                commits.push(cleaned);
            }
        }
    }

    // 如果检测到图形且提交数量较多，添加摘要
    if has_graph && commits.len() > 10 {
        let mut result = first_line.clone();
        result.push('\n');
        result.push_str(&format!(
            "[GRAPH] {} commits (use --no-graph for details)\n",
            commits.len()
        ));
        // 只显示前 10 个提交
        for commit in commits.iter().take(10) {
            result.push_str(commit);
            result.push('\n');
        }
        result.push_str(&format!(
            "... ({} more commits omitted)\n",
            commits.len() - 10
        ));
        result.trim().to_string()
    } else if has_graph && !commits.is_empty() {
        // 提交数量不多，去除图形但保留所有提交
        let mut result = first_line.clone();
        result.push('\n');
        for commit in &commits {
            result.push_str(commit);
            result.push('\n');
        }
        result.trim().to_string()
    } else {
        text.to_string()
    }
}

/// 压缩 git reflog 输出：折叠冗长的 reflog 条目
///
/// # 示例
/// ```text
/// 输入:
/// abc1234 HEAD@{0}: commit: feat: Add feature
/// def5678 HEAD@{1}: commit: fix: Fix bug
/// ... (50 entries)
///
/// 输出:
/// [REFLOG] 50 entries (first 20 shown, 30 omitted)
/// abc1234 HEAD@{0}: commit: feat: Add feature
/// ... (first 20 entries)
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn compress_reflog(text: &str) -> String {
    const MAX_ENTRIES: usize = 20;

    let mut entries = Vec::new();
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

        // 检测 reflog 条目（格式：hash HEAD@{N}: action: message）
        if trimmed.contains("HEAD@{") {
            entries.push(line.to_string());
        }
    }

    // 如果条目数量超过阈值，折叠
    if entries.len() > MAX_ENTRIES {
        let mut result = first_line.clone();
        result.push('\n');
        result.push_str(&format!(
            "[REFLOG] {} entries (first {} shown, {} omitted)\n",
            entries.len(),
            MAX_ENTRIES,
            entries.len() - MAX_ENTRIES
        ));

        for entry in entries.iter().take(MAX_ENTRIES) {
            result.push_str(entry);
            result.push('\n');
        }

        result.trim().to_string()
    } else {
        text.to_string()
    }
}

/// 增强的 git merge 压缩：应用 conflict 压缩
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_merge_enhanced(raw: &str) -> String {
    let basic_compact = compact_git_merge_for_ai(raw);

    // 应用 conflict 压缩
    let with_conflicts = compress_merge_conflicts(&basic_compact);

    // ROI 门控：确保不扩展
    crate::core::utils::roi::prefer_non_expanding(raw, with_conflicts)
}

/// 增强的 git rebase 压缩：应用交互式输出折叠
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_rebase_enhanced(raw: &str) -> String {
    // 先应用交互式输出折叠（在 parser 之前）
    let with_interactive = compress_rebase_interactive(raw);

    // 再应用基础 rebase 压缩
    let basic_compact = compact_git_rebase_for_ai(&with_interactive);

    // ROI 门控：确保不扩展
    crate::core::utils::roi::prefer_non_expanding(raw, basic_compact)
}

/// 增强的 git log 压缩：应用 graph 和 reflog 压缩
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_log_enhanced(raw: &str) -> String {
    // 检测是否为 --graph 或 reflog 模式
    let cmd_lower = raw.lines().next().unwrap_or("").trim().to_ascii_lowercase();
    let is_graph = cmd_lower.contains("--graph");
    let is_reflog = cmd_lower.contains("reflog");

    // 对于 graph 和 reflog，先应用特殊压缩，再应用基础压缩
    let result = if is_graph {
        // 先应用 graph 压缩（在 parser 之前）
        let graph_compressed = compress_log_graph(raw);
        // 再应用基础 log 压缩
        compact_git_log_for_ai(&graph_compressed)
    } else if is_reflog {
        // 先应用 reflog 压缩（在 parser 之前）
        let reflog_compressed = compress_reflog(raw);
        // 再应用基础 log 压缩
        compact_git_log_for_ai(&reflog_compressed)
    } else {
        // 普通 log，直接应用基础压缩
        compact_git_log_for_ai(raw)
    };

    // ROI 门控：确保不扩展
    crate::core::utils::roi::prefer_non_expanding(raw, result)
}
