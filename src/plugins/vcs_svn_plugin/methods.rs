#![allow(dead_code)]
//! SVN 压缩方法 — Compression Protocol V1
use super::parser::*;
use crate::core::path_compressor::types::PathCompressor;
use crate::core::plugin_config_loader::parse_vcs_command_words_from_line;

/// 对 SVN 压缩输出中的路径执行字典压缩
/// SVN 路径分为工作副本路径（如 src/plugins/...）和仓库 URL，分别提取公共前缀
/// 降低 min_prefix_length 至 10 以适应 SVN 的短层级路径结构
#[tracing::instrument(level = "debug", skip_all)]
pub fn compress_svn_paths(text: &str) -> String {
    if text.is_empty() {
        return text.to_string();
    }
    let mut compressor = PathCompressor::new();
    compressor.set_min_prefix_length(10); // SVN 路径前缀通常较短
    compressor.set_min_occurrences(2);
    compressor.extract_and_compress_from_text(text)
}

// ============================================================================
// 压缩基础设施（法则 0 锚点守卫）
// ============================================================================

/// 通用 Parser 处理：将 VcsDocument.records 渲染为文本
pub fn process_parser(parser: &dyn VcsParser, raw: &str) -> String {
    if let Some(doc) = parser.parse(raw) {
        let mut out = String::new();
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

/// ROI 门控：若压缩后体积反增，退回原始文本
fn cost_gate(raw: &str, output: String) -> String {
    if output.len() < raw.len() {
        output
    } else {
        raw.to_string()
    }
}

/// 法则 0 绝对锚点守卫：强制保留输入首行命令作为输出首行
fn anchor_guard(raw: &str, f: impl FnOnce() -> String) -> String {
    let anchor = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");
    let body = f();
    if body.is_empty() || anchor.is_empty() {
        return body;
    }
    // 命令锚点必须 100% 保留，不计压缩代价
    if body.starts_with(anchor) {
        return body;
    }
    format!("{}\n{}", anchor, body.trim_start())
}

/// 检测给定行是否为 svn 命令行（以 "svn " 开头且后续非空白）
#[inline]
pub fn is_svn_command_line(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("svn ") && t.len() > 4 && !t[4..].trim_start().is_empty()
}

/// SVN 叙述性噪音检测
fn is_svn_narrative_noise(t: &str) -> bool {
    let l = t.to_ascii_lowercase();
    l.starts_with("transmitting file data")
        || l.starts_with("updating '.':")
        || l == "cleaned up."
        || l == "cleanup completed."
        || l == "export complete."
        || l.starts_with("merging differences between repository urls")
        || l.starts_with("switched to ")
        || l.starts_with("relocated ")
}

// ============================================================================
// 公开 API（法则 0 锚点守卫已集成）
// ============================================================================

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_svn_status_for_ai(raw: &str) -> String {
    match svn_subcommand_from_raw(raw).as_deref().unwrap_or_default() {
        "update" => {
            // compress_update 已经包含命令锚点，不需要 anchor_guard
            let enhanced = compress_update(raw);
            crate::core::utils::roi::prefer_non_expanding(raw, enhanced)
        }
        _ => {
            // 其他命令使用 anchor_guard
            anchor_guard(raw, || {
                match svn_subcommand_from_raw(raw).as_deref().unwrap_or_default() {
                    "cleanup" => process_parser(&SvnCleanupParser, raw),
                    "export" => process_parser(&SvnExportParser, raw),
                    "revert" => process_parser(&SvnRevertParser, raw),
                    "resolve" => process_parser(&SvnResolveParser, raw),
                    _ => process_parser(&SvnStatusParser, raw),
                }
            })
        }
    }
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_svn_diff_for_ai(raw: &str) -> String {
    anchor_guard(raw, || process_parser(&SvnDiffParser, raw))
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_svn_log_for_ai(raw: &str) -> String {
    // 检测是否为 merge 命令
    if svn_command_is(raw, &["merge"]) {
        // compress_merge 已经包含命令锚点，不需要 anchor_guard
        let enhanced = compress_merge(raw);
        return crate::core::utils::roi::prefer_non_expanding(raw, enhanced);
    }

    anchor_guard(raw, || process_parser(&SvnLogParser, raw))
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_svn_commit_for_ai(raw: &str) -> String {
    anchor_guard(raw, || {
        let mut out: Vec<String> = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_end_matches('\r').trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("svn commit") {
                continue;
            }
            if let Some(path) = trimmed.strip_prefix("Sending ") {
                out.push(format!("M {}", path.trim()));
                continue;
            }
            if let Some(path) = trimmed.strip_prefix("Adding ") {
                out.push(format!("A {}", path.trim()));
                continue;
            }
            if let Some(path) = trimmed.strip_prefix("Deleting ") {
                out.push(format!("D {}", path.trim()));
                continue;
            }
            if trimmed.starts_with("Transmitting file data") {
                continue;
            }
            if let Some(rev) = trimmed
                .strip_prefix("Committed revision ")
                .and_then(|s| s.strip_suffix('.'))
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                out.push(format!("r{rev}"));
                continue;
            }
        }
        if out.is_empty() {
            raw.to_string()
        } else {
            out.join("\n")
        }
    })
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_svn_blame_for_ai(raw: &str) -> String {
    // compress_annotate 已经包含命令锚点，不需要 anchor_guard
    let enhanced = compress_annotate(raw);
    crate::core::utils::roi::prefer_non_expanding(raw, enhanced)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_svn_list_for_ai(raw: &str) -> String {
    anchor_guard(raw, || process_parser(&SvnListParser, raw))
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_svn_prop_for_ai(raw: &str) -> String {
    anchor_guard(raw, || process_parser(&SvnPropParser, raw))
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_svn_info_for_ai(raw: &str) -> String {
    anchor_guard(raw, || process_parser(&SvnInfoParser, raw))
}

// ============================================================================
// 检测函数（用于 core_logic 分派）
// ============================================================================

pub fn is_svn_blame_block(text: &str) -> bool {
    fn looks_like_svn_blame_line(line: &str) -> bool {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            return false;
        }
        if let Some(idx) = trimmed.find(char::is_whitespace) {
            let revision = &trimmed[..idx];
            revision == "-" || revision.chars().all(|c| c.is_ascii_digit())
        } else {
            false
        }
    }
    svn_command_is(text, &["blame"]) || text.lines().any(looks_like_svn_blame_line)
}

pub fn is_svn_list_block(text: &str) -> bool {
    fn looks_like_svn_list_entry(line: &str) -> bool {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return false;
        }
        if trimmed.ends_with('/') {
            return true;
        }
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        if tokens.is_empty() {
            return false;
        }
        tokens.len() >= 5 && tokens[0].chars().all(|c| c.is_ascii_digit())
    }
    svn_command_is(text, &["list"]) || text.lines().any(looks_like_svn_list_entry)
}

pub fn is_svn_prop_block(text: &str) -> bool {
    svn_command_is(text, &["propget", "proplist"])
        || text
            .lines()
            .any(|line| line.trim_start().starts_with("Properties on "))
}

pub fn is_svn_info_block(text: &str) -> bool {
    svn_command_is(text, &["info"])
        || ["Path: ", "URL: ", "Relative URL: ", "Repository Root: "]
            .iter()
            .any(|prefix| text.lines().any(|l| l.trim_start().starts_with(prefix)))
}

/// 检测是否为 svn status 块（用于 core_logic 分派）
pub fn is_svn_status_block(text: &str) -> bool {
    svn_command_is(
        text,
        &[
            "status", "st", "update", "switch", "revert", "resolve", "lock", "unlock", "cleanup",
            "export",
        ],
    )
}

/// 检测是否为 svn log 块（用于 core_logic 分派）
pub fn is_svn_log_block(text: &str) -> bool {
    svn_command_is(
        text,
        &[
            "log", "commit", "merge", "relocate", "switch", "propset", "import",
        ],
    ) || text.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("r") && trimmed.contains(" | ")
    })
}

// ============================================================================
// other 分派
// ============================================================================

pub fn compact_svn_other_for_ai(raw: &str) -> String {
    if is_svn_blame_block(raw) {
        compact_svn_blame_for_ai(raw)
    } else if is_svn_list_block(raw) {
        compact_svn_list_for_ai(raw)
    } else if is_svn_prop_block(raw) {
        compact_svn_prop_for_ai(raw)
    } else if is_svn_info_block(raw) {
        compact_svn_info_for_ai(raw)
    } else {
        raw.to_string()
    }
}

/// 使用统一 argv 解析提取 svn 子命令，避免 split_whitespace 受到全局参数影响。
fn svn_subcommand_from_raw(raw: &str) -> Option<String> {
    let first = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");
    let (tool, words) = parse_vcs_command_words_from_line(first)?;
    if tool != "svn" {
        return None;
    }
    words.first().cloned()
}

fn svn_command_is(raw: &str, expected: &[&str]) -> bool {
    let Some(sub) = svn_subcommand_from_raw(raw) else {
        return false;
    };
    expected.iter().any(|cmd| *cmd == sub)
}

// ==================== Phase 2 P2 增强功能 ====================

/// 压缩 svn merge 输出：折叠合并详细信息，保留冲突摘要
///
/// # 示例
/// ```text
/// 输入:
/// svn merge ^/branches/feature
/// --- Merging r123 through r456 into '.':
/// U    src/file1.py
/// U    src/file2.py
/// C    src/file3.py
/// ...
/// Summary of conflicts:
///   Text conflicts: 3
///
/// 输出:
/// svn merge ^/branches/feature
/// [MERGE] r123-r456: 10 updated, 3 conflicts
/// C    src/file3.py
/// C    src/file5.py
/// C    src/file7.py
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn compress_merge(text: &str) -> String {
    let mut first_line = String::new();
    let mut updated_count = 0;
    let mut conflict_files = Vec::new();
    let mut rev_range = String::new();

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

        // 提取 revision 范围
        // 格式: "--- Merging r123 through r456 into '.':"
        if trimmed.starts_with("--- Merging r") && trimmed.contains(" through r") {
            // 找到 "r123" 的起始位置（"--- Merging r" 之后）
            let prefix = "--- Merging r";
            let start_pos = prefix.len();

            // 找到 " through r" 的位置
            if let Some(through_pos) = trimmed.find(" through r") {
                // 提取起始 revision（r123 中的 123）
                let start_rev = &trimmed[start_pos..through_pos];

                // 找到结束 revision（r456 中的 456）
                let end_start = through_pos + " through r".len();
                let remaining = &trimmed[end_start..];

                // 找到下一个空格或结束位置
                if let Some(space_pos) = remaining.find(' ') {
                    let end_rev = &remaining[..space_pos];
                    rev_range = format!("r{}-r{}", start_rev, end_rev);
                }
            }
            continue;
        }

        // 跳过 Recording mergeinfo 行
        if trimmed.starts_with("--- Recording mergeinfo") {
            continue;
        }

        // 跳过 Summary of conflicts 头
        if trimmed.starts_with("Summary of conflicts:") {
            continue;
        }

        // 跳过 Text conflicts 统计行（我们自己统计）
        if trimmed.starts_with("Text conflicts:") {
            continue;
        }

        // 检测文件状态行
        if trimmed.starts_with("U ") {
            updated_count += 1;
            continue;
        }

        if trimmed.starts_with("C ") {
            conflict_files.push(line.to_string());
            continue;
        }

        // 跳过 mergeinfo 更新行（" U   ."）
        if trimmed == "U   ." {
            continue;
        }
    }

    // 构建压缩输出
    let mut result = first_line.clone();
    result.push('\n');

    // 添加摘要行
    if !rev_range.is_empty() || updated_count > 0 || !conflict_files.is_empty() {
        let conflict_count = conflict_files.len();
        result.push_str(&format!(
            "[MERGE] {}: {} updated, {} conflicts\n",
            if rev_range.is_empty() {
                "merge".to_string()
            } else {
                rev_range
            },
            updated_count,
            conflict_count
        ));

        // 保留冲突文件列表
        for conflict in &conflict_files {
            result.push_str(conflict);
            result.push('\n');
        }
    }

    result.trim().to_string()
}

/// 压缩 svn update 输出：折叠详细的文件列表
///
/// # 示例
/// ```text
/// 输入:
/// svn update
/// Updating '.':
/// U    src/file1.py
/// U    src/file2.py
/// ...（20 个文件）
/// At revision 12345.
/// Updated 20 files.
///
/// 输出:
/// svn update
/// [UPDATE] r12345: 20 files updated
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn compress_update(text: &str) -> String {
    let mut first_line = String::new();
    let mut updated_files = Vec::new();
    let mut revision = String::new();

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

        // 跳过 "Updating '.':" 行
        if trimmed.starts_with("Updating ") {
            continue;
        }

        // 提取 revision
        if trimmed.starts_with("At revision ") {
            if let Some(rev) = trimmed
                .strip_prefix("At revision ")
                .and_then(|s| s.strip_suffix('.'))
            {
                revision = format!("r{}", rev.trim());
            }
            continue;
        }

        // 跳过 "Updated N files." 行（我们自己统计）
        if trimmed.starts_with("Updated ") && trimmed.ends_with(" files.") {
            continue;
        }

        // 检测文件状态行
        if trimmed.starts_with("U ") || trimmed.starts_with("A ") || trimmed.starts_with("D ") {
            updated_files.push(line.to_string());
            continue;
        }
    }

    // 如果文件数量少于阈值，保留详细列表
    const MAX_FILES: usize = 10;
    if updated_files.len() <= MAX_FILES {
        return text.to_string();
    }

    // 构建压缩输出
    let mut result = first_line.clone();
    result.push('\n');

    // 添加摘要行
    result.push_str(&format!(
        "[UPDATE] {}: {} files updated (details suppressed)\n",
        if revision.is_empty() {
            "update".to_string()
        } else {
            revision
        },
        updated_files.len()
    ));

    result.trim().to_string()
}

/// 压缩 svn annotate/blame 输出：折叠相同作者的连续行
///
/// # 示例
/// ```text
/// 输入:
/// svn annotate file.py
///    123   alice.chen     line1
///    123   alice.chen     line2
///    123   alice.chen     line3
///    125   bob.wang       line4
///    125   bob.wang       line5
///
/// 输出:
/// svn annotate file.py
/// [ANNOTATE] 5 lines (2 contributors: alice.chen, bob.wang)
/// r123 @alice.chen (3 lines)
/// r125 @bob.wang (2 lines)
/// ```
#[tracing::instrument(level = "debug", skip_all)]
pub fn compress_annotate(text: &str) -> String {
    let mut first_line = String::new();
    let mut contributors = std::collections::HashMap::new();
    let mut total_lines = 0;

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

        // 解析 annotate 行格式：revision author content
        // 格式: "   123   alice.chen     line content"
        // 使用 split_whitespace 自动处理多个空格
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 2 {
            let revision = parts[0].trim();
            let author = parts[1].trim();

            if !revision.is_empty() && !author.is_empty() {
                total_lines += 1;
                let key = format!("r{} @{}", revision, author);
                *contributors.entry(key).or_insert(0) += 1;
            }
        }
    }

    // 如果行数少于阈值，保留原始输出
    const MAX_LINES: usize = 20;
    if total_lines <= MAX_LINES {
        return text.to_string();
    }

    // 构建压缩输出
    let mut result = first_line.clone();
    result.push('\n');

    // 提取唯一作者
    let mut unique_authors: Vec<String> = contributors
        .keys()
        .filter_map(|k| k.split('@').nth(1).map(|s| s.to_string()))
        .collect();
    unique_authors.sort();
    unique_authors.dedup();

    // 添加摘要行
    result.push_str(&format!(
        "[ANNOTATE] {} lines ({} contributors: {})\n",
        total_lines,
        unique_authors.len(),
        unique_authors
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    ));

    // 添加贡献者统计
    let mut sorted_contributors: Vec<_> = contributors.iter().collect();
    sorted_contributors.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0))); // 按行数降序，稳定处理并列项

    for (key, count) in sorted_contributors.iter().take(10) {
        result.push_str(&format!("{} ({} lines)\n", key, count));
    }

    if sorted_contributors.len() > 10 {
        result.push_str(&format!(
            "... and {} more contributors\n",
            sorted_contributors.len() - 10
        ));
    }

    result.trim().to_string()
}

/// 增强的 svn merge 压缩：应用 merge 输出折叠
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_svn_merge_enhanced(raw: &str) -> String {
    // 先应用 merge 压缩
    let with_merge = compress_merge(raw);
    // ROI 门控：确保不扩展
    crate::core::utils::roi::prefer_non_expanding(raw, with_merge)
}

/// 增强的 svn update 压缩：应用 update 输出折叠
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_svn_update_enhanced(raw: &str) -> String {
    // 先应用 update 压缩
    let with_update = compress_update(raw);
    // ROI 门控：确保不扩展
    crate::core::utils::roi::prefer_non_expanding(raw, with_update)
}

/// 增强的 svn annotate 压缩：应用 annotate 输出折叠
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_svn_annotate_enhanced(raw: &str) -> String {
    // 先应用 annotate 压缩
    let with_annotate = compress_annotate(raw);
    // ROI 门控：确保不扩展
    crate::core::utils::roi::prefer_non_expanding(raw, with_annotate)
}
