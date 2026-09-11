#![allow(dead_code)]
//! Perforce (P4) 压缩方法 — Compression Protocol V1
use super::parser::*;
use crate::core::path_compressor::types::PathCompressor;
use crate::core::plugin_config_loader::parse_vcs_command_words_from_line;

/// 对 P4 压缩输出中的 depot 路径执行字典压缩
/// P4 depot 路径（如 //depot/main/src/...）天然具备公共前缀，是 $P Token 的完美标的
/// 将 min_prefix_length 降低至 10 以适应 P4 的短层级路径结构
#[tracing::instrument(level = "debug", skip_all)]
pub fn compress_depot_paths(text: &str) -> String {
    if text.is_empty() {
        return text.to_string();
    }
    let mut compressor = PathCompressor::new();
    compressor.set_min_prefix_length(10); // P4 路径前缀通常较短（如 //depot/main/ 14 chars）
    compressor.set_min_occurrences(2); // 至少出现 2 次才提取为公共前缀
    compressor.extract_and_compress_from_text(text)
}

/// 用 VcsParser 解析文本并渲染记录；空记录返回原文。
pub fn process_parser(parser: &dyn VcsParser, raw: &str) -> String {
    if let Some(doc) = parser.parse(raw) {
        let out: String = doc.records.iter().map(|r| format!("{}\n", r)).collect();
        let t = out.trim();
        if t.is_empty() {
            raw.to_string()
        } else {
            t.to_string()
        }
    } else {
        raw.to_string()
    }
}
/// 成本门控：输出比原文短才采用，否则回退原文。
fn cost_gate(raw: &str, output: String) -> String {
    if output.len() < raw.len() {
        output
    } else {
        raw.to_string()
    }
}

/// 锚点守卫：强制在输出开头保留首行命令锚点。
fn anchor_guard(raw: &str, f: impl FnOnce() -> String) -> String {
    let anchor = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");
    let body = f();
    // 空输出或无锚点 → 直接返回
    if body.is_empty() || anchor.is_empty() {
        return body;
    }
    // 命令锚点必须 100% 保留，不计压缩代价
    if body.starts_with(anchor) {
        return body;
    }
    format!("{}\n{}", anchor, body.trim_start())
}

/// 判断是否为 p4 叙述噪音行（change created/submitted/sync completed 等）。
fn is_p4_narrative_noise(t: &str) -> bool {
    let l = t.to_ascii_lowercase();
    // 显式分组确认意图（&& 优先级高于 ||），避免后续维护误读（Q31 处置）
    (l.starts_with("change ") && (l.contains("created.") || l.contains("submitted.")))
        || (l.contains("shelve change") && l.contains("created."))
        || l.contains("unshelved change")
        || l == "sync completed."
        || l.starts_with("affected files")
        || l.starts_with("differences")
        || l.starts_with("shelved files")
        || l.starts_with("updated files")
        || l.ends_with("files restored.")
        || l.ends_with("files updated.")
        || l.ends_with("files would be updated.")
        || l.ends_with("files reverted.")
        || l.ends_with("files resolved")
        || l.contains("conflict remaining")
        || l.contains("default change") && !l.starts_with("...") && !l.starts_with("//")
}

// ============================================================================
// 公开 API
// ============================================================================

/// 判断给定文本行是否为 p4 命令行（以 "p4 " 开头且后续非空白）
#[inline]
pub fn is_p4_command_line(line: &str) -> bool {
    let t = line.trim();
    // "p4 " 前缀 + 后续命令名非空
    t.starts_with("p4 ") && t.len() > 3 && !t[3..].trim_start().is_empty()
}

/// 压缩 p4 opened 输出：锚点守卫 + opened 行解析为状态码:路径。
pub fn compact_p4_opened_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_opened(raw))
}
/// 压缩 p4 describe 输出：锚点守卫 + describe 解析（Change 头符号化 + diff 保留）。
pub fn compact_p4_describe_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_describe(raw))
}
/// 压缩 p4 fstat 输出：锚点守卫 + fstat 属性拍扁为单行。
pub fn compact_p4_fstat_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_fstat(raw))
}
/// 压缩 p4 where 输出：P4WhereParser 解析 + 锚点守卫。
pub fn compact_p4_where_for_ai(raw: &str) -> String {
    anchor_guard(raw, || process_parser(&P4WhereParser, raw))
}
/// 压缩 p4 info 输出：P4InfoParser 解析 + 锚点守卫。
pub fn compact_p4_info_for_ai(raw: &str) -> String {
    anchor_guard(raw, || process_parser(&P4InfoParser, raw))
}
/// 压缩 p4 sync 输出：锚点守卫 + sync 行解析。
pub fn compact_p4_sync_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_sync(raw))
}
/// 压缩 p4 submit 输出：锚点守卫 + submit 命令解析。
pub fn compact_p4_submit_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_submit_cmd(raw))
}
/// 压缩 p4 shelve 输出：锚点守卫 + 复用 submit 命令解析。
pub fn compact_p4_shelve_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_submit_cmd(raw))
}
/// 压缩 p4 unshelve 输出：P4UnshelveParser 解析 + 锚点守卫。
pub fn compact_p4_unshelve_for_ai(raw: &str) -> String {
    anchor_guard(raw, || process_parser(&P4UnshelveParser, raw))
}
/// 压缩 p4 resolve 输出：P4ResolveParser 解析 + 锚点守卫。
pub fn compact_p4_resolve_for_ai(raw: &str) -> String {
    anchor_guard(raw, || process_parser(&P4ResolveParser, raw))
}
/// 压缩 p4 revert 输出：P4RevertParser 解析 + 锚点守卫。
pub fn compact_p4_revert_for_ai(raw: &str) -> String {
    anchor_guard(raw, || process_parser(&P4RevertParser, raw))
}
/// 压缩 p4 edit 输出：简单状态解析 + 锚点守卫。
pub fn compact_p4_edit_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_simple_status(raw))
}
/// 压缩 p4 add 输出：简单状态解析 + 锚点守卫。
pub fn compact_p4_add_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_simple_status(raw))
}
/// 压缩 p4 delete 输出：简单状态解析 + 锚点守卫。
pub fn compact_p4_delete_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_simple_status(raw))
}
/// 压缩 p4 labels 输出：解析标签行并压缩时间戳，成本门控。
pub fn compact_p4_labels_for_ai(raw: &str) -> String {
    anchor_guard(raw, || {
        let base = process_parser(&P4LabelsParser, raw);
        let mut out = String::new();
        for line in base.lines() {
            let mut s = line.to_string();
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() >= 4
                && parts.get(1).map_or(false, |p| p.contains('/'))
                && parts
                    .get(2)
                    .map_or(false, |p| p.len() == 8 && p.matches(':').count() == 2)
            {
                let mut r = vec![parts[0], parts[1]];
                r.push(&parts[2][..5]);
                r.push(parts[3]);
                if parts.len() > 4 {
                    r.extend_from_slice(&parts[4..]);
                }
                s = r.join(" ");
            }
            out.push_str(&s);
            out.push('\n');
        }
        if !raw.ends_with('\n') && out.ends_with('\n') {
            out.pop();
        }
        cost_gate(raw, out)
    })
}
/// 压缩 p4 dirs 输出：解析目录行并提取公共根前缀。
pub fn compact_p4_dirs_for_ai(raw: &str) -> String {
    anchor_guard(raw, || {
        let b = process_parser(&P4DirsParser, raw);
        maybe_factor_p4_dirs_root(b)
    })
}
/// 压缩 p4 changes 输出：Change 头符号化为 CH/CR/OW/ST/CM 单行。
pub fn compact_p4_changes_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_changes(raw))
}
/// 压缩 p4 filelog 输出：filelog 行解析。
pub fn compact_p4_filelog_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_filelog(raw))
}
/// 压缩 p4 files 输出：files 行解析。
pub fn compact_p4_files_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_files(raw))
}
/// 压缩 p4 move 输出：move 行解析（path -> path）。
pub fn compact_p4_move_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_move_cmd(raw))
}
/// 压缩 p4 copy 输出：copy 行解析（path -> path）。
pub fn compact_p4_copy_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_copy_cmd(raw))
}
/// 压缩 p4 integrate 输出：复用 copy 解析（格式相同）。
pub fn compact_p4_integrate_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_copy_cmd(raw)) // integrate 与 copy 格式相同: path -> path
}
/// 压缩 p4 users 输出：users 行解析。
pub fn compact_p4_users_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_users(raw))
}
/// 压缩 p4 workspaces 输出：workspaces 行解析。
pub fn compact_p4_workspaces_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_workspaces(raw))
}
/// 压缩 p4 depot 输出：depot 行解析。
pub fn compact_p4_depot_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_depot(raw))
}
/// 压缩 p4 context diff 输出：上下文 diff 压缩。
pub fn compact_p4_context_diff_for_ai(raw: &str) -> String {
    anchor_guard(raw, || compact_p4_context_diff(raw))
}

// ============================================================================
// handlers
// ============================================================================
/// 压缩 p4 describe 正文：Change 头符号化、depot 路径动作映射、diff 块保留。
fn compact_p4_describe(raw: &str) -> String {
    let mut out = Vec::new();
    let mut current_cm: Option<String> = None; // 当前在收集的 CM: 行
    let mut in_patch = false; // 是否已进入 diff/patch 块

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") {
            continue;
        }

        let l = t.to_ascii_lowercase();

        // === 噪音标题行 ===
        if l.starts_with("affected files")
            || l.starts_with("differences")
            || l.starts_with("shelved files")
            || l.starts_with("updated files")
        {
            // 结束当前 CM: 收集
            if let Some(cm) = current_cm.take() {
                out.push(cm);
            }
            in_patch = false;
            continue;
        }

        // === Change 头部行 → 符号化 ===
        if let Some(rest) = t.strip_prefix("Change ") {
            if let Some(cm) = current_cm.take() {
                out.push(cm);
            }
            in_patch = false;

            let (id, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let id = id.trim();
            let (date, remainder) = extract_change_field_on(args);
            let (user, remainder) = extract_change_field_by(&remainder);
            let remaining = remainder.trim().trim_matches('\'');

            // 强制加上 CM: 占位，哪怕 remaining 是空的，这样后续行才知道往哪里拼！
            let entry = format!("CH:{} CR:{} OW:@{} CM:{}", id, date, user, remaining);
            current_cm = Some(entry.trim().to_string());
            continue;
        }

        // === 已进入 patch 块：直接保留 diff/patch 行 ===
        if in_patch {
            // 跳过 ---/+++ 路径行（DIFF: 已标识文件）
            if t.starts_with("--- ") || t.starts_with("+++ ") {
                continue;
            }
            // 下一个 ==== 头 → 重新进入 DIFF 头处理
            if t.starts_with("==== ") && t.ends_with(" ====") {
                if let Some(diff) = compact_p4_diff_head(t) {
                    out.push(diff);
                }
                continue;
            }
            out.push(t.to_string());
            continue;
        }

        // === diff 头处理：==== 行 → DIFF: ====
        if t.starts_with("==== ") && t.ends_with(" ====") {
            if let Some(diff) = compact_p4_diff_head(t) {
                // 提交之前收集的 CM:
                if let Some(cm) = current_cm.take() {
                    out.push(cm);
                }
                out.push(diff);
                in_patch = true;
            }
            continue;
        }

        // === depot 文件路径行（以 // 或 ... // 开头） ====
        if t.starts_with("//") || t.starts_with("... //") {
            if let Some(cm) = current_cm.take() {
                out.push(cm);
            }
            // 提取动作并映射为状态前缀
            let action_code = detect_p4_opened_action_from_line(t);
            let cleaned = t.trim_start_matches("...").trim();
            // 剔除版本号 #N 和动作后缀
            let path_str = if let Some(hash_pos) = cleaned.find('#') {
                cleaned[..hash_pos].to_string()
            } else if let Some(space_pos) = cleaned.find(' ') {
                cleaned[..space_pos].to_string()
            } else {
                cleaned.to_string()
            };
            out.push(format!("{}:{}", action_code, path_str));
            continue;
        }

        // === CM: 续行：当前有 CM: 上下文时，续接文本 ===
        if let Some(ref mut cm) = current_cm {
            *cm = format!("{} {}", cm, t);
            continue;
        }

        // === 纯描述/正文行（非上述类型）→ 保留 ===
        out.push(t.to_string());
    }

    // 提交最后的 CM:
    if let Some(cm) = current_cm.take() {
        out.push(cm);
    }

    if out.is_empty() {
        raw.to_string()
    } else {
        out.join("\n")
    }
}

/// 压缩 p4 opened 正文：剥离省略号前缀，解析路径与动作映射为状态码。
fn compact_p4_opened(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") {
            continue;
        }
        if is_p4_narrative_noise(t) {
            continue;
        }
        // 剥离前导 ... 或 Unicode 省略号
        let cleaned = if let Some(s) = t.strip_prefix("... ") {
            s.trim_start().to_string()
        } else if let Some(s) = t.strip_prefix("...") {
            s.trim_start().to_string()
        } else if t.starts_with('\u{2026}') {
            t[3..].trim_start().to_string()
        } else {
            t.to_string()
        };

        let result = if cleaned.starts_with("//") {
            // 分支 A: "//depot/...#N action default change (text)" 格式
            if let Some((path_part, _)) = cleaned.split_once(" default change") {
                compact_p4_opened_path_action(path_part.trim())
            }
            // 分支 B: "//depot/...#N on changelist by user@client" 格式
            else if let Some((path_part, _)) = cleaned.split_once(" on ") {
                let clean_path = path_part.split('#').next().unwrap_or(path_part).trim();
                let action_code = detect_p4_opened_action_from_line(&cleaned);
                format!("{}:{}", action_code, clean_path)
            }
            // 分支 C: "//depot/path - action" 格式
            else if let Some((path, _)) = cleaned.split_once(" - ") {
                let code = detect_p4_opened_action_from_line(&cleaned);
                format!("{}:{}", code, path.trim())
            }
            // 分支 D: "//depot/...#N action" 简写格式（无 "default change"）
            else {
                compact_p4_opened_path_action(&cleaned)
            }
        } else {
            cleaned
        };
        out.push(result);
    }
    if out.is_empty() {
        raw.to_string()
    } else {
        out.join("\n")
    }
}

/// 从行中检测 opened 动作（delete→D/add→A/move→R/其他→M）。
fn detect_p4_opened_action_from_line(line: &str) -> &'static str {
    let lower = line.to_ascii_lowercase();
    if lower.contains("delete") || lower.contains("deleted") {
        "D"
    } else if lower.contains("add") || lower.contains("added") {
        "A"
    } else if lower.contains("move") || lower.contains("moved") {
        "R"
    } else {
        "M"
    }
}

/// 按动词后缀（move/rename/edit/add/delete/integrate）映射路径与状态码。
fn compact_p4_opened_path_action(input: &str) -> String {
    let actions: &[(&str, &str)] = &[
        ("move/rename", "R"),
        ("move", "R"),
        ("rename", "R"),
        ("edit", "M"),
        ("add", "A"),
        ("delete", "D"),
        ("integrate", "I"),
    ];
    for (verb, code) in actions {
        let suffix = format!(" {}", verb);
        if input.ends_with(&suffix) {
            let raw_path = &input[..input.len() - suffix.len()];
            let final_path = raw_path.split('#').next().unwrap_or(raw_path).trim();
            return format!("{}:{}", code, final_path);
        }
    }
    let path = input.split('#').next().unwrap_or(input).trim();
    path.to_string()
}

/// 压缩简单状态行（edit/add/delete 的 "path - opened for X" 格式）。
fn compact_p4_simple_status(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") {
            continue;
        }
        if is_p4_narrative_noise(t) {
            continue;
        }
        if let Some((path, action)) = t.split_once(" - ") {
            let code = match action.trim().to_ascii_lowercase().as_str() {
                "opened for edit" => "M",
                "added for add" => "A",
                "deleted for delete" => "D",
                _ => "",
            };
            if !code.is_empty() {
                out.push(format!("{}:{}", code, path.trim()));
            } else {
                out.push(t.to_string());
            }
            continue;
        }
        out.push(t.to_string());
    }
    if out.is_empty() {
        raw.to_string()
    } else {
        out.join("\n")
    }
}

/// 压缩 p4 changes 正文：Change 头解析并拼接 CM 续行。
fn compact_p4_changes(raw: &str) -> String {
    let mut out = Vec::new();
    let mut current_entry: Option<String> = None;

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") {
            continue;
        }
        if is_p4_narrative_noise(t) {
            continue;
        }

        if let Some(args) = t.strip_prefix("Change ") {
            // 提交前一条记录
            if let Some(entry) = current_entry.take() {
                out.push(entry);
            }

            // 提取 Change ID（第一个空白前的 token）
            let (id, args) = args.split_once(char::is_whitespace).unwrap_or((args, ""));
            let id = id.trim();

            // 提取日期：兼容 "on DATE by USER" 和 "by USER on DATE" 两种格式
            let (date, remainder) = extract_change_field_on(args);
            // 提取作者：兼容 "by user@workspace" 和 "by user" 格式
            let (user, remainder) = extract_change_field_by(&remainder);
            // 提取状态和描述：兼容 *status* 'desc' 和 'desc' *status* 两种顺序
            let (status, desc) = extract_change_meta_strict(&remainder);
            let status = if status.is_empty() {
                "submitted"
            } else {
                &status
            };

            // 构建单行记录，CM: 字段总在末尾，等待续行拼接
            let mut entry = format!(
                "CH:{} CR:{} OW:@{} ST:{} CM:{}",
                id, date, user, status, desc
            );
            entry = entry.trim().to_string();
            current_entry = Some(entry);
            continue;
        }

        // 处于 Change 上下文中 → 所有非空行无脑拼接到 CM: 后面，不允许断行
        if let Some(ref mut entry) = current_entry {
            // CM: 为空时直接拼接（不添加空格），否则用空格分隔
            if entry.ends_with(':') {
                entry.push_str(t);
            } else {
                entry.push(' ');
                entry.push_str(t);
            }
            continue;
        }

        out.push(t.to_string());
    }

    if let Some(entry) = current_entry.take() {
        out.push(entry);
    }

    if out.is_empty() {
        raw.to_string()
    } else {
        out.join("\n")
    }
}

/// 从 P4 changes 行中提取日期字段（支持 "on DATE by" 和 "by USER on DATE" 两种格式）
/// 返回 (压缩后日期, 剩余文本)
/// 压缩后日期格式: YYYYMMDD（剔除 / 分隔符，符合 Protocol V1 法则 B）
#[tracing::instrument(level = "debug", skip_all)]
fn extract_change_field_on(args: &str) -> (String, String) {
    let args = args.trim();
    let (after, preceding) = if let Some(a) = args.strip_prefix("on ") {
        (a, "")
    } else if let Some(pos) = args.find(" on ") {
        (&args[pos + 4..], &args[..pos])
    } else {
        return (String::new(), args.to_string());
    };
    let rest = after.trim();
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    // 法则 P4-5: 日期必须剔除 / 分隔符，输出如 20260403
    let date_raw = rest[..end].to_string();
    let date = date_raw.replace("/", "");
    // 跳过紧随日期的时间戳 (HH:MM[:SS])，防止泄漏到 CM:
    let tail = skip_time_token(&rest[end..]);
    let remainder = if preceding.is_empty() {
        tail.to_string()
    } else {
        format!("{}{}", preceding, tail)
    };
    (date, remainder.trim().to_string())
}

/// 若 s 以 HH:MM:SS 或 HH:MM 开头，返回跳过时间后的剩余文本；否则返回 s
fn skip_time_token(s: &str) -> &str {
    let s = s.trim_start();
    let b = s.as_bytes();
    // HH:MM:SS (8 chars)
    if b.len() >= 8
        && b[0].is_ascii_digit()
        && b[1].is_ascii_digit()
        && b[2] == b':'
        && b[3].is_ascii_digit()
        && b[4].is_ascii_digit()
        && b[5] == b':'
        && b[6].is_ascii_digit()
        && b[7].is_ascii_digit()
    {
        return s[8..].trim_start();
    }
    // HH:MM (5 chars)
    if b.len() >= 5
        && b[0].is_ascii_digit()
        && b[1].is_ascii_digit()
        && b[2] == b':'
        && b[3].is_ascii_digit()
        && b[4].is_ascii_digit()
    {
        return s[5..].trim_start();
    }
    s
}

/// 从 args 中提取 "on DATE" 字段（去除 / 分隔符并跳过时间戳）。
fn extract_change_field_by(args: &str) -> (String, String) {
    let args = args.trim();
    let (after, preceding) = if let Some(a) = args.strip_prefix("by ") {
        (a, "")
    } else if let Some(pos) = args.find(" by ") {
        (&args[pos + 4..], &args[..pos])
    } else {
        return (String::new(), args.to_string());
    };
    let end = after
        .find(|c: char| c == ' ' || c == '*' || c == '@' || c == '\'')
        .unwrap_or(after.len());
    let user = after[..end].to_string();
    let remaining = if end < after.len() && after.as_bytes()[end] == b'@' {
        let after_at = &after[end + 1..];
        let s = after_at
            .find(|c: char| c == ' ' || c == '*')
            .unwrap_or(after_at.len());
        &after_at[s..]
    } else {
        &after[end..]
    };
    let remainder = if preceding.is_empty() {
        remaining.trim().to_string()
    } else {
        format!("{}{}", preceding, remaining).trim().to_string()
    };
    (user, remainder)
}

/// 从 args 中提取 "by USER" 字段（兼容 @workspace 后缀）。
fn extract_change_meta(args: &str) -> (String, String) {
    // 保留旧函数签名以供其他调用方使用
    extract_change_meta_strict(args)
}

/// 强化版状态/描述提取：兼容 *STATUS* 'DESC' 和 'DESC' *STATUS* 两种顺序
fn extract_change_meta_strict(args: &str) -> (String, String) {
    let args = args.trim();
    let mut status = String::new();
    let mut desc = String::new();

    // 先尝试提取 *STATUS* 标记
    let rest_after_status = if let Some(after_star) = args.strip_prefix('*') {
        if let Some(end) = after_star.find('*') {
            status = after_star[..end].trim().to_string();
            let rest = after_star[end + 1..].trim();
            rest.to_string()
        } else {
            args.to_string()
        }
    } else {
        args.to_string()
    };

    // 再尝试提取 'DESC' 单引号内容
    let remaining = if let Some(after_q) = rest_after_status.strip_prefix('\'') {
        // 从后往前找最后一个单引号（处理描述内含单引号的情况）
        if let Some(end) = after_q.rfind('\'') {
            desc = after_q[..end].trim().to_string();
            after_q[end + 1..].trim().to_string()
        } else {
            rest_after_status.to_string()
        }
    } else {
        rest_after_status
    };

    // 如果还没提取到 status（说明 order 是 'DESC' *STATUS*），从剩余部分再试
    if status.is_empty() && !remaining.is_empty() {
        if let Some(after_star) = remaining.strip_prefix('*') {
            if let Some(end) = after_star.find('*') {
                status = after_star[..end].trim().to_string();
            }
        }
    }

    // 如果 desc 仍为空且 remaining 有内容，尝试无引号提取（去除首尾空白作为 desc）
    if desc.is_empty() && !remaining.is_empty() {
        let cleaned = remaining.trim().trim_matches('\'');
        if !cleaned.is_empty() {
            desc = cleaned.to_string();
        }
    }

    (status, desc)
}

/// 缩写 fstat 键名（depotFile→DF、headAction→ACT 等，冗余键丢弃）。
fn abbreviate_fstat_key(key: &str) -> Option<&str> {
    match key {
        "depotFile" => Some("DF"),
        "clientFile" => None, // 冗余信息，丢弃
        "headAction" => Some("ACT"),
        "headType" => Some("TYPE"),
        "headTime" => Some("TIME"),
        "headRev" => Some("REV"),
        "headChange" => Some("CHANGE"),
        "haveRev" => Some("HAVE"),
        "action" => None, // 块末尾残留行 → 丢弃
        _ => Some(key),
    }
}

/// P4 fstat 压缩：每个 depot 文件的所有属性拍扁到单行（法则 P4-2）
/// 标准格式输出: DF://depot/path REV:8 ACT:edit TYPE:text CHANGE:12340 HAVE:7
/// -T 格式输出: DF://depot/path TYPE:text REV:5
#[tracing::instrument(level = "debug", skip_all)]
fn compact_p4_fstat(raw: &str) -> String {
    let mut out = Vec::new();
    // 当前正在收集的文件块
    let mut current_path: Option<String> = None;
    let mut props: Vec<String> = Vec::new();

    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") {
            continue;
        }
        if is_p4_narrative_noise(t) {
            continue;
        }

        // 格式 A: 标准 fstat — "... key value"
        if t.starts_with("... ") || t.starts_with('\u{2026}') {
            let cleaned = if t.starts_with("... ") {
                t[4..].trim_start()
            } else {
                t[3..].trim_start()
            };
            if let Some((key, value)) = cleaned.split_once(char::is_whitespace) {
                match abbreviate_fstat_key(key) {
                    Some("DF") => {
                        // 新文件块开始 → 先 flush 上一个
                        flush_fstat_block(&mut out, &mut current_path, &mut props);
                        current_path = Some(value.trim().to_string());
                    }
                    Some(short) => {
                        props.push(format!("{}:{}", short, value.trim()));
                    }
                    None => {} // clientFile / action → 丢弃
                }
            }
            continue;
        }

        // 格式 B: fstat -T 单行 — "//path ... key1 value1 key2 value2 ..."
        if let Some(dot_pos) = t.find(" ... ") {
            // 新行 → 先 flush 上一个
            flush_fstat_block(&mut out, &mut current_path, &mut props);

            let path_part = &t[..dot_pos].trim();
            current_path = Some(path_part.to_string());

            let meta_part = t[dot_pos + 5..].trim(); // 跳过 " ... "
            let tokens: Vec<&str> = meta_part.split_whitespace().collect();
            let mut i = 0;
            while i + 1 < tokens.len() {
                let key = tokens[i];
                let value = tokens[i + 1];
                // -T 格式中 depotFile 的路径已从 path_part 提取，跳过避免 DF: 重复
                if key == "depotFile" {
                    i += 2;
                    continue;
                }
                if let Some(short) = abbreviate_fstat_key(key) {
                    props.push(format!("{}:{}", short, value));
                }
                i += 2;
            }
            // 立即 flush（-T 格式每行独立）
            flush_fstat_block(&mut out, &mut current_path, &mut props);
            continue;
        }

        // 格式 C: 纯路径行（如 fstat -T 首行的路径）→ 保留
        if t.starts_with("//") && !t.contains(' ') {
            out.push(t.to_string());
            continue;
        }

        // 降级：通用 "key value" 处理
        if let Some((key, value)) = t.split_once(char::is_whitespace) {
            if let Some(short) = abbreviate_fstat_key(key) {
                out.push(format!("{}: {}", short, value.trim()));
            }
        } else {
            out.push(t.to_string());
        }
    }

    // 提交最后一个文件块
    flush_fstat_block(&mut out, &mut current_path, &mut props);

    if out.is_empty() {
        raw.to_string()
    } else {
        out.join("\n")
    }
}

/// 将当前文件块的累积属性拍扁为单行输出
fn flush_fstat_block(
    out: &mut Vec<String>,
    current_path: &mut Option<String>,
    props: &mut Vec<String>,
) {
    if let Some(path) = current_path.take() {
        if props.is_empty() {
            // 只有路径，没有属性 → 保留路径
            out.push(path.to_string());
        } else {
            // 法则 P4-2: DF: + 路径 + 空格分隔的 KV 对
            let line = format!("DF:{} {}", path, props.join(" "));
            out.push(line);
        }
        props.clear();
    }
}

/// 压缩 p4 filelog 正文。
fn compact_p4_filelog(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") {
            continue;
        }
        if is_p4_narrative_noise(t) {
            continue;
        }
        // 格式: "... #<rev> change <ch> <action> <date> by <user>"
        // 深度粉碎为: "#<rev>|CH:<ch>|<code>|<yyyy-mm-dd>|@<user>"
        if let Some(after_dots) = t.strip_prefix("... ") {
            let s = after_dots.trim();
            if let Some(rest) = s.strip_prefix('#') {
                if let Some((rev, rest)) = rest.split_once(char::is_whitespace) {
                    let rest = rest.trim();
                    if let Some(after_change) = rest.strip_prefix("change ") {
                        let tokens: Vec<&str> = after_change.split_whitespace().collect();
                        // tokens: [ch, action, date, "by", user]
                        if tokens.len() >= 5 && tokens[3] == "by" {
                            let ch = tokens[0];
                            let action_code = match tokens[1].to_ascii_lowercase().as_str() {
                                "edit" => "M",
                                "add" => "A",
                                "delete" => "D",
                                "integrate" => "I",
                                "branch" => "B",
                                _ => tokens[1],
                            };
                            let date = tokens[2].replace('/', "-");
                            let user = tokens[4];
                            out.push(format!(
                                "#{}|CH:{}|{}|{}|@{}",
                                rev, ch, action_code, date, user
                            ));
                            continue;
                        }
                    }
                }
            }
        }
        out.push(t.to_string());
    }
    if out.is_empty() {
        raw.to_string()
    } else {
        out.join("\n")
    }
}

/// 压缩 p4 submit 正文。
fn compact_p4_submit_cmd(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") {
            continue;
        }
        // 提交成功确认行 "Change <n> submitted." 携带关键变更号，符号化为 SUB:<id>，
        // 不能作为叙述噪音丢弃（否则 LLM 无法得知本次 submit 产出的 changelist 号）。
        // "Change <n> created." 是提交前的中转状态，仍按噪音过滤。
        let lower = t.to_ascii_lowercase();
        if lower.starts_with("change ") && lower.ends_with(" submitted.") {
            let id = t["Change ".len()..]
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
            if !id.is_empty() {
                out.push(format!("SUB:{}", id));
                continue;
            }
        }
        if is_p4_narrative_noise(t) {
            continue;
        }
        out.push(t.to_string());
    }
    out.join("\n")
}

/// p4 files 压缩: #N - add/edit → A:/M:/D: 状态前缀 + 路径（法则 P4-3）
#[tracing::instrument(level = "debug", skip_all)]
fn compact_p4_files(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") {
            continue;
        }
        if is_p4_narrative_noise(t) {
            continue;
        }
        // 格式: //depot/main/src/main.rs#2 - edit
        if let Some((left, right)) = t.split_once(" - ") {
            let path = left.split('#').next().unwrap_or(left).trim();
            let action = right.trim().to_ascii_lowercase();
            let code = match action.as_str() {
                "add" => "A",
                "edit" => "M",
                "delete" => "D",
                "branch" => "B",
                "integrate" => "I",
                "move/add" => "A",
                "move/delete" => "D",
                _ => "",
            };
            if code.is_empty() {
                out.push(t.to_string());
            } else {
                out.push(format!("{}:{}", code, path));
            }
            continue;
        }
        out.push(t.to_string());
    }
    if out.is_empty() {
        raw.to_string()
    } else {
        out.join("\n")
    }
}

/// p4 move 压缩: #N - moved from //source → R://dest <- //source（法则 P4-3）
#[tracing::instrument(level = "debug", skip_all)]
fn compact_p4_move_cmd(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") {
            continue;
        }
        if is_p4_narrative_noise(t) {
            continue;
        }
        // 格式: //depot/main/dest.txt#1 - moved from //depot/main/source.txt
        if let Some((left, right)) = t.split_once(" - ") {
            let dest = left.split('#').next().unwrap_or(left).trim();
            let source = right
                .trim()
                .strip_prefix("moved from ")
                .unwrap_or(right.trim());
            out.push(format!("R:{} <- {}", dest, source.trim()));
            continue;
        }
        out.push(t.to_string());
    }
    if out.is_empty() {
        raw.to_string()
    } else {
        out.join("\n")
    }
}

/// p4 copy / integrate 压缩: //source -> //dest → C://source -> //dest（法则 P4-3）
#[tracing::instrument(level = "debug", skip_all)]
fn compact_p4_copy_cmd(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") {
            continue;
        }
        if is_p4_narrative_noise(t) {
            continue;
        }
        // 格式: //depot/main/src/main.rs -> //depot/feature/src/main.rs
        // 仅处理 depot 路径间的箭头（避免匹配 Rust 函数返回类型中的 ->）
        if t.starts_with("//") && t.contains(" -> ") {
            // 剔除版本号 #N
            let parts: Vec<&str> = t.split(" -> ").collect();
            if parts.len() == 2 {
                let src = parts[0].split('#').next().unwrap_or(parts[0]).trim();
                let dst = parts[1].split('#').next().unwrap_or(parts[1]).trim();
                out.push(format!("C:{} -> {}", src, dst));
                continue;
            }
        }
        out.push(t.to_string());
    }
    if out.is_empty() {
        raw.to_string()
    } else {
        out.join("\n")
    }
}

/// p4 sync 压缩：支持 updated/added/deleted/updating 等多种动作符号化（法则 P4-3）
/// sync -n 格式: //depot/main/src/main.rs#5 - updating /home/user/project/... → M://depot/main/src/main.rs
#[tracing::instrument(level = "debug", skip_all)]
fn compact_p4_sync(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") {
            continue;
        }
        if is_p4_narrative_noise(t) {
            continue;
        }
        // 拦截摘要行: "N files would be updated." / "N files updated."
        let l = t.to_ascii_lowercase();
        if l.ends_with("files would be updated.") || l.ends_with("files updated.") {
            continue;
        }
        // 格式 A: "path - action"（本地路径格式，如 src/main.rs - updated）
        if let Some((path, action)) = t.split_once(" - ") {
            let action_lower = action.trim().to_ascii_lowercase();
            let code = if action_lower == "updated" || action_lower.starts_with("updated ") {
                "M"
            } else if action_lower == "added" || action_lower.starts_with("added ") {
                "A"
            } else if action_lower == "deleted" || action_lower.starts_with("deleted ") {
                "D"
            } else {
                ""
            };
            if !code.is_empty() {
                // 剔除 #N 版本号
                let clean_path = path.trim().split('#').next().unwrap_or(path.trim()).trim();
                out.push(format!("{}:{}", code, clean_path));
                continue;
            }
        }
        // 格式 B: "//depot/path#N - updating /home/..." （sync -n depot 格式）
        if t.starts_with("//") && t.contains(" - ") {
            if let Some((path, action)) = t.split_once(" - ") {
                let action_lower = action.trim().to_ascii_lowercase();
                let code = if action_lower.starts_with("updating")
                    || action_lower.starts_with("updated")
                {
                    "M"
                } else if action_lower.starts_with("adding") || action_lower.starts_with("added") {
                    "A"
                } else if action_lower.starts_with("deleting")
                    || action_lower.starts_with("deleted")
                {
                    "D"
                } else {
                    ""
                };
                if !code.is_empty() {
                    let clean_path = path.trim().split('#').next().unwrap_or(path.trim()).trim();
                    out.push(format!("{}:{}", code, clean_path));
                    continue;
                }
            }
        }
        out.push(t.to_string());
    }
    out.join("\n")
}

/// p4 users 压缩：多空格降维 + 邮箱域名截断（法则 P4-8）
/// 格式: alice   Alice Chen   alice@example.com   -- → alice Alice Chen alice --
#[tracing::instrument(level = "debug", skip_all)]
fn compact_p4_users(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") {
            continue;
        }
        // 压缩连缀空格为单空格
        let compacted: Vec<&str> = t.split_whitespace().collect();
        let mut processed = Vec::new();
        for word in compacted {
            // 邮箱降维：user@domain → user
            if word.contains('@') {
                processed.push(word.split('@').next().unwrap_or(word).to_string());
            } else {
                processed.push(word.to_string());
            }
        }
        out.push(processed.join(" "));
    }
    if out.is_empty() {
        raw.to_string()
    } else {
        out.join("\n")
    }
}

/// p4 workspaces 压缩：多空格降维
/// 格式: alice-ws    alice    /home/alice/workspace → alice-ws alice /home/alice/workspace
#[tracing::instrument(level = "debug", skip_all)]
fn compact_p4_workspaces(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") {
            continue;
        }
        // 压缩连缀空格为单空格
        out.push(t.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    if out.is_empty() {
        raw.to_string()
    } else {
        out.join("\n")
    }
}

/// p4 depot 压缩：表头剥离 + 多空格降维
/// 格式: Depot depottype description → 丢弃; //depot/main/    local    Main dev → //depot/main/ local Main dev
#[tracing::instrument(level = "debug", skip_all)]
fn compact_p4_depot(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") {
            continue;
        }
        // 剥离纯视觉表头 "Depot depottype description"（含多空格对齐变体）
        let normalized: String = t.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized == "Depot depottype description" {
            continue;
        }
        // 压缩连缀空格为单空格
        out.push(t.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    if out.is_empty() {
        raw.to_string()
    } else {
        out.join("\n")
    }
}

/// p4 diff -dc（Context Diff）头部降维（法则 P4-7）
/// 格式: *************** 和 *** N,M **** 分隔符 → 丢弃；保留 +/- 行及上下文行
#[tracing::instrument(level = "debug", skip_all)]
fn compact_p4_context_diff(raw: &str) -> String {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("p4 ") {
            continue;
        }
        // 丢弃 Context Diff 纯分隔符行（全为 *）：它不携带语义，仅起视觉分区作用。
        if t.chars().all(|c| c == '*') && t.len() >= 5 {
            continue;
        }
        // Context Diff hunk 头 "*** N,M ****"：行号区间 N-M 是决策信息（标识改动落在
        // 哪些行段），必须保留（法则 P4-3 语义保真）。否则只能看到 +/- 行却不知改动位置。
        if t.starts_with("*** ") && t.ends_with(" ****") {
            out.push(t.to_string());
            continue;
        }
        out.push(t.to_string());
    }
    if out.is_empty() {
        raw.to_string()
    } else {
        out.join("\n")
    }
}

/// 压缩 p4 diff 头行（
fn compact_p4_diff_head(line: &str) -> Option<String> {
    let t = line.trim();
    // 标准 diff: "==== //path#N (text) ===="
    if t.starts_with("==== ") && t.ends_with(" ====") {
        let inner = &t[5..t.len() - 5].trim();
        if let Some((depot, _)) = inner.split_once(" - ") {
            let depot = depot.trim().split('#').next().unwrap_or(depot).trim();
            return if depot.starts_with("//") {
                Some(format!("DIFF:{}", depot))
            } else {
                None
            };
        }
        if inner.starts_with("//") {
            let d = inner.split('#').next().unwrap_or(inner).trim();
            return Some(format!("DIFF:{}", d));
        }
        return None;
    }
    // diff2: "==== //path1#N - //path2#M"（无尾缀 ====）
    // 修订号 #N/#M 标识参与对比的文件版本，属于决策信息，必须保留（法则 P4-3 语义保真）。
    if t.starts_with("==== ") {
        let inner = t[5..].trim();
        if let Some((left, right)) = inner.split_once(" - ") {
            let left = left.trim();
            let right = right.trim();
            if left.starts_with("//") && right.starts_with("//") {
                return Some(format!("DIFF2:{} - {}", left, right));
            }
        }
        if let Some((left, _right)) = inner.split_once(" - ") {
            if left.starts_with("//") {
                return Some(format!("DIFF2:{}", left));
            }
        }
    }
    None
}

// ============================================================================
// 分发逻辑
// ============================================================================
/// p4 branch / p4 label 实体压缩（专用，替代通用回退）：
/// 结构均为 `Branch|Label <name> <date> '<desc>'` + `View: <depot 路径>`。
/// - 日期 `YYYY/MM/DD` 符号化为 8 位 `YYYYMMDD`（与 describe CR: 日期风格一致）
/// - 头行首标 `BR:` / `LB:` 作为实体指纹，便于下游区分命令来源
/// - View 行前缀缩为 `V:`，路径交给字典压缩提取公共 depot 前缀
/// - 分支名 / 描述 / View 路径全量保留（分支语义决策所需，对应 scenario expected_keep）
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_p4_branch_label_for_ai(raw: &str) -> String {
    anchor_guard(raw, || {
        let mut out = Vec::new();
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            if t.starts_with("p4 ") {
                out.push(t.to_string());
                continue;
            }
            if let Some(rest) = t
                .strip_prefix("Branch ")
                .or_else(|| t.strip_prefix("Label "))
            {
                let head = if t.starts_with("Branch ") { "BR" } else { "LB" };
                let words: Vec<&str> = rest.split_whitespace().collect();
                // words = [name, date, desc...]；日期符号化为 8 位
                if words.len() >= 2 {
                    let date = words[1].replace('/', "");
                    let mut parts = vec![format!("{}:{}", head, words[0]), date];
                    if words.len() > 2 {
                        parts.push(words[2..].join(" "));
                    }
                    out.push(parts.join(" "));
                } else {
                    out.push(format!("{}:{}", head, rest));
                }
                continue;
            }
            if let Some(view) = t.strip_prefix("View:") {
                out.push(format!("V:{}", compress_depot_paths(view.trim())));
                continue;
            }
            out.push(t.to_string());
        }
        if out.is_empty() {
            raw.to_string()
        } else {
            out.join("\n")
        }
    })
}
/// p4 tag 压缩：将冗长确认行 `Tag <name> marked on <N> files.` 符号化为 `TAG:<name> N:<count>`。
/// tag 名与受影响文件条数是决策信息，必须保留（法则 P4-3 语义保真）。
#[tracing::instrument(level = "debug", skip_all)]
pub fn compact_p4_tag_for_ai(raw: &str) -> String {
    anchor_guard(raw, || {
        let mut out = Vec::new();
        for line in raw.lines() {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            if t.starts_with("p4 ") {
                continue;
            }
            // 精简确认行: "Tag v1.0.0 marked on 50 files." → "TAG:v1.0.0 N:50"
            if let Some(rest) = t.strip_prefix("Tag ") {
                if let Some(mid) = rest.find(" marked on ") {
                    let name = rest[..mid].trim();
                    let tail = rest[mid + " marked on ".len()..].trim();
                    let count = tail
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim_end_matches('.');
                    if !name.is_empty() && !count.is_empty() {
                        out.push(format!("TAG:{} N:{}", name, count));
                        continue;
                    }
                }
            }
            out.push(t.to_string());
        }
        if out.is_empty() {
            raw.to_string()
        } else {
            out.join("\n")
        }
    })
}
/// p4 other 家族压缩入口：按块类型分派。
pub fn compact_p4_other_for_ai(raw: &str) -> String {
    if is_p4_fstat_block(raw) {
        return compact_p4_fstat_for_ai(raw);
    }
    if is_p4_where_block(raw) {
        return compact_p4_where_for_ai(raw);
    }
    if is_p4_info_block(raw) {
        return compact_p4_info_for_ai(raw);
    }
    if is_p4_dirs_block(raw) {
        return compact_p4_dirs_for_ai(raw);
    }
    if is_p4_users_block(raw) {
        return compact_p4_users_for_ai(raw);
    }
    if is_p4_workspaces_block(raw) {
        return compact_p4_workspaces_for_ai(raw);
    }
    if is_p4_depot_block(raw) {
        return compact_p4_depot_for_ai(raw);
    }
    if is_p4_context_diff_block(raw) {
        return compact_p4_context_diff_for_ai(raw);
    }
    if is_p4_files_block(raw) {
        return compact_p4_files_for_ai(raw);
    }
    if is_p4_move_block(raw) {
        return compact_p4_move_for_ai(raw);
    }
    if is_p4_copy_block(raw) {
        return compact_p4_copy_for_ai(raw);
    }
    if is_p4_integrate_block(raw) {
        return compact_p4_integrate_for_ai(raw);
    }
    if is_p4_sync_block(raw) {
        return compact_p4_sync_for_ai(raw);
    }
    if is_p4_resolve_block(raw) {
        return compact_p4_resolve_for_ai(raw);
    }
    if is_p4_revert_block(raw) {
        return compact_p4_revert_for_ai(raw);
    }
    if is_p4_edit_block(raw) {
        return compact_p4_edit_for_ai(raw);
    }
    if is_p4_add_block(raw) {
        return compact_p4_add_for_ai(raw);
    }
    if is_p4_delete_block(raw) {
        return compact_p4_delete_for_ai(raw);
    }
    // branch / label 实体输出近乎最小元数据，走专用压缩：日期符号化 + View 路径字典化
    if is_p4_branch_block(raw) || is_p4_label_block(raw) {
        return compact_p4_branch_label_for_ai(raw);
    }
    // tag 确认行冗长（"Tag <name> marked on <N> files."），走专用符号化压缩
    if is_p4_tag_block(raw) {
        return compact_p4_tag_for_ai(raw);
    }
    compact_p4_generic(raw)
}
/// p4 log 家族压缩入口：按块类型分派。
pub fn compact_p4_log_family_for_ai(raw: &str) -> String {
    if is_p4_labels_block(raw) {
        return compact_p4_labels_for_ai(raw);
    }
    if is_p4_shelve_block(raw) {
        return compact_p4_shelve_for_ai(raw);
    }
    if is_p4_unshelve_block(raw) {
        return compact_p4_unshelve_for_ai(raw);
    }
    if is_p4_filelog_block(raw) {
        return compact_p4_filelog_for_ai(raw);
    }
    // 法则：p4 submit 输出以 "Change <n> submitted." 结尾（变更号是本次提交的关键产物），
    // 需走提交专用压缩以保留变更号。仅当首行命令确实为 "submit" 时路由——changes 输出
    // 虽可能含 "*submitted*" 状态串，但首行命令是 changes，不可误判为 submit。
    if p4_subcommand_is(raw, &["submit"]) {
        return compact_p4_submit_for_ai(raw);
    }
    compact_p4_changes_for_ai(raw)
}
/// p4 status 压缩入口。
pub fn compact_p4_status_for_ai(raw: &str) -> String {
    if is_p4_opened_block(raw) {
        return compact_p4_opened_for_ai(raw);
    }
    if is_p4_sync_block(raw) {
        return compact_p4_sync_for_ai(raw);
    }
    if is_p4_move_block(raw) {
        return compact_p4_move_for_ai(raw);
    }
    if is_p4_copy_block(raw) {
        return compact_p4_copy_for_ai(raw);
    }
    if is_p4_integrate_block(raw) {
        return compact_p4_integrate_for_ai(raw);
    }
    if is_p4_edit_block(raw) {
        return compact_p4_edit_for_ai(raw);
    }
    if is_p4_add_block(raw) {
        return compact_p4_add_for_ai(raw);
    }
    if is_p4_delete_block(raw) {
        return compact_p4_delete_for_ai(raw);
    }
    compact_p4_generic(raw)
}
/// p4 通用压缩：保留锚点，过滤噪音，警报映射。
fn compact_p4_generic(raw: &str) -> String {
    let mut out = Vec::new();
    let mut first = true;
    for line in raw.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("p4 ") && first {
            out.push(t.to_string());
            first = false;
            continue;
        }
        if is_p4_narrative_noise(t) {
            continue;
        }
        if let Some(diff) = compact_p4_diff_head(t) {
            out.push(diff);
            continue;
        }
        // 删除 diff 中冗余的 ---/+++ 路径行（DIFF: 已标识文件）
        if t.starts_with("--- ") || t.starts_with("+++ ") {
            continue;
        }
        out.push(t.strip_prefix("... ").unwrap_or(t).to_string());
    }
    out.join("\n")
}

/// 判断是否为 p4 branch 实体命令（区别于复数 branches 列表）。
pub fn is_p4_branch_block(text: &str) -> bool {
    p4_subcommand_is(text, &["branch"])
}
/// 判断是否为 p4 label 实体命令（区别于复数 labels 列表）。
pub fn is_p4_label_block(text: &str) -> bool {
    p4_subcommand_is(text, &["label"])
}

// ============================================================================
// is_p4_* 检测
// ============================================================================
/// 判断是否为 p4 opened 块。
pub fn is_p4_opened_block(text: &str) -> bool {
    p4_subcommand_is(text, &["opened"])
        || text.lines().any(|line| {
            let t = line.trim().strip_prefix("... ").unwrap_or(line.trim());
            t.starts_with("//")
                && t.contains('#')
                && (t.contains(" edit ") || t.contains(" add ") || t.contains(" delete "))
        })
}
/// 判断是否为 p4 describe 块。
pub fn is_p4_describe_block(text: &str) -> bool {
    p4_subcommand_is(text, &["describe"])
        || text
            .lines()
            .any(|line| line.trim().starts_with("Change ") && line.contains("by"))
}
/// 判断是否为 p4 changes 块。
pub fn is_p4_changes_block(text: &str) -> bool {
    p4_subcommand_is(text, &["changes"])
        || text.lines().any(|line| {
            let t = line.trim();
            t.starts_with("Change ") && t.contains(" on ") && t.contains(" by ")
        })
}
/// 判断是否为 p4 fstat 块。
pub fn is_p4_fstat_block(text: &str) -> bool {
    p4_subcommand_is(text, &["fstat"])
        || text
            .lines()
            .any(|line| line.trim().starts_with("... depotFile"))
}
/// 判断是否为 p4 where 块。
pub fn is_p4_where_block(text: &str) -> bool {
    p4_subcommand_is(text, &["where"])
}
/// 判断是否为 p4 info 块。
pub fn is_p4_info_block(text: &str) -> bool {
    p4_subcommand_is(text, &["info"])
        || ["User name:", "Client name:", "Server address:"]
            .iter()
            .any(|p| text.lines().any(|l| l.trim_start().starts_with(p)))
}
/// 判断是否为 p4 labels 块。
pub fn is_p4_labels_block(text: &str) -> bool {
    p4_subcommand_is(text, &["labels"])
        || text
            .lines()
            .any(|line| line.trim().starts_with("Label ") && line.contains("by"))
}
/// 判断是否为 p4 dirs 块。
pub fn is_p4_dirs_block(text: &str) -> bool {
    p4_subcommand_is(text, &["dirs"])
}
/// 判断是否为 p4 sync 块。
pub fn is_p4_sync_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    p4_subcommand_is(text, &["sync"])
        || lower.contains("sync completed")
        || text.lines().any(|line| {
            let t = line.trim();
            t.ends_with(" - updated") || t.ends_with(" - added") || t.ends_with(" - deleted")
        })
}
/// 判断是否为 p4 submit 块。
pub fn is_p4_submit_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    p4_subcommand_is(text, &["submit"])
        || (lower.contains("change ") && lower.contains("submitted"))
}
/// 判断是否为 p4 shelve 块。
pub fn is_p4_shelve_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    p4_subcommand_is(text, &["shelve"]) || lower.contains("shelve change")
}
/// 判断是否为 p4 unshelve 块。
/// 注意：`shelve change`/`shelve change` 判定会误伤 unshelve，故单独列出，
/// 避免 unshelve 被当作 shelve 处理而丢失变更号。
pub fn is_p4_unshelve_block(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    p4_subcommand_is(text, &["unshelve"])
        || lower.contains("unshelved change")
        || lower.contains("unshelve of change")
}
/// 判断是否为 p4 filelog 块。
pub fn is_p4_filelog_block(text: &str) -> bool {
    p4_subcommand_is(text, &["filelog"])
        || text.lines().any(|line| {
            let t = line.trim();
            t.starts_with("... #") && t.contains(" change ")
        })
}
/// 判断是否为 p4 resolve 块。
pub fn is_p4_resolve_block(text: &str) -> bool {
    p4_subcommand_is(text, &["resolve"])
        || text.lines().any(|line| {
            let t = line.trim();
            (t.contains(" - resolved using ") || t.contains(" - skipped "))
                && t.split_whitespace()
                    .next()
                    .is_some_and(|tok| tok.starts_with("//"))
        })
}
/// 判断是否为 p4 revert 块。
pub fn is_p4_revert_block(text: &str) -> bool {
    p4_subcommand_is(text, &["revert"])
        || text
            .lines()
            .any(|line| line.trim().ends_with(" - reverted"))
}
/// 判断是否为 p4 edit 块。
pub fn is_p4_edit_block(text: &str) -> bool {
    p4_subcommand_is(text, &["edit"])
        || text
            .lines()
            .any(|line| line.trim().ends_with(" - opened for edit"))
}
/// 判断是否为 p4 add 块。
pub fn is_p4_add_block(text: &str) -> bool {
    p4_subcommand_is(text, &["add"])
        || text
            .lines()
            .any(|line| line.trim().ends_with(" - added for add"))
}
/// 判断是否为 p4 delete 块。
pub fn is_p4_delete_block(text: &str) -> bool {
    p4_subcommand_is(text, &["delete"])
        || text
            .lines()
            .any(|line| line.trim().ends_with(" - deleted for delete"))
}
/// 判断是否为 p4 files 块。
pub fn is_p4_files_block(text: &str) -> bool {
    p4_subcommand_is(text, &["files"])
        || text.lines().any(|line| {
            let t = line.trim();
            t.starts_with("//") && t.contains('#') && t.contains(" - ")
        })
}
/// 判断是否为 p4 move 块。
pub fn is_p4_move_block(text: &str) -> bool {
    p4_subcommand_is(text, &["move"])
        || text.lines().any(|line| {
            let t = line.trim().to_ascii_lowercase();
            t.contains(" - moved from ")
        })
}
/// 判断是否为 p4 copy 块。
pub fn is_p4_copy_block(text: &str) -> bool {
    p4_subcommand_is(text, &["copy"])
        || text.lines().any(|line| {
            let t = line.trim();
            // 仅当 " -> " 两侧均为 depot 路径（// 开头）时才识别为 copy
            t.starts_with("//") && t.contains(" -> ")
        })
}
/// 判断是否为 p4 integrate 块。
pub fn is_p4_integrate_block(text: &str) -> bool {
    p4_subcommand_is(text, &["integrate"])
}
/// 判断是否为 p4 users 块。
pub fn is_p4_users_block(text: &str) -> bool {
    p4_subcommand_is(text, &["users"])
}
/// 判断是否为 p4 workspaces 块。
pub fn is_p4_workspaces_block(text: &str) -> bool {
    p4_subcommand_is(text, &["workspaces"])
}
/// 判断是否为 p4 depot 块。
pub fn is_p4_depot_block(text: &str) -> bool {
    p4_subcommand_is(text, &["depot"])
}
/// 判断是否为 p4 tag 块。
pub fn is_p4_tag_block(text: &str) -> bool {
    p4_subcommand_is(text, &["tag"])
        || text.lines().any(|l| {
            let t = l.trim();
            t.starts_with("Tag ") && t.contains(" marked on ")
        })
}
/// 检测 Context Diff 格式（p4 diff -dc 族）：包含 *************** 分隔符
pub fn is_p4_context_diff_block(text: &str) -> bool {
    text.to_ascii_lowercase().contains(" -dc")
        || text.lines().any(|line| {
            let t = line.trim();
            t.chars().all(|c| c == '*') && t.len() >= 5
        })
}

/// 使用统一 argv 解析识别 p4 子命令，避免扫描正文词汇误判。
fn p4_subcommand_is(raw: &str, expected: &[&str]) -> bool {
    let first = raw
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim())
        .unwrap_or("");
    let Some((tool, words)) = parse_vcs_command_words_from_line(first) else {
        return false;
    };
    if tool != "p4" {
        return false;
    }
    let Some(sub) = words.first().map(String::as_str) else {
        return false;
    };
    expected.iter().any(|cmd| *cmd == sub)
}

/// 提取 p4 dirs 输出的公共根前缀并替换为 "root: " 行。
fn maybe_factor_p4_dirs_root(text: String) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 2 {
        return text;
    }
    let idxs: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            if l.trim().starts_with("//") && !l.trim().contains(' ') {
                Some(i)
            } else {
                None
            }
        })
        .collect();
    if idxs.len() < 2 {
        return text;
    }
    let first = lines[*idxs.first().unwrap()].trim();
    let mut common = first.to_string();
    for i in idxs.iter().skip(1) {
        common = common_prefix_str(&common, lines[*i].trim());
        if common.is_empty() {
            return text;
        }
    }
    let Some(s) = common.rfind('/') else {
        return text;
    };
    let root = &common[..=s];
    if root.len() <= 2 {
        return text;
    }
    let mut r = String::new();
    r.push_str("root: ");
    r.push_str(root);
    r.push('\n');
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if idxs.contains(&i) && t.starts_with(root) {
            let sfx = t[root.len()..].trim_start_matches('/');
            r.push_str(if sfx.is_empty() { ".\n" } else { sfx });
            r.push('\n');
        } else {
            r.push_str(t);
            r.push('\n');
        }
    }
    if r.len() < text.len() {
        r
    } else {
        text
    }
}
/// 计算两个字符串的公共前缀。
fn common_prefix_str(a: &str, b: &str) -> String {
    let mut o = String::new();
    for (ca, cb) in a.chars().zip(b.chars()) {
        if ca != cb {
            break;
        }
        o.push(ca);
    }
    o
}
