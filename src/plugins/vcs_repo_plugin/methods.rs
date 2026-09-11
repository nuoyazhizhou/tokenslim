#![allow(dead_code)]
//! Repo 压缩方法 — Compression Protocol V1
use crate::core::plugin_config_loader::parse_vcs_command_words_from_line;

// ============================================================================
// 公开 API
// ============================================================================
/// repo status 的 AI 压缩入口：委托 compact_repo_log_for_ai。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_repo_status_for_ai(raw: &str) -> String {
    compact_repo_log_for_ai(raw)
}

/// repo 其他输出的 AI 压缩入口：委托 compact_repo_log_for_ai。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_repo_other_for_ai(raw: &str) -> String {
    compact_repo_log_for_ai(raw)
}

/// 判断是否为 repo 命令块：首个非空行以 repo 命令头开头。
#[tracing::instrument(level = "debug", skip_all)]
pub fn is_repo_log_block(text: &str) -> bool {
    text.lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim_start().starts_with("repo "))
        .unwrap_or(false)
}

// ============================================================================
// 调度器
// ============================================================================
/// 主入口：按 `repo` 子命令分发到专用语义萃取器
fn compact_repo_log_for_ai(raw: &str) -> String {
    // 健壮性：过短输入直接返回原始文本，避免反向压缩
    if raw.len() < 50 {
        return raw.to_string();
    }

    let first_line = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();

    if let Some((tool, words)) = parse_vcs_command_words_from_line(first_line) {
        if tool == "repo" {
            match words.first().map(String::as_str) {
                Some("sync") => return compact_repo_sync(raw),
                Some("status") => return compact_repo_status_cmd(raw),
                Some("upload") => return compact_repo_upload(raw),
                _ => {}
            }
        }
    }

    // 通用噪声过滤
    compact_repo_generic(raw)
}

// ============================================================================
// Case 100: sync — 保留锚点，消除进度噪音，映射项目与哈希
// ============================================================================
/// 压缩 repo sync 输出：保留锚点，消除进度噪音，项目映射为 PRJ:@hash。
fn compact_repo_sync(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }

        // 保留命令锚点（仅第一行）
        if trimmed.starts_with("repo sync") {
            if first {
                out.push(trimmed.to_string());
                first = false;
            }
            continue;
        }

        // 消除进度噪音：Downloading ..., Syncing: ...
        if trimmed.starts_with("Downloading") || trimmed.starts_with("Syncing:") {
            continue;
        }

        // 消除完成行
        if trimmed == "Syncing done." {
            continue;
        }

        // project 行: "project platform/frameworks/base/: HEAD is now at abc123def"
        if trimmed.starts_with("project ") {
            let rest = &trimmed["project ".len()..];
            if let Some(idx) = rest.find(": HEAD is now at ") {
                let project_path = rest[..idx].trim_end_matches('/');
                let hash = rest[idx + ": HEAD is now at ".len()..].trim();
                out.push(format!("PRJ:{} @{}", project_path, hash));
                continue;
            }
        }

        out.push(trimmed.to_string());
    }

    out.join("\n")
}

// ============================================================================
// Case 116: status — 保留锚点，扁平化项目状态与文件修改
// ============================================================================
/// 压缩 repo status 输出：项目行映射为 PRJ:BR:，文件修改映射为 M/A/D:。
fn compact_repo_status_cmd(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }

        // 保留命令锚点（仅第一行）
        if trimmed.starts_with("repo status") {
            if first {
                out.push(trimmed.to_string());
                first = false;
            }
            continue;
        }

        // project 行: "project platform/build/make/            branch master (clean)"
        if trimmed.starts_with("project ") {
            let rest = &trimmed["project ".len()..];
            if let Some(branch_idx) = rest.find("branch ") {
                let project_path = rest[..branch_idx].trim().trim_end_matches('/');
                let after_branch = rest[branch_idx + "branch ".len()..].trim();
                // 状态提取：branch <name> (<state>)
                if let Some(open_paren) = after_branch.rfind('(') {
                    let branch = after_branch[..open_paren].trim();
                    let state = &after_branch[open_paren + 1..];
                    let state = state.trim_end_matches(')').trim();
                    out.push(format!("PRJ:{} BR:{} ({})", project_path, branch, state));
                } else {
                    out.push(format!("PRJ:{} BR:{}", project_path, after_branch));
                }
            } else {
                out.push(trimmed.to_string());
            }
            continue;
        }

        // 文件修改行: " - Modified: src/SettingsActivity.java"
        // 压缩协议 V1 状态码映射: Modified→M, Added→A, Deleted→D
        // P3-194：旧实现 `trimmed.starts_with(" - ")` 在 `trim()` 后恒假（前导空白已去除），
        // 属死条件——`- Modified` 经 trim 变为 `- Modified`，由下方 `"- "` 分支覆盖，故删除冗余判断。
        if trimmed.starts_with("- ") {
            let clean = trimmed.trim_start_matches(|c: char| c == '-' || c.is_ascii_whitespace());
            if let Some(file) = clean.strip_prefix("Modified:") {
                out.push(format!("M:{}", file.trim()));
            } else if let Some(file) = clean.strip_prefix("Added:") {
                out.push(format!("A:{}", file.trim()));
            } else if let Some(file) = clean.strip_prefix("Deleted:") {
                out.push(format!("D:{}", file.trim()));
            }
            continue;
        }

        out.push(trimmed.to_string());
    }

    out.join("\n")
}

// ============================================================================
// Case 124: upload — 保留锚点，抹除 SSH URL，保留分支推送映射
// ============================================================================
/// 压缩 repo upload 输出：抹除 SSH URL，保留 HEAD -> refs 推送映射。
fn compact_repo_upload(raw: &str) -> String {
    let mut out = Vec::new();
    let mut current_project: Option<String> = None;

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }

        // 保留命令锚点
        if trimmed.starts_with("repo upload") {
            out.push(trimmed.to_string());
            continue;
        }

        // 跟踪当前项目
        if trimmed.starts_with("Upload project:") {
            let rest = &trimmed["Upload project:".len()..];
            current_project = Some(rest.trim().trim_end_matches('/').to_string());
            continue;
        }

        // 消除 SSH/HTTPS URL 噪音
        if trimmed.starts_with("To ssh://") || trimmed.starts_with("To https://") {
            continue;
        }

        // 推送映射行: " * [new branch] HEAD -> refs/changes/123/456/1"
        if trimmed.contains("HEAD -> refs/") {
            // P3-194：旧实现 `if let Some(proj) = &current_project` 在尚未跟踪到
            // 项目名时直接丢弃该行（无 else）。现无项目上下文的推送映射仍原样保留，
            // 仅在能关联到项目时加 PRJ 前缀。
            if let Some(proj) = &current_project {
                if let Some(ref_part) = trimmed.split("HEAD ->").nth(1) {
                    out.push(format!("PRJ:{}: HEAD ->{}", proj, ref_part.trim_end()));
                } else {
                    out.push(trimmed.to_string());
                }
            } else {
                out.push(trimmed.to_string());
            }
            continue;
        }

        // 跳过汇总行: "2 projects uploaded."
        if trimmed.ends_with("projects uploaded.") || trimmed.ends_with("project uploaded.") {
            continue;
        }

        // P3-194：未命中任何已知分支的行原样保留（旧实现无 else 直接静默丢弃，
        // 错误信息/意外输出/上传失败提示等会丢失，用户无从排查）。
        out.push(trimmed.to_string());
    }

    out.join("\n")
}

// ============================================================================
// 通用噪音过滤与 fallback（list / branches / diff / start / checkout / forall / stage / init）
// ============================================================================
/// 通用压缩：保留命令锚点，过滤噪音/URL，diff/hunk 压缩与警报映射。
fn compact_repo_generic(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }

        // 保留命令锚点（仅第一行）
        if trimmed.starts_with("repo ") && first {
            out.push(trimmed.to_string());
            first = false;
            continue;
        }

        // 消除噪音
        if is_repo_noise(trimmed) {
            continue;
        }

        // 消除 SSH/HTTP URL
        if trimmed.starts_with("ssh://")
            || trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
        {
            continue;
        }

        // diff 内容压缩
        if trimmed.starts_with("diff --git ") {
            if let Some(compact) = compact_diff_line(trimmed) {
                out.push(compact);
                continue;
            }
        }
        if trimmed.starts_with("index ")
            || trimmed.starts_with("--- ")
            || trimmed.starts_with("+++ ")
        {
            continue;
        }
        if trimmed.starts_with("@@") {
            if let Some(compact) = compact_hunk_header(trimmed) {
                out.push(compact);
                continue;
            }
        }
        if trimmed.starts_with('+') && !trimmed.starts_with("+++") {
            out.push(format!("+{}", trimmed[1..].trim()));
            continue;
        }
        if trimmed.starts_with('-') && !trimmed.starts_with("---") {
            out.push(format!("-{}", trimmed[1..].trim()));
            continue;
        }

        // 压缩协议 V1: 异常状态映射为 ! 前缀
        if let Some(alert) = map_repo_alert(trimmed) {
            out.push(alert);
            continue;
        }

        out.push(trimmed.to_string());
    }

    out.join("\n")
}

// ============================================================================
// 辅助函数
// ============================================================================

/// Repo 专属噪音检测：进度条、系统回显、叙述性文本
pub(super) fn is_repo_noise(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    // 下载与同步进度
    if lower.starts_with("downloading") || lower.starts_with("syncing:") {
        return true;
    }
    if lower == "syncing done." || lower == "listing projects ..." {
        return true;
    }
    // 暂存/上传辅助文本
    if lower.starts_with("staged changes in:") || lower.starts_with("upload project:") {
        return true;
    }
    // init 的环境叙述
    if lower.starts_with("repo initialized")
        || lower.starts_with("your identity")
        || lower.starts_with("will use a mirror")
    {
        return true;
    }
    // repo: 前缀的回显
    if lower.starts_with("repo:") {
        return true;
    }
    false
}

/// Compression Protocol V1: 异常状态映射
/// Conflict/Error/Failed/Rejected → ! 前缀
pub(super) fn map_repo_alert(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let triggers = ["conflict", "error:", "failed", "rejected"];
    if triggers.iter().any(|t| lower.contains(t)) {
        let cleaned = line.trim_start();
        if cleaned.starts_with('!') {
            Some(cleaned.to_string())
        } else {
            Some(format!("!{}", cleaned))
        }
    } else {
        None
    }
}

// ============================================================================
// diff / hunk 格式压缩（保留复用）
// ============================================================================
/// 压缩 diff --git 行为 D:file。
fn compact_diff_line(line: &str) -> Option<String> {
    if let Some(a_pos) = line.find(" a/") {
        let rest = &line[a_pos + 3..];
        if let Some(b_pos) = rest.find(" b/") {
            let file = &rest[..b_pos];
            return Some(format!("D:{}", file));
        }
    }
    None
}

/// 压缩 hunk 头行 @@ ... @@ 为 @@a->b@@。
fn compact_hunk_header(line: &str) -> Option<String> {
    if let Some(start) = line.find("@@ ") {
        let end = line.rfind(" @@").unwrap_or(line.len());
        let inner = &line[start + 3..end];
        return Some(format!("@@{}@@", inner.replace(" ", "->")));
    }
    None
}
