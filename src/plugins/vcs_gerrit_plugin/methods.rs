#![allow(dead_code)]
//! Gerrit 压缩方法 — Compression Protocol V1
use crate::core::plugin_config_loader::parse_vcs_command_words_from_line;
use regex::Regex;

/// 主入口：将 Gerrit 输出压缩为语义化紧凑格式
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_gerrit_log_for_ai(raw: &str) -> String {
    let cleaned = crate::core::utils::strip_ansi(raw);

    // 健壮性：过短输入直接返回，避免反向压缩
    if cleaned.len() < 50 {
        return cleaned;
    }

    let first_line = cleaned
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();

    // 按命令分发到专门的语义萃取器
    if let Some((tool, words)) = parse_vcs_command_words_from_line(first_line) {
        match (tool.as_str(), words.first().map(String::as_str)) {
            ("gerrit", Some("query")) => return compact_gerrit_query(&cleaned),
            ("gerrit", Some("review")) => return compact_gerrit_review(&cleaned),
            ("gerrit", Some("push")) | ("git", Some("push")) => {
                return compact_gerrit_push(&cleaned)
            }
            ("gerrit", Some("checkout")) => return compact_gerrit_checkout(&cleaned),
            _ => {}
        }
    }

    //  fallback：通用噪音过滤
    compact_gerrit_generic(&cleaned)
}

// ============================================================================
// 1. Query 语义萃取 (Case 97)
// ============================================================================
fn compact_gerrit_query(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;
    let mut current_change: Vec<String> = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }

        // 保留命令锚点
        if trimmed.starts_with("gerrit query") {
            if first {
                out.push(trimmed.to_string());
                first = false;
            }
            continue;
        }

        // 跳过噪音
        if is_gerrit_noise(trimmed) {
            continue;
        }

        // change 起始行: "change Iabc123def456789"
        if trimmed.starts_with("change ") {
            flush_change(&mut out, &mut current_change);
            let change_id = trimmed["change ".len()..].trim();
            current_change.push(format!("CHG @{}", change_id));
            continue;
        }

        // 缩进键值对: "  project: platform/frameworks/base"
        if let Some(compact) = compact_query_kv(trimmed) {
            current_change.push(compact);
            continue;
        }
    }

    flush_change(&mut out, &mut current_change);
    out.join("\n")
}

fn flush_change(out: &mut Vec<String>, buf: &mut Vec<String>) {
    if !buf.is_empty() {
        out.push(buf.join(" "));
        buf.clear();
    }
}

fn compact_query_kv(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let (k, v) = trimmed.split_once(':')?;
    let key = k.trim();
    let val = v.trim();
    if key.is_empty() || val.is_empty() {
        return None;
    }

    let sym = match key.to_ascii_lowercase().as_str() {
        "project" => "PRJ",
        "branch" => "BR",
        "status" => "ST",
        "owner" => "OW",
        "reviewers" => "RV",
        "subject" => "SJ",
        "updated" => "UP",
        "created" => "CR",
        "topic" => "TP",
        "id" => "ID",
        _ => key,
    };

    if sym == "RV" {
        let reviewers: Vec<&str> = val.split(',').map(|s| s.trim()).collect();
        return Some(format!("{}:{}", sym, reviewers.join(",")));
    }

    Some(format!("{}:{}", sym, val))
}

// ============================================================================
// 2. Review 语义萃取 (Case 125)
// ============================================================================
/// 压缩协议 V1: review 语义萃取 — 标签合并至命令锚点同行
fn compact_gerrit_review(raw: &str) -> String {
    let mut labels: Vec<String> = Vec::new();
    let mut command: Option<String> = None;

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }

        // 保留命令锚点（暂存，最后与标签同行输出）
        if trimmed.starts_with("gerrit review") {
            command = Some(trimmed.to_string());
            continue;
        }

        // 跳过噪音
        if is_gerrit_noise(trimmed) {
            continue;
        }

        // 标签行: "  Code-Review+2 (alice)"
        if let Some(compact) = compact_review_label(trimmed) {
            labels.push(compact);
            continue;
        }
    }

    // 压缩协议 V1: 标签结果合并至命令锚点同行
    match command {
        Some(cmd) if !labels.is_empty() => {
            format!("{}: {}", cmd, labels.join(", "))
        }
        Some(cmd) => cmd,
        None => labels.join(", "),
    }
}

fn compact_review_label(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let open = trimmed.find('(')?;
    let close = trimmed.rfind(')')?;
    let label_part = trimmed[..open].trim();
    let user = trimmed[open + 1..close].trim();

    let short = if label_part.starts_with("Code-Review") {
        label_part.replacen("Code-Review", "CR", 1)
    } else if label_part.starts_with("Verified") {
        label_part.replacen("Verified", "V", 1)
    } else {
        label_part.to_string()
    };

    Some(format!("{}@{}", short, user))
}

// ============================================================================
// 3. Push 语义萃取 (Case 126)
// ============================================================================
fn compact_gerrit_push(raw: &str) -> String {
    let mut out = Vec::new();
    let mut refs: Vec<String> = Vec::new();
    let mut pushed_count: Option<String> = None;

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }

        // 保留命令锚点
        if trimmed.starts_with("gerrit push") || trimmed.starts_with("git push") {
            out.push(trimmed.to_string());
            continue;
        }

        // 跳过噪音
        if is_gerrit_noise(trimmed) {
            continue;
        }

        // 跳过长 URL
        if trimmed.starts_with("Push to ssh://") || trimmed.starts_with("http") {
            continue;
        }

        // refs 映射行: "  refs/heads/master -> refs/heads/master"
        if trimmed.contains(" -> ") {
            if let Some(compact) = compact_push_ref(trimmed) {
                refs.push(compact);
            }
            continue;
        }

        // 计数行: "Pushed 3 refs"
        if trimmed.starts_with("Pushed ") {
            pushed_count = Some(trimmed.to_string());
            continue;
        }
    }

    if !refs.is_empty() {
        out.push(refs.join(", "));
    }
    if let Some(cnt) = pushed_count {
        out.push(cnt);
    }
    out.join("\n")
}

fn compact_push_ref(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let parts: Vec<&str> = trimmed.split(" -> ").collect();
    if parts.len() != 2 {
        return None;
    }
    let src = parts[0].trim();
    let dst = parts[1].trim();

    // 精简 refs/heads/ 前缀
    fn simplify_ref_path(s: &str) -> &str {
        if let Some(rest) = s.strip_prefix("refs/heads/") {
            rest
        } else if let Some(rest) = s.strip_prefix("refs/changes/") {
            rest
        } else {
            s
        }
    }

    let src_s = simplify_ref_path(src);
    let dst_s = simplify_ref_path(dst);

    if src_s == dst_s {
        // 如果是 changes 引用，加 @ 前缀表示修订号
        if src.starts_with("refs/changes/") {
            return Some(format!("@{}", src_s));
        }
        src_s.to_string().into()
    } else {
        Some(format!("{}->{}", src_s, dst_s))
    }
}

// ============================================================================
// 4. Checkout 语义萃取 (Case 127)
// ============================================================================
/// 压缩协议 V1: checkout 语义萃取 — 分支与状态合并至命令锚点同行
fn compact_gerrit_checkout(raw: &str) -> String {
    let mut command: Option<String> = None;
    let mut branch: Option<String> = None;
    let mut status: Option<String> = None;

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }

        // 保留命令锚点（暂存，最后与摘要同行输出）
        if trimmed.starts_with("gerrit checkout") {
            command = Some(trimmed.to_string());
            continue;
        }

        // "Switched to branch \"feature-xyz\""
        if trimmed.starts_with("Switched to branch") {
            if let Some(b) = extract_quoted(trimmed) {
                branch = Some(b);
            }
            continue;
        }

        // "Your branch is up to date with 'origin/feature-xyz'."
        if trimmed.starts_with("Your branch is") {
            if trimmed.contains("up to date") {
                status = Some("up-to-date".to_string());
            } else if trimmed.contains("ahead") {
                status = Some("ahead".to_string());
            } else if trimmed.contains("behind") {
                status = Some("behind".to_string());
            }
            continue;
        }
    }

    // 压缩协议 V1: 分支摘要合并至命令锚点同行
    match (command, branch) {
        (Some(cmd), Some(b)) => {
            let mut summary = format!("*{b}");
            if let Some(st) = status {
                summary.push_str(&format!(" ({st})"));
            }
            format!("{}: {}", cmd, summary)
        }
        (Some(cmd), None) => cmd,
        _ => String::new(),
    }
}

fn extract_quoted(line: &str) -> Option<String> {
    let re = Regex::new(r#""([^"]+)"|'(\[?[^\]]+\]?)'"#).ok()?;
    re.captures(line).and_then(|cap| {
        cap.get(1)
            .or_else(|| cap.get(2))
            .map(|m| m.as_str().to_string())
    })
}

// ============================================================================
// 通用噪音过滤与 fallback
// ============================================================================
/// 压缩协议 V1: 通用语义压缩 fallback
fn compact_gerrit_generic(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }

        // 保留命令锚点
        if trimmed.starts_with("gerrit ") || trimmed.starts_with("git push ") {
            if first {
                out.push(trimmed.to_string());
                first = false;
            }
            continue;
        }

        if is_gerrit_noise(trimmed) {
            continue;
        }

        // 进度条/传输中
        if trimmed.ends_with("...") && trimmed.len() < 40 {
            continue;
        }

        // 重复 URL
        if trimmed.starts_with("http") || trimmed.starts_with("URL:") {
            out.push(format!("URL:{}", abbreviate_url(trimmed)));
            continue;
        }

        // 压缩协议 V1: 异常状态映射为 ! 前缀
        if let Some(alert) = map_alert_line(trimmed) {
            out.push(alert);
            continue;
        }

        out.push(trimmed.to_string());
    }

    out.join("\n")
}

fn is_gerrit_noise(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if lower.starts_with("remote:") {
        let alert_like = ["error:", "fatal", "failed", "rejected", "conflict"];
        if alert_like.iter().any(|k| lower.contains(k)) {
            return false;
        }
    }
    let noise = [
        "description:",
        "commit message:",
        "diff:",
        "labels are now set",
        "transmitting",
        "counting objects",
        "writing objects",
        "remote:",
    ];
    noise.iter().any(|n| lower.starts_with(n))
}

fn abbreviate_url(url: &str) -> String {
    url.replace("https://gerrit.example.com/", "gr:")
        .replace("https://github.com/", "gh:")
        .replace("https://gitlab.com/", "gl:")
        .replace("https://dev.azure.com/", "az:")
        .replace("https://bitbucket.org/", "bb:")
        .replace("ssh://review.example.com:29418/", "ssh:gr:")
}

/// 压缩协议 V1: 异常状态映射
/// Conflict/Error/Failed/Rejected → ! 前缀
pub(super) fn map_alert_line(line: &str) -> Option<String> {
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

/// 压缩协议 V1: 文件状态码映射
/// Modified→M, Added→A, Deleted→D, Renamed→R
#[allow(dead_code)]
pub(super) fn map_file_status(status: &str) -> &str {
    match status {
        "Modified" | "modified" | "M" => "M",
        "Added" | "added" | "A" => "A",
        "Deleted" | "deleted" | "D" => "D",
        "Renamed" | "renamed" | "R" => "R",
        _ => status,
    }
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_gerrit_other_for_ai(raw: &str) -> String {
    compact_gerrit_log_for_ai(raw)
}

#[tracing::instrument(level = "debug", skip_all)]
pub fn is_gerrit_log_block(_: &str) -> bool {
    true
}
