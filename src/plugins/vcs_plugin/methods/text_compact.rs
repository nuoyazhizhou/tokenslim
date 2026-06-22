fn compact_vcs_text_with_paths(
    plugin: &VcsPlugin,
    text: &str,
    dict_engine: &mut DictionaryEngine,
    arena: &Bump,
    compact_inline_alignment: bool,
) -> String {
    // 法则 C：ANSI 净化 — 剥离所有 ANSI 逃逸序列和终端颜色代码
    let text = strip_ansi_escapes(&text);
    
    let mut out = bumpalo::collections::String::new_in(arena);
    let mut blank_count = 0usize;
    let mut ws_ctx = WsCompactionContext::default();
    let mut last_remote_url = String::new();

    for raw in text.lines() {
        let mut line = raw.trim_end_matches('\r').to_string();

        if is_decorative_line(&line) || is_git_network_progress_line(&line) {
            continue;
        }

        let patch_payload_line = is_patch_payload_line(&line, &ws_ctx);

        update_ws_context_from_line(&line, &mut ws_ctx);

        if plugin.config.compact_leading_ws && !patch_payload_line {
            line = compact_leading_ws_with_language_guard(&line, &ws_ctx);
            if compact_inline_alignment {
                line = compact_inline_alignment_ws(&line);
            }
        }

        if !patch_payload_line {
            line = compact_common_vcs_ack_line(&line);
            if line.is_empty()
                && raw
                    .trim_start()
                    .starts_with("Transmitting file data")
            {
                continue;
            }
        }

        let is_blank = line.trim().is_empty() && !patch_payload_line;
        if plugin.config.collapse_blank_lines && is_blank {
            blank_count += 1;
            if blank_count > plugin.config.max_blank_lines {
                continue;
            }
        } else {
            blank_count = 0;
        }

        let rewritten = if patch_payload_line {
            line
        } else {
            let mut l = plugin.rewrite_line_with_paths(&line, dict_engine);
            if l.starts_with("Date: ") || l.starts_with("patch ") || l.starts_with("AuthorDate: ") || l.starts_with("CommitDate: ") {
                if let Some(pos) = l.rfind(' ') {
                    let last = l[pos + 1..].trim_matches(|c| c == '(' || c == ')');
                    let is_tz = (last.len() == 5 || last.len() == 6) 
                        && (last.starts_with('+') || last.starts_with('-')) 
                        && last[1..].chars().all(|c| c.is_ascii_digit() || c == ':');
                    let is_named = last.len() >= 2 && last.len() <= 5 
                        && last.chars().all(|c| c.is_ascii_alphabetic() && c.is_ascii_uppercase());
                    if is_tz || is_named {
                        l.truncate(pos);
                        l = l.trim_end().to_string();
                    }
                }
            }
            l
        };

        if rewritten.starts_with("To http") || rewritten.starts_with("From http") {
            if rewritten == last_remote_url {
                continue;
            }
            last_remote_url = rewritten.clone();
        }

        out.push_str(&rewritten);
        out.push('\n');
    }

    // 法则 A 扩展：邻近行去重，防止如 git add 等命令输出重复状态行
    dedup_adjacent_lines(&out.into_bump_str())
}

/// 邻近行去重：移除连续出现的相同行
/// 例如 git add 输出多次相同路径信息时，只保留首次出现的行
fn dedup_adjacent_lines(text: &str) -> String {
    let mut prev = "";
    text.lines()
        .filter(|&line| {
            let is_dup = line == prev;
            if !is_dup {
                prev = line;
            }
            !is_dup
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl VcsPlugin {
    fn commands_for(&self, tool: VcsTool) -> Option<&[String]> {
        self.command_whitelists.get(&tool).map(|v| v.as_slice())
    }
}

fn command_hits(lower_text: &str, commands: &[String]) -> usize {
    commands
        .iter()
        .filter(|cmd| {
            let cmd_lower = cmd.to_ascii_lowercase();
            lower_text.contains(&format!(" {} ", cmd_lower))
                || lower_text.contains(&format!(" {}\n", cmd_lower))
                || lower_text.contains(&format!("\n{} ", cmd_lower))
        })
        .count()
}

fn infer_tool_from_explicit_command(text: &str) -> Option<VcsTool> {
    let first = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim_start_matches('\u{feff}')
        .trim_end_matches('\r')
        .trim();
    let tokens = parse_command_line_tokens(first)?;
    let tool = tokens.first()?;
    let keyword = normalize_command_keyword(tool);
    vcs_tool_from_keyword(&keyword)
}

fn first_non_empty_line(text: &str) -> &str {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim_start()
}

fn first_explicit_command_line(text: &str) -> Option<&str> {
    let first = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim_start())?;
    if infer_tool_from_explicit_command(first).is_some() {
        Some(first.trim_end_matches('\r'))
    } else {
        None
    }
}

fn command_lines_equivalent(lhs: &str, rhs: &str) -> bool {
    let normalize = |s: &str| {
        s.split_whitespace()
            .map(|t| t.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(" ")
    };
    normalize(lhs) == normalize(rhs)
}

fn command_lines_share_verb(lhs: &str, rhs: &str) -> bool {
    let lhs_tokens: Vec<String> = lhs
        .split_whitespace()
        .map(|t| t.to_ascii_lowercase())
        .collect();
    let rhs_tokens: Vec<String> = rhs
        .split_whitespace()
        .map(|t| t.to_ascii_lowercase())
        .collect();
    if lhs_tokens.is_empty() || rhs_tokens.is_empty() {
        return false;
    }
    if lhs_tokens[0] != rhs_tokens[0] {
        return false;
    }
    if lhs_tokens.len() >= 2 && rhs_tokens.len() >= 2 {
        return lhs_tokens[1] == rhs_tokens[1];
    }
    lhs_tokens.len() == rhs_tokens.len()
}

fn parse_diff_r_range_marker(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if !trimmed.to_ascii_lowercase().starts_with("diff ") {
        return None;
    }

    // Supports both:
    //   diff -r 17:abc -r 18:def file
    //   diff -r17:abc -r18:def file
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let mut from_rev: Option<&str> = None;
    let mut to_rev: Option<&str> = None;
    let mut i = 0usize;
    while i < parts.len() {
        let p = parts[i];
        if p == "-r" {
            if i + 1 < parts.len() {
                if from_rev.is_none() {
                    from_rev = Some(parts[i + 1]);
                } else if to_rev.is_none() {
                    to_rev = Some(parts[i + 1]);
                }
                i += 2;
                continue;
            }
            break;
        }
        if let Some(rest) = p.strip_prefix("-r") {
            if !rest.is_empty() {
                if from_rev.is_none() {
                    from_rev = Some(rest);
                } else if to_rev.is_none() {
                    to_rev = Some(rest);
                }
            }
        }
        i += 1;
    }

    match (from_rev, to_rev) {
        (Some(a), Some(b)) => Some((a.to_string(), b.to_string())),
        _ => None,
    }
}

fn parse_p4_diff_pair_marker(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("====") {
        return None;
    }

    let inner = trimmed.trim_matches('=').trim();
    let (left, right) = inner.split_once(" - ")?;
    let left = left.trim();
    let right = right.trim();
    if left.is_empty() || right.is_empty() {
        return None;
    }

    if !(left.contains('#') || right.contains('#')) {
        return None;
    }

    Some((left.to_string(), right.to_string()))
}

fn normalize_diff_scope_marker(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some((from_rev, to_rev)) = parse_diff_r_range_marker(trimmed) {
        return Some(format!("revs: {} -> {}", from_rev, to_rev));
    }
    if let Some((from_rev, to_rev)) = parse_p4_diff_pair_marker(trimmed) {
        return Some(format!("revs: {} -> {}", from_rev, to_rev));
    }

    let lower = trimmed.to_ascii_lowercase();
    if (trimmed.starts_with("--- ") || trimmed.starts_with("+++ "))
        && lower.contains("(revision ")
        && lower.ends_with(')')
    {
        let side = if trimmed.starts_with("--- ") {
            "old-rev"
        } else {
            "new-rev"
        };
        if let Some(start) = lower.rfind("(revision ") {
            let start_idx = start + "(revision ".len();
            if start_idx <= trimmed.len() {
                let rev = trimmed[start_idx..].trim_end_matches(')').trim();
                if !rev.is_empty() {
                    return Some(format!("{side}: {rev}"));
                }
            }
        }
    }
    if trimmed.starts_with("+++ ") && lower.ends_with("(working copy)") {
        return Some("new-rev: working-copy".to_string());
    }
    if let Some(rest) = lower.strip_prefix("old revision:") {
        let start = trimmed.len() - rest.len();
        let rev = trimmed[start..].trim();
        if !rev.is_empty() {
            return Some(format!("old-rev: {}", rev));
        }
    }
    if let Some(rest) = lower.strip_prefix("new revision:") {
        let start = trimmed.len() - rest.len();
        let rev = trimmed[start..].trim();
        if !rev.is_empty() {
            return Some(format!("new-rev: {}", rev));
        }
    }
    if let Some(rest) = lower.strip_prefix("retrieving revision ") {
        let start = trimmed.len() - rest.len();
        let rev = trimmed[start..].trim();
        if !rev.is_empty() {
            return Some(format!("rev: {}", rev));
        }
    }
    if lower.starts_with("change ") && lower.contains(" on ") && lower.contains(" by ") {
        return Some(MULTI_HWS_RE.replace_all(trimmed, " ").to_string());
    }

    None
}

fn preserve_diff_scope_metadata(raw: &str, compacted: String) -> String {
    let mut markers: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    let command = first_explicit_command_line(raw).map(|s| s.trim());

    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }
        if command.is_some_and(|cmd| command_lines_equivalent(trimmed, cmd)) {
            continue;
        }

        if let Some(marker) = normalize_diff_scope_marker(trimmed) {
            if seen.insert(marker.clone()) {
                markers.push(marker);
            }
        }
    }

    if markers.is_empty() {
        return compacted;
    }

    let mut present = HashSet::new();
    for line in compacted.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        present.insert(trimmed.to_ascii_lowercase());
    }

    let mut pending: Vec<String> = Vec::new();
    for marker in markers {
        if !present.contains(&marker.to_ascii_lowercase()) {
            pending.push(marker);
        }
    }

    if pending.is_empty() {
        return compacted;
    }

    if compacted.trim().is_empty() {
        return pending.join("\n");
    }

    let lines: Vec<&str> = compacted.lines().collect();
    let mut insert_at: Option<usize> = None;
    if let Some(cmd) = command {
        for (idx, line) in lines.iter().enumerate() {
            if command_lines_equivalent(line.trim(), cmd) {
                insert_at = Some(idx + 1);
                break;
            }
        }
    } else if !lines.is_empty() {
        insert_at = Some(1);
    }

    let mut out: Vec<String> = Vec::with_capacity(lines.len() + pending.len());
    if let Some(pos) = insert_at {
        for (idx, line) in lines.iter().enumerate() {
            out.push((*line).to_string());
            if idx + 1 == pos {
                for marker in &pending {
                    out.push(marker.clone());
                }
            }
        }
    } else {
        for marker in pending {
            out.push(marker);
        }
        for line in lines {
            out.push(line.to_string());
        }
    }

    out.join("\n")
}

fn preserve_p4_diff_revision_scope(raw: &str, compacted: String) -> String {
    if !command_is(raw, "p4", "diff2") {
        return compacted;
    }

    // Already carries explicit file revision scope (`#rev`) in compact text.
    if compacted.contains('#') {
        return compacted;
    }

    let mut marker: Option<String> = None;
    for line in raw.lines() {
        if let Some((from_rev, to_rev)) = parse_p4_diff_pair_marker(line) {
            marker = Some(format!("revs: {} -> {}", from_rev, to_rev));
            break;
        }
    }
    let Some(marker) = marker else {
        return compacted;
    };

    let mut present = HashSet::new();
    for line in compacted.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            present.insert(trimmed.to_ascii_lowercase());
        }
    }
    if present.contains(&marker.to_ascii_lowercase()) {
        return compacted;
    }

    if compacted.trim().is_empty() {
        return marker;
    }

    let command = first_explicit_command_line(raw).map(|s| s.trim());
    let lines: Vec<&str> = compacted.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 1);
    let mut inserted = false;

    if let Some(cmd_line) = command {
        for line in lines.iter() {
            out.push((*line).to_string());
            if !inserted && command_lines_equivalent(line.trim(), cmd_line) {
                out.push(marker.clone());
                inserted = true;
            }
        }
    } else {
        if let Some((first, rest)) = lines.split_first() {
            out.push((*first).to_string());
            out.push(marker.clone());
            for line in rest {
                out.push((*line).to_string());
            }
            inserted = true;
        }
    }

    if !inserted {
        out.insert(0, marker);
    }

    out.join("\n")
}

fn ensure_explicit_command_header(raw: &str, rendered: String) -> String {
    let Some(command) = first_explicit_command_line(raw) else {
        return rendered;
    };

    let first_rendered = rendered
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim_start().trim_end_matches('\r'));
    if first_rendered.is_some_and(|line| {
        command_lines_equivalent(line, command) || command_lines_share_verb(line, command)
    }) {
        return rendered;
    }

    if rendered.trim().is_empty() {
        return command.to_string();
    }

    let body = rendered.trim_start_matches(|c| c == '\n' || c == '\r');
    let mut out = String::with_capacity(command.len() + 1 + body.len());
    out.push_str(command);
    out.push('\n');
    out.push_str(body);
    out
}

fn is_git_status_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("on branch ")
        || lower.contains("changes not staged for commit:")
        || lower.contains("untracked files:")
}

fn is_git_status_fragment(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("untracked files:")
        || lower.contains("changes not staged for commit:")
        || lower.contains("changes to be committed:")
        || lower.contains("(use \"git add")
        || lower.contains("(use \"git restore")
        || lower.contains("updating files:")
        || lower.contains("restored files:")
    {
        return true;
    }

    text.lines().any(|line| {
        let line = line.trim_end_matches('\r');
        if STATUS_PATH_RE.is_match(line) {
            return true;
        }
        if let Some(caps) = SHORT_STATUS_RE.captures(line.trim_start()) {
            if let Some(path) = caps.name("path") {
                return looks_like_vcs_path(path.as_str());
            }
        }
        false
    })
}

fn is_git_log_block(text: &str) -> bool {
    if text
        .lines()
        .any(|line| line.trim_start().starts_with("git rebase -i "))
    {
        return true;
    }

    let mut commit_lines = 0usize;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("commit ") {
            let hash = trimmed
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .trim_matches(|c| c == '(' || c == ')');
            if hash.len() >= 7 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                commit_lines += 1;
                if commit_lines >= 1 {
                    return true;
                }
            }
        }
    }
    false
}

fn is_git_diff_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("diff --git ")
        || lower.contains("@@ -")
        || lower.contains("+++ b/")
        || lower.contains("--- a/")
        || is_git_name_only_or_status_block(text)
}

fn is_git_show_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let has_commit = text
        .lines()
        .any(|line| line.trim_start().starts_with("commit "));
    has_commit
        && (is_git_diff_block(text)
            || lower.contains(" files changed")
            || lower.contains(" file changed")
            || lower.contains("insertion(+)")
            || lower.contains("deletion(-)")
            || text.lines().any(|line| looks_like_vcs_path(line.trim())))
}

fn is_git_name_only_or_status_block(text: &str) -> bool {
    let mut non_empty = 0usize;
    let mut matched = 0usize;

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r').trim();
        if line.is_empty() {
            continue;
        }
        non_empty += 1;

        if looks_like_git_name_status_line(line) || looks_like_vcs_path(line) {
            matched += 1;
            continue;
        }

        return false;
    }

    non_empty > 0 && non_empty == matched
}

fn looks_like_git_name_status_line(line: &str) -> bool {
    let mut parts = line.split_whitespace();
    let status = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();

    if path.is_empty() {
        return false;
    }

    let status_ok = (1..=2).contains(&status.len())
        && status
            .chars()
            .all(|c| matches!(c, 'M' | 'A' | 'D' | 'R' | 'C' | 'T' | 'U' | 'X' | '?'));

    status_ok && looks_like_vcs_path(path)
}

fn has_prefixed_lines(text: &str, prefixes: &[&str], min_matches: usize) -> bool {
    let mut matches = 0usize;
    for line in text.lines() {
        let trimmed = line.trim();
        if prefixes.iter().any(|prefix| trimmed.starts_with(prefix)) {
            matches += 1;
            if matches >= min_matches {
                return true;
            }
        }
    }

    false
}

fn looks_like_svn_blame_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return false;
    }

    let mut parts = trimmed.split_whitespace();
    let revision = parts.next().unwrap_or_default();
    let author = parts.next().unwrap_or_default();
    let content = parts.next().unwrap_or_default();

    !author.is_empty()
        && !content.is_empty()
        && (revision == "-" || revision.chars().all(|c| c.is_ascii_digit()))
}

fn looks_like_svn_list_entry(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.len() >= 4
        && (tokens[0] == "-" || looks_like_iso_date_token_for_list(tokens[0]))
        && tokens
            .last()
            .map(|path| path.ends_with('/') || looks_like_vcs_path(path))
            .unwrap_or(false)
    {
        return true;
    }

    !trimmed.contains(char::is_whitespace)
        && (trimmed.ends_with('/') || looks_like_vcs_path(trimmed))
}

fn looks_like_iso_date_token_for_list(token: &str) -> bool {
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

fn looks_like_p4_where_mapping_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.starts_with("//") {
        return false;
    }

    let mut parts = trimmed.split_whitespace();
    let depot = parts.next().unwrap_or_default();
    let client = parts.next().unwrap_or_default();
    let local = parts.next().unwrap_or_default();
    depot.starts_with("//") && client.starts_with("//") && !local.is_empty()
}

fn is_svn_status_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("svn status") {
        return true;
    }

    // Disambiguation: generic `M path` lines also appear in git short status.
    // Only treat as SVN without explicit command when SVN-specific hints exist.
    if !(lower.contains("checked out revision") || lower.contains("w155")) {
        return false;
    }

    text.lines().any(|raw| {
        let line = raw.trim_end_matches('\r').trim();
        looks_like_git_name_status_line(line)
    })
}

fn is_svn_diff_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("svn diff") || lower.contains("index: ") || lower.contains("@@ -")
}

fn is_svn_log_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("svn log")
        || text
            .lines()
            .any(|line| line.trim_start().starts_with('r') && line.contains('|'))
}

fn is_svn_blame_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("svn blame") || text.lines().any(looks_like_svn_blame_line)
}

fn is_svn_list_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("svn list") || text.lines().any(looks_like_svn_list_entry)
}

fn is_svn_prop_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("svn propget")
        || lower.contains("svn proplist")
        || text
            .lines()
            .any(|line| line.trim_start().starts_with("Properties on "))
}

fn is_svn_info_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("svn info")
        || has_prefixed_lines(
            text,
            &[
                "Path:",
                "URL:",
                "Repository Root:",
                "Revision:",
                "Node Kind:",
            ],
            2,
        )
}

fn is_hg_status_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("hg status") {
        return true;
    }

    text.lines().any(|raw| {
        let line = raw.trim_end_matches('\r').trim();
        looks_like_git_name_status_line(line)
    })
}

fn is_hg_diff_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("hg diff") || lower.contains("diff -r ") || lower.contains("@@ -")
}

fn is_hg_log_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("hg log") || lower.contains("changeset:") || lower.contains("summary:")
}

#[allow(dead_code)]
fn is_hg_heads_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("hg heads") || (lower.contains("changeset:") && lower.contains("branch:"))
}

#[allow(dead_code)]
fn is_hg_outgoing_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("hg outgoing")
        || (lower.contains("comparing with") && lower.contains("searching for changes"))
}

#[allow(dead_code)]
fn is_hg_incoming_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("hg incoming")
        || (lower.contains("comparing with") && lower.contains("searching for changes"))
}

#[allow(dead_code)]
fn is_hg_parents_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("hg parents") || lower.contains("parent:")
}

fn is_p4_opened_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("p4 opened") || text.contains("... //")
}

fn is_p4_describe_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("p4 describe")
        || (text
            .lines()
            .any(|line| line.trim_start().starts_with("Change "))
            && (lower.contains("affected files") || lower.contains("differences")))
}

fn is_p4_changes_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("p4 changes")
        || text.lines().any(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("Change ") && trimmed.contains(" on ") && trimmed.contains(" by ")
        })
}

fn is_p4_fstat_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("p4 fstat")
        || text.lines().any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("... depotFile ")
                || trimmed.starts_with("... headChange ")
                || trimmed.starts_with("... action ")
        })
}

fn is_p4_where_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("p4 where") || text.lines().any(looks_like_p4_where_mapping_line)
}

fn is_p4_info_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("p4 info")
        || has_prefixed_lines(
            text,
            &[
                "User name:",
                "Client name:",
                "Client root:",
                "Server address:",
                "Server version:",
            ],
            2,
        )
}

fn is_p4_labels_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("p4 labels")
        || text
            .lines()
            .any(|line| line.trim_start().starts_with("Label "))
}

fn is_p4_dirs_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("p4 dirs") {
        return true;
    }

    let mut matched = 0usize;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("//") && !trimmed.contains(' ') {
            matched += 1;
            continue;
        }
        return false;
    }

    matched > 0
}

fn is_cvs_status_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("cvs status")
        || lower.contains("cvs update")
        || text.lines().any(|line| {
            let t = line.trim_start();
            let mut chars = t.chars();
            if let Some(ch) = chars.next() {
                matches!(ch, 'M' | 'A' | 'D' | 'R' | 'C' | '!' | '?')
                    && chars.next().is_some_and(|c| c.is_whitespace())
            } else {
                false
            }
        })
}

fn is_cvs_diff_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("cvs diff")
        || (text.lines().any(|l| l.trim_start().starts_with("Index:"))
            && text.lines().any(|l| {
                let t = l.trim_start();
                t.starts_with("*** ") || t.starts_with("--- ") || t.starts_with("@@")
            }))
}

fn is_cvs_log_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("cvs log")
        || (text
            .lines()
            .any(|l| l.trim_start().starts_with("revision "))
            && text
                .lines()
                .any(|l| l.trim_start().to_ascii_lowercase().starts_with("date:")))
}

fn is_bzr_status_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("bzr status")
        || lower.contains("bzr st")
        || has_prefixed_lines(text, &["modified:", "added:", "removed:"], 1)
}

fn is_bzr_diff_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("bzr diff")
        || (text
            .lines()
            .any(|l| l.trim_start().starts_with("=== modified file"))
            && text.lines().any(|l| {
                let t = l.trim_start();
                t.starts_with("--- ") || t.starts_with("+++ ") || t.starts_with("@@")
            }))
}

fn is_bzr_log_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("bzr log")
        || (text
            .lines()
            .any(|l| l.trim_start().to_ascii_lowercase().starts_with("revno:"))
            && text.lines().any(|l| {
                l.trim_start()
                    .to_ascii_lowercase()
                    .starts_with("committer:")
            }))
}

fn is_fossil_status_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("fossil status")
        || lower.contains("fossil changes")
        || has_prefixed_lines(text, &["ADDED", "EDITED", "DELETED", "MISSING"], 1)
}

fn is_fossil_diff_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("fossil diff")
        || (text.lines().any(|l| l.trim_start().starts_with("Index:"))
            && text.lines().any(|l| {
                let t = l.trim_start();
                t.starts_with("--- ") || t.starts_with("+++ ") || t.starts_with("@@")
            }))
}

fn is_fossil_log_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("fossil timeline")
        || (text.lines().any(|l| l.contains("["))
            && text
                .lines()
                .any(|l| l.to_ascii_lowercase().contains("user:")))
}

fn is_darcs_status_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if command_is(text, "darcs", "log") || command_is(text, "darcs", "changes") {
        return false;
    }
    lower.contains("darcs whatsnew") || has_prefixed_lines(text, &["A ", "M ", "R "], 1)
}

fn is_darcs_diff_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("darcs diff")
        || (text.lines().any(|l| l.trim_start().starts_with("--- "))
            && text.lines().any(|l| l.trim_start().starts_with("+++ ")))
}

fn is_darcs_log_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("darcs changes")
        || lower.contains("darcs log")
        || (text
            .lines()
            .any(|l| l.trim_start().to_ascii_lowercase().starts_with("author:"))
            && text
                .lines()
                .any(|l| l.trim_start().to_ascii_lowercase().starts_with("date:")))
}

/// 法则 C：剥离 ANSI 逃逸序列
/// 移除所有 \x1b[...m 格式的终端颜色/样式代码
static ANSI_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

fn strip_ansi_escapes(text: &str) -> String {
    ANSI_RE.replace_all(text, "").to_string()
}

/// 默认最大单文件 Diff 行数（法则 F）
#[allow(dead_code)]
const MAX_DIFF_LINES: usize = 100;

#[allow(dead_code)]
fn compact_with_parser<P: VcsParser>(parser: P, raw: &str) -> String {
    if let Some(doc) = parser.parse(raw) {
        let normalized = VcsRuleEngine::normalize(doc);
        let rendered = VcsRuleEngine::render_text(&normalized);
        if rendered.trim().is_empty() {
            raw.to_string()
        } else {
            // 法则 E：若清理废话后输出仅剩命令锚点，追加 ST:[CLEAN]
            let first_line = raw.lines().next().map(|l| l.trim()).unwrap_or("");
            let body = rendered.trim();
            if body.lines().count() <= 1 && body == first_line {
                format!("{}\nST:[CLEAN]", first_line)
            } else {
                rendered
            }
        }
    } else {
        raw.to_string()
    }
}

