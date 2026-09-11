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

    // P2-61：status 行渲染为 "M path"，默认 path_pattern 不要求分隔符会把
    // 首捕获吃成状态字母 → 同状态 N 行坍缩为 1 叶子、文件列表全灭。
    // 与 diff 分支同理，显式要求 ≥1 路径分隔符并感知扩展名。
    let config = TreeConfig {
        path_pattern: r"([a-zA-Z0-9_./\\-]+[/\\][a-zA-Z0-9_./\\-]+(?:\.[a-zA-Z0-9]+)?)"
            .to_string(),
        ..TreeConfig::default()
    };
    restructure_as_tree(&compacted, &config)
}

/// 压缩 git checkout 输出：用 GitCheckoutParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_checkout_for_ai(raw: &str) -> String {
    process_parser(&GitCheckoutParser, raw)
}

/// 压缩 git log 输出：GitLogParser 解析后经 rebase todo 与冗余 tag 命令清理。
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
        // 使用文件路径专用正则：要求至少一个路径分隔符或文件扩展名，避免锚点行
        // 上的 `git`/`diff`/`--name-only` 等单词被误解析为伪路径条目。
        let config = TreeConfig {
            path_pattern: r"([a-zA-Z0-9_./\\-]+[/\\][a-zA-Z0-9_./\\-]+(?:\.[a-zA-Z0-9]+)?)"
                .to_string(),
            ..TreeConfig::default()
        };
        restructure_as_tree(&compacted, &config)
    } else {
        compacted
    }
}

/// 压缩 git add 输出：用 GitAddParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_add_for_ai(raw: &str) -> String {
    process_parser(&GitAddParser, raw)
}

/// 压缩 git stash 输出：用 GitStashParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_stash_for_ai(raw: &str) -> String {
    // stash list 是独立命令形态，行序即 stash 索引，样板可单独折叠
    if is_git_stash_list(raw) {
        return compact_git_stash_list_for_ai(raw);
    }
    process_parser(&GitStashParser, raw)
}

/// 判定输入是否为 `git stash list` 命令（含其锚点行）。
#[tracing::instrument(level = "debug", skip_all)]
fn is_git_stash_list(raw: &str) -> bool {
    raw.lines()
        .any(|l| l.trim().eq_ignore_ascii_case("git stash list"))
}

/// 压缩 `git stash list` 输出：条目的 `stash@{N}: ` 连续索引序列中，
/// N 与行序一一对应，故折叠该样板前缀、靠行序隐含索引，完整保留每条 stash 内容。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_stash_list_for_ai(raw: &str) -> String {
    let mut out = String::from("git stash list\n");
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() || t.eq_ignore_ascii_case("git stash list") {
            continue;
        }
        // 剥离 `stash@{N}: ` 前缀，仅保留索引后的正文（WIP on <branch>: <hash> <msg> 等）
        let body = t
            .strip_prefix("stash@{")
            .and_then(|r| r.split_once('}'))
            .map(|(_, after)| after.trim_start_matches(':').trim_start().to_string())
            .unwrap_or_else(|| t.to_string());
        if body.is_empty() {
            continue;
        }
        out.push_str(&body);
        out.push('\n');
    }
    out.trim_end_matches('\n').to_string()
}

/// 压缩 git reset 输出：用 GitResetParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_reset_for_ai(raw: &str) -> String {
    process_parser(&GitResetParser, raw)
}

/// 压缩 git switch 输出：用 GitSwitchParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_switch_for_ai(raw: &str) -> String {
    process_parser(&GitSwitchParser, raw)
}

/// 压缩 git merge 输出：用 GitMergeParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_merge_for_ai(raw: &str) -> String {
    process_parser(&GitMergeParser, raw)
}

/// 压缩 git restore 输出：用 GitRestoreParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_restore_for_ai(raw: &str) -> String {
    process_parser(&GitRestoreParser, raw)
}

/// 压缩 git clean 输出：用 GitCleanParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_clean_for_ai(raw: &str) -> String {
    process_parser(&GitCleanParser, raw)
}

/// 压缩 git show 输出：用 GitShowParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_show_for_ai(raw: &str) -> String {
    process_parser(&GitShowParser, raw)
}

/// 压缩 git blame 输出：用 GitBlameParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_blame_for_ai(raw: &str) -> String {
    process_parser(&GitBlameParser, raw)
}

/// 压缩 git revert 输出：用 GitRevertParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_revert_for_ai(raw: &str) -> String {
    process_parser(&GitRevertParser, raw)
}

/// 压缩 git cherry-pick 输出：用 GitCherryPickParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_cherry_pick_for_ai(raw: &str) -> String {
    process_parser(&GitCherryPickParser, raw)
}

/// 压缩 git branch 输出：用 GitBranchParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_branch_for_ai(raw: &str) -> String {
    process_parser(&GitBranchParser, raw)
}

/// 压缩 git remote 输出：用 GitRemoteParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_remote_for_ai(raw: &str) -> String {
    process_parser(&GitRemoteParser, raw)
}

/// 压缩 git tag 输出：用 GitTagParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_tag_for_ai(raw: &str) -> String {
    process_parser(&GitTagParser, raw)
}

/// 压缩 git rm 输出：用 GitRmParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_rm_for_ai(raw: &str) -> String {
    process_parser(&GitRmParser, raw)
}

/// 压缩 git fetch 输出：用 GitFetchParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_fetch_for_ai(raw: &str) -> String {
    process_parser(&GitFetchParser, raw)
}

/// 压缩 git push 输出：用 GitPushParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_push_for_ai(raw: &str) -> String {
    process_parser(&GitPushParser, raw)
}

/// 压缩 git pull 输出：用 GitPullParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_pull_for_ai(raw: &str) -> String {
    process_parser(&GitPullParser, raw)
}

/// 压缩 git bisect 输出：用 GitBisectParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_bisect_for_ai(raw: &str) -> String {
    process_parser(&GitBisectParser, raw)
}

/// 压缩 git submodule 输出：用 GitSubmoduleParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_submodule_for_ai(raw: &str) -> String {
    process_parser(&GitSubmoduleParser, raw)
}

/// 压缩 git rebase 输出：用 GitRebaseParser 解析并锚点守卫。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_rebase_for_ai(raw: &str) -> String {
    process_parser(&GitRebaseParser, raw)
}

/// git 其他输出入口：按首行子命令分派到 blame/revert/branch/stash/remote 等专用压缩。
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

/// 压缩 git help 输出：
/// - 将 `usage:` 主行与其 `[`/`<` 续行合并为单行（flag 列表语义保留）
/// - 命令列表行折叠对齐空格为单空格
/// - 折叠连续空行为单空行
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_help_for_ai(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_blank = false;
    let lines: Vec<&str> = raw.lines().collect();
    let mut i = 0usize;

    while i < lines.len() {
        let line = lines[i];
        let trimmed_end = line.trim_end();

        if trimmed_end.trim().is_empty() {
            if !prev_blank {
                out.push('\n');
                prev_blank = true;
            }
            i += 1;
            continue;
        }
        prev_blank = false;

        // usage 块合并：`usage:` 主行与后续以 `[`/`<` 开头的续行合并为单行
        if trimmed_end.trim_start().starts_with("usage:") {
            let mut usage = trimmed_end.trim().to_string();
            let mut j = i + 1;
            while j < lines.len() {
                let tj = lines[j].trim();
                if tj.is_empty() || (!tj.starts_with('[') && !tj.starts_with('<')) {
                    break;
                }
                usage.push(' ');
                usage.push_str(tj);
                j += 1;
            }
            out.push_str(&usage);
            out.push('\n');
            i = j;
            continue;
        }

        // 命令列表行：去掉行首对齐缩进，连续空格折叠为单空格
        let normalized = if trimmed_end.starts_with(' ') {
            trimmed_end.split_whitespace().collect::<Vec<_>>().join(" ")
        } else {
            trimmed_end.to_string()
        };

        out.push_str(&normalized);
        out.push('\n');
        i += 1;
    }

    out.trim_end_matches('\n').to_string()
}

/// 压缩 rebase todo 输出：删除 # 注释行，仅保留命令。
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

/// 压缩冗余的 git tag 命令行（单独一行的 "git tag" 被删除）。
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

/// 压缩 git worktree 输出：保留 "git worktree list" 锚点并去重。
///
/// 多条 worktree 常共享同一仓库根前缀（如 `C:/git_work/TokenSlim` 及其
/// `-feature-auth` / `-bugfix-123` 变体）。提取共享前缀生成 `[paths]` 字典，
/// 将绝对路径冗余符号化为 `$P0` token，降低逐行重复。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_git_worktree_for_ai(raw: &str) -> String {
    // 第一遍：剥离锚点与空行，分离每行「路径 + 尾部附加信息（branch 列等）」
    let mut rows: Vec<(String, String)> = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() || t.eq_ignore_ascii_case("git worktree list") {
            continue;
        }
        let mut it = t.split_whitespace();
        let path = it.next().unwrap_or("").to_string();
        if path.is_empty() {
            continue;
        }
        let tail = it.collect::<Vec<_>>().join(" ");
        rows.push((path, tail));
    }
    if rows.is_empty() {
        return raw.trim().to_string();
    }

    // 提取最长公共前缀（最长公共字符前缀）
    let common = longest_common_prefix(rows.iter().map(|(p, _)| p.as_str()).collect());
    // 边界门控：公共前缀后一位必须是分隔符类或行尾，否则前缀会切断单词，字典无意义
    let safe_prefix = if worktree_prefix_boundary_safe(&rows, &common) {
        common
    } else {
        String::new()
    };
    // 前缀足够长才有字典收益（否则 [paths] 元数据净膨胀）
    let dict_version = if safe_prefix.len() >= 8 {
        let dict_line = format!("[paths] $P0={}", safe_prefix);
        let mut out = String::from("git worktree list\n");
        out.push_str(&dict_line);
        out.push('\n');
        for (path, tail) in &rows {
            let rest = &path[safe_prefix.len()..];
            out.push_str("$P0");
            out.push_str(rest);
            if !tail.is_empty() {
                out.push(' ');
                out.push_str(tail);
            }
            out.push('\n');
        }
        out.trim_end_matches('\n').to_string()
    } else {
        // 前缀太短：退化为仅去重锚点的基础压缩
        let mut out = String::from("git worktree list\n");
        for (path, tail) in &rows {
            out.push_str(path);
            if !tail.is_empty() {
                out.push(' ');
                out.push_str(tail);
            }
            out.push('\n');
        }
        out.trim_end_matches('\n').to_string()
    };

    // ROI 门控：压缩结果不得扩张，否则回退原文
    crate::core::utils::roi::prefer_non_expanding(raw, dict_version)
}

/// 计算一组字符串的最长公共前缀；输入为空时返回空串。
fn longest_common_prefix<'a>(mut texts: Vec<&'a str>) -> String {
    let first = match texts.pop() {
        Some(t) => t,
        None => return String::new(),
    };
    let mut prefix_len = first.len();
    for t in &texts {
        let mut n = 0;
        for (a, b) in first.bytes().zip(t.bytes()) {
            if a != b {
                break;
            }
            n += 1;
        }
        prefix_len = prefix_len.min(n);
    }
    first[..prefix_len].to_string()
}

/// worktree 公共前缀边界安全检查：对每个路径，公共前缀之后必须是分隔符类
/// （`/` `\` `-` `_`）或行尾（路径恰为前缀），确保 `$P0` token 不会切断单词。
fn worktree_prefix_boundary_safe(rows: &[(String, String)], prefix: &str) -> bool {
    if prefix.is_empty() {
        return false;
    }
    rows.iter().all(|(path, _)| {
        if let Some(rest) = path.strip_prefix(prefix) {
            rest.is_empty()
                || rest
                    .chars()
                    .next()
                    .is_some_and(|c| matches!(c, '/' | '\\' | '-' | '_'))
        } else {
            false
        }
    })
}

/// 压缩 git grep 输出：保留命令锚点，去除重复锚点行与空行。
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

/// 返回文本中第一个非空行。
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
