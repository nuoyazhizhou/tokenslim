//! cli run 子命令

use crate::cli::app::{is_tokenslim_builtin_command, render_run_command_hint};
use crate::cli::common::*;
use crate::cli::get_plugins;
use crate::cli::types::*;
use crate::core::compression::{CompressionMetadata, CompressionOutput, Token};
use crate::core::compression_context::CompressionContext;
use crate::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
use crate::core::dedup_engine::{DedupConfig, DedupEngine};
use crate::core::dictionary_engine::DictionaryEngine;
use crate::core::metrics::{MetricsCollector, MetricsConfig};
use crate::core::path_optimizer::methods::{
    optimize_path_dictionary_blocks_with_options, PathDictionaryOptions,
};
use crate::core::path_optimizer::token_boundary::{
    is_path_token_boundary_next, replace_path_token_boundary,
};
use crate::core::plugin_config_loader::{self, RunRouteCapability};
use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::{Slice, SliceFlags, SliceType};
use crate::utils::i18n::{render_user_facing_terminal_message, t, t1, t2, UserFacingMessage};
use bumpalo::Bump;
use serde::Serialize;
use serde_json::json;
use std::borrow::Cow;
use std::io::{self, IsTerminal, Read};


pub(crate) fn parse_run_target<'a>(
    program: &str,
    run_command: &'a [String],
) -> Result<(&'a str, &'a [String]), CliError> {
    if run_command.is_empty() {
        let hint = render_run_command_hint(program);
        return Err(CliError::InvalidArgs(format!(
            "{}\n\n{}",
            format_invalid_args_message(
                "E_CLI_RUN_EMPTY",
                "未提供要执行的外部命令。",
                "No external command was provided for run mode.",
                Some(format!("{program} run git status 或 {program} git status")),
                Some(format!("{program} run git status or {program} git status")),
            ),
            hint
        )));
    }
    let prog = run_command[0].as_str();
    if prog.starts_with('-') {
        let normalized = prog.trim_start_matches('-');
        let (hint_zh, hint_en) = if normalized.eq_ignore_ascii_case("gain") {
            (
                Some(format!(
                    "检测到内置命令: `{program} gain`。若要执行外部命令，请使用 `{program} run <command>`。"
                )),
                Some(format!(
                    "Detected built-in command: `{program} gain`. For external commands, use `{program} run <command>`."
                )),
            )
        } else if is_tokenslim_builtin_command(normalized) {
            (
                Some(format!(
                    "检测到内置命令: `{program} {normalized}`。若要执行外部命令，请使用 `{program} run <command>`。"
                )),
                Some(format!(
                    "Detected built-in command: `{program} {normalized}`. For external commands, use `{program} run <command>`."
                )),
            )
        } else {
            (
                Some(format!(
                    "请改为: {program} run git status 或 {program} git status"
                )),
                Some(format!(
                    "Try: {program} run git status or {program} git status"
                )),
            )
        };
        let hint = render_run_command_hint(program);
        return Err(CliError::InvalidArgs(format!(
            "{}\n\n{}",
            format_invalid_args_message(
                "E_CLI_RUN_INVALID_TARGET",
                format!("`{prog}` 不是可执行命令。"),
                format!("`{prog}` is not a valid executable command."),
                hint_zh,
                hint_en,
            ),
            hint
        )));
    }
    Ok((prog, &run_command[1..]))
}


/// 判断 `prog` 是否是 `git` 可执行 (兼容绝对/相对路径与 Windows 扩展名)。
///
/// 例如全部返回 `true`:
/// - `"git"`
/// - `"/usr/bin/git"`
/// - `"C:\\Program Files\\Git\\bin\\git.exe"`
pub(crate) fn is_git_program(prog: &str) -> bool {
    let lower = prog.to_ascii_lowercase();
    if lower == "git" {
        return true;
    }
    // 取 basename (兼容 / 与 \)
    let basename = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
    // 去掉 Windows 可执行扩展名 (.exe / .cmd / .bat)
    let stem = basename
        .strip_suffix(".exe")
        .or_else(|| basename.strip_suffix(".cmd"))
        .or_else(|| basename.strip_suffix(".bat"))
        .unwrap_or(basename);
    stem == "git"
}


/// 检测 `git <subcmd> [args...]` 是否需要交互式输入 (vim/merge-tool/hunk 选择器等)。
///
/// 命中后调用方应放弃 stdout 压缩, 直接透传 stdio 给原生命令, 否则子进程会卡死。
///
/// 黑名单规则 (与 `git --help` 行为对齐):
/// - `commit` 无 `-m` / `-F` / `--file` / `--message` → 打开 vim
/// - `rebase` 含 `-i` / `--interactive` → 打开 todo list 编辑器
/// - `tag` 含 `-a` / `--annotate` 且无 `-m` / `-F` → 打开 vim
/// - `add` 含 `-p` / `--patch` → hunk 选择器
/// - `checkout` / `restore` 含 `-p` / `--patch` → hunk 选择器
/// - `clean` 含 `-i` / `--interactive` → 文件选择器
///
/// **不**进黑名单 (无冲突/无 flag 时不进入交互):
/// - `merge` / `pull` / `cherry-pick` / `stash` — 无冲突时无 tty 需求
/// - `push` — 协议层 (HTTP/SSH agent) 处理认证
/// - `branch` / `log` / `diff` / `show` / `fetch` / `clone`
///
/// 注意: 这是启发式检测, 别名/外部 `git-foo` 工具可能漏判。漏判的最坏后果是
/// 用户再次卡住, 不会损坏数据。
pub(crate) fn detect_git_interactive(prog: &str, args: &[String]) -> bool {
    if !is_git_program(prog) {
        return false;
    }
    let sub = match args.first().map(String::as_str) {
        Some(s) => s,
        None => return false, // 裸 `git` 本身是 help, 不交互
    };

    // 通用 flag 命中检测: 完全相等 / 短/长 flag / 带 `=` 的形式
    let has = |flag: &str| -> bool {
        let eq_form = format!("{flag}=");
        args.iter().any(|a| a == flag || a.starts_with(&eq_form))
    };

    match sub {
        "commit" => {
            // `-m` / `-F` / `--file` / `--message` / `--no-edit` 都能跳过 vim
            !(has("-m")
                || has("-F")
                || has("--file")
                || has("--message")
                || has("--no-edit"))
        }
        "rebase" => has("-i") || has("--interactive"),
        "tag" => (has("-a") || has("--annotate")) && !(has("-m") || has("-F")),
        "add" => has("-p") || has("--patch"),
        "checkout" | "restore" | "rm" => has("-p") || has("--patch"),
        "clean" => has("-i") || has("--interactive"),
        _ => false,
    }
}


/// 透传 stdio 跑外部命令 (无压缩, 无 tty 转发)。
///
/// 用于 `git` 交互式子命令的 fallback: 不接管 stdout/stderr/stdin, 让子进程
/// 看到真实的 tty, vim/merge-tool 等能正常工作。退出码透传给调用方。
pub(crate) fn run_external_command_passthrough(
    prog: &str,
    cmd_args: &[String],
) -> Result<std::process::ExitStatus, CliError> {
    use std::process::Stdio;
    let mut child = std::process::Command::new(prog);
    child
        .args(cmd_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let status = child.spawn().map_err(CliError::Io)?.wait().map_err(CliError::Io)?;
    Ok(status)
}


pub(crate) fn run_external_command_capture(
    prog: &str,
    cmd_args: &[String],
) -> Result<(std::process::ExitStatus, String), CliError> {
    let mut child = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C");
        c.arg(prog);
        c.args(cmd_args);
        c
    } else {
        let mut c = std::process::Command::new(prog);
        c.args(cmd_args);
        c
    };

    let mut child = child
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(CliError::Io)?;

    let mut stdout_bytes = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_end(&mut stdout_bytes);
    }

    let mut stderr_bytes = Vec::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_end(&mut stderr_bytes);
    }

    let status = child.wait().map_err(CliError::Io)?;
    let (out_str, _out_enc, _out_fix_steps) =
        crate::core::encoding_fallback::decode_and_repair_for_display(&stdout_bytes);
    let (err_str, _err_enc, _err_fix_steps) =
        crate::core::encoding_fallback::decode_and_repair_for_display(&stderr_bytes);

    let combined = if err_str.is_empty() {
        out_str.to_string()
    } else if out_str.is_empty() {
        err_str.to_string()
    } else {
        format!("{}\n{}", out_str, err_str)
    };

    Ok((status, combined))
}


pub(crate) fn should_quote_run_anchor_token(token: &str) -> bool {
    token.is_empty()
        || token.chars().any(|ch| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '"' | '\''
                        | '`'
                        | '$'
                        | '&'
                        | '|'
                        | ';'
                        | '<'
                        | '>'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '*'
                        | '!'
                        | '?'
                        | '#'
                )
        })
}


pub(crate) fn quote_run_anchor_token(token: &str) -> String {
    if !should_quote_run_anchor_token(token) {
        return token.to_string();
    }
    let escaped = token.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}


pub(crate) fn tokenize_command_line(line: &str) -> Option<Vec<String>> {
    #[derive(Clone, Copy)]
    enum QuoteMode {
        None,
        Single,
        Double,
    }

    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut mode = QuoteMode::None;
    let mut escaped = false;

    for ch in line.chars() {
        match mode {
            QuoteMode::None => {
                if ch.is_whitespace() {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                } else if ch == '"' {
                    mode = QuoteMode::Double;
                } else if ch == '\'' {
                    mode = QuoteMode::Single;
                } else {
                    current.push(ch);
                }
            }
            QuoteMode::Single => {
                if ch == '\'' {
                    mode = QuoteMode::None;
                } else {
                    current.push(ch);
                }
            }
            QuoteMode::Double => {
                if escaped {
                    current.push(ch);
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    mode = QuoteMode::None;
                } else {
                    current.push(ch);
                }
            }
        }
    }

    match mode {
        QuoteMode::None => {}
        QuoteMode::Single | QuoteMode::Double => return None,
    }

    if escaped {
        current.push('\\');
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Some(tokens)
}


pub(crate) fn is_equivalent_run_anchor_line(line: &str, prog: &str, cmd_args: &[String]) -> bool {
    let Some(actual_tokens) = tokenize_command_line(line.trim_start()) else {
        return false;
    };
    if actual_tokens.is_empty() {
        return false;
    }

    let expected_len = cmd_args.len() + 1;
    if actual_tokens.len() != expected_len {
        return false;
    }

    if command_keyword(&actual_tokens[0]) != command_keyword(prog) {
        return false;
    }

    actual_tokens
        .iter()
        .skip(1)
        .zip(cmd_args.iter())
        .all(|(actual, expected)| actual == expected)
}


pub(crate) fn build_run_command_anchor(prog: &str, cmd_args: &[String]) -> String {
    let mut parts = Vec::with_capacity(cmd_args.len() + 1);
    parts.push(quote_run_anchor_token(prog));
    for arg in cmd_args {
        parts.push(quote_run_anchor_token(arg));
    }
    parts.join(" ")
}


pub(crate) fn is_explicit_vcs_command_line(line: &str) -> bool {
    let Some(tokens) = tokenize_command_line(line.trim_start()) else {
        return false;
    };
    if tokens.is_empty() {
        return false;
    }
    let prog = &tokens[0];
    let args = tokens.iter().skip(1).cloned().collect::<Vec<_>>();
    matches!(detect_run_plugin_route(prog, &args), RunPluginRoute::Vcs)
}


pub(crate) fn is_explicit_run_command_line(line: &str, prog: &str, cmd_args: &[String]) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return false;
    }

    if is_equivalent_run_anchor_line(trimmed, prog, cmd_args) {
        return true;
    }

    false
}


pub(crate) fn prepend_run_command_anchor_if_needed(combined: &str, prog: &str, cmd_args: &[String]) -> String {
    let first_non_empty = combined
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim_end_matches('\r');
    if is_explicit_run_command_line(first_non_empty, prog, cmd_args) {
        combined.to_string()
    } else {
        format!("{}\n{}", build_run_command_anchor(prog, cmd_args), combined)
    }
}


pub(crate) fn build_single_document_slice<'a>(input: &'a str, line_count: usize) -> Slice<'a> {
    Slice {
        id: 0,
        text: Cow::Borrowed(input),
        slice_type: SliceType::Paragraph,
        offset: 0,
        line_start: 1,
        line_end: line_count,
        file_metadata: None,
        flags: Default::default(),
    }
}


pub(crate) fn build_compression_metadata_from_tokens(
    input: &str,
    tokens: &[Token<'static>],
    processing_time_ms: u128,
    context: &CompressionContext,
) -> CompressionMetadata {
    let original_size = input.len();
    let compressed_size: usize = tokens.iter().map(|t| t.estimated_size()).sum();
    let original_tokens = original_size / 4;
    let compressed_tokens: usize = tokens.iter().map(|t| t.estimated_tokens()).sum();
    CompressionMetadata {
        original_size,
        compressed_size,
        original_tokens,
        compressed_tokens,
        token_savings: original_tokens.saturating_sub(compressed_tokens),
        compression_ratio: if original_size == 0 {
            1.0
        } else {
            compressed_size as f32 / original_size as f32
        },
        token_ratio: if original_tokens == 0 {
            1.0
        } else {
            compressed_tokens as f32 / original_tokens as f32
        },
        slice_count: 1,
        processing_time_ms,
        order_info: None,
        base_timestamp: context.base_timestamp().map(|ts| ts.to_rfc3339()),
    }
}


pub(crate) fn compress_vcs_run_as_single_document(input: &str) -> CompressionOutput {
    let start = std::time::Instant::now();
    let mut dict_engine = DictionaryEngine::new();
    let mut dedup_engine = DedupEngine::new(DedupConfig::default());
    let arena = Bump::new();
    let mut context = CompressionContext::new();
    let plugin = crate::plugins::vcs_plugin::VcsPlugin::new();
    let line_count = input.lines().count().max(1);

    let slice = build_single_document_slice(input, line_count);

    let result = plugin.compress_with_context(
        &slice,
        &mut dict_engine,
        &mut dedup_engine,
        &arena,
        &mut context,
    );

    let tokens: Vec<Token<'static>> = result.tokens.into_iter().map(|t| t.into_owned()).collect();
    let metadata = build_compression_metadata_from_tokens(
        input,
        &tokens,
        start.elapsed().as_millis(),
        &context,
    );

    CompressionOutput {
        tokens,
        dictionary: dict_engine.snapshot(),
        metadata,
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VcsRunIntent {
    Status,
    Log,
    Diff,
    Other,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunPluginRoute {
    Vcs,
    Node,
    Build,
    Generic,
}


pub(crate) fn command_keyword(prog: &str) -> String {
    let file = std::path::Path::new(prog.trim_matches('"'))
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(prog)
        .to_ascii_lowercase();

    for suffix in [".exe", ".cmd", ".bat", ".com", ".ps1"] {
        if let Some(stripped) = file.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }

    file
}


/// 加载运行路由配置（配置文件优先，缺失时内置默认）
pub(crate) fn load_run_routes() -> Vec<RunRouteCapability> {
    let config_dir = std::path::Path::new("config").join("plugins");
    plugin_config_loader::load_run_route_capabilities(if config_dir.exists() {
        Some(&config_dir)
    } else {
        None
    })
}


pub(crate) fn detect_run_plugin_route(prog: &str, cmd_args: &[String]) -> RunPluginRoute {
    let caps = load_run_routes();
    let route = plugin_config_loader::resolve_run_route(&caps, prog, cmd_args);
    match route.route_group.as_str() {
        "vcs" => RunPluginRoute::Vcs,
        "node" => RunPluginRoute::Node,
        "build" => RunPluginRoute::Build,
        _ => RunPluginRoute::Generic,
    }
}


pub(crate) fn remove_vcs_plugins(plugins: Vec<Box<dyn Plugin>>) -> Vec<Box<dyn Plugin>> {
    plugins
        .into_iter()
        .filter(|p| !matches!(p.name(), "vcs" | "git_diff"))
        .collect()
}


pub(crate) fn keep_generic_run_plugins(plugins: Vec<Box<dyn Plugin>>) -> Vec<Box<dyn Plugin>> {
    plugins
        .into_iter()
        .filter(|p| matches!(p.name(), "generic_text" | "ansi_cleaner" | "noise_filter"))
        .collect()
}


pub(crate) fn plugins_for_run_command(prog: &str, cmd_args: &[String]) -> Vec<Box<dyn Plugin>> {
    let route = detect_run_plugin_route(prog, cmd_args);
    let plugins = get_plugins();

    match route {
        RunPluginRoute::Vcs => plugins,
        RunPluginRoute::Node | RunPluginRoute::Build => remove_vcs_plugins(plugins),
        RunPluginRoute::Generic => keep_generic_run_plugins(plugins),
    }
}


pub(crate) fn explain_run_route(prog: &str, cmd_args: &[String], args: &CliArgs) -> String {
    let caps = load_run_routes();
    let route = plugin_config_loader::resolve_run_route(&caps, prog, cmd_args);
    let route_candidates =
        plugin_config_loader::explain_run_route_candidates(&caps, prog, cmd_args);
    let plugins = plugins_for_run_command(prog, cmd_args);
    let plugin_names = plugins
        .iter()
        .map(|plugin| plugin.name())
        .collect::<Vec<_>>()
        .join(", ");
    let vcs_intent = get_vcs_intent(prog, cmd_args);
    let vcs_ai_compact =
        should_enable_vcs_ai_compact(vcs_intent, args.output_format.clone(), args.preset);

    let mut out = String::new();
    out.push_str("run_route\n");
    out.push_str(&format!(
        "command={}\n",
        build_run_command_anchor(prog, cmd_args)
    ));
    out.push_str(&format!("normalized_tool={}\n", route.command_keyword));
    out.push_str(&format!("route_plugin={}\n", route.plugin_name));
    out.push_str(&format!("route_group={}\n", route.route_group));
    out.push_str(&format!(
        "intent={}\n",
        route.intent.as_deref().unwrap_or("none")
    ));
    out.push_str(&format!("fallback={}\n", route.is_fallback));
    out.push_str(&format!("matched_by={}\n", route.matched_by));
    out.push_str(&format!(
        "matched_pattern={}\n",
        route.matched_pattern.as_deref().unwrap_or("none")
    ));
    out.push_str(&format!(
        "route_priority={}\n",
        route
            .priority
            .map(|p| p.to_string())
            .unwrap_or_else(|| "none".to_string())
    ));
    out.push_str(&format!("route_candidates={}\n", route_candidates.len()));
    for (idx, candidate) in route_candidates.iter().enumerate() {
        out.push_str(&format!(
            "route_candidate_{}={}|group={}|priority={}|matched_by={}|matched_pattern={}|intent={}|fallback={}\n",
            idx + 1,
            candidate.plugin_name,
            candidate.route_group,
            candidate
                .priority
                .map(|p| p.to_string())
                .unwrap_or_else(|| "none".to_string()),
            candidate.matched_by,
            candidate.matched_pattern.as_deref().unwrap_or("none"),
            candidate.intent.as_deref().unwrap_or("none"),
            candidate.is_fallback
        ));
    }
    out.push_str(&format!(
        "output_format={}\n",
        match args.output_format {
            OutputFormat::Json => "json",
            OutputFormat::Markdown => "markdown",
            OutputFormat::Text => "text",
        }
    ));
    out.push_str(&format!(
        "preset={}\n",
        match args.preset {
            Some(Preset::Fast) => "fast",
            Some(Preset::Balanced) => "balanced",
            Some(Preset::Ai) => "ai",
            None => "none",
        }
    ));
    out.push_str(&format!("vcs_ai_compact={}\n", vcs_ai_compact));
    out.push_str(&format!("plugin_chain={}\n", plugin_names));
    out
}


pub(crate) fn get_vcs_intent(prog: &str, args: &[String]) -> Option<VcsRunIntent> {
    let caps = load_run_routes();
    let route = plugin_config_loader::resolve_run_route(&caps, prog, args);
    if route.route_group.eq_ignore_ascii_case("vcs") && route.intent.is_none() {
        return Some(VcsRunIntent::Other);
    }
    match route.intent {
        Some(intent) if intent.eq_ignore_ascii_case("status") => Some(VcsRunIntent::Status),
        Some(intent) if intent.eq_ignore_ascii_case("log") => Some(VcsRunIntent::Log),
        Some(intent) if intent.eq_ignore_ascii_case("diff") => Some(VcsRunIntent::Diff),
        Some(_) => Some(VcsRunIntent::Other),
        None => None,
    }
}


/// 配置驱动的 VCS 意图检测（替代原硬编码 detect_vcs_run_intent）
pub(crate) fn detect_vcs_run_intent(prog: &str, cmd_args: &[String]) -> Option<VcsRunIntent> {
    get_vcs_intent(prog, cmd_args)
}


pub(crate) fn count_paths_footer_lines(text: &str) -> usize {
    text.lines()
        .filter(|line| line.starts_with("paths: ") || line.starts_with("[paths]"))
        .count()
}


pub(crate) fn parse_path_dictionary_blocks(text: &str) -> (Vec<(String, String)>, String) {
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut other_lines: Vec<&str> = Vec::new();

    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("[paths] ") || t.starts_with("paths: ") {
            let body = if let Some(rest) = t.strip_prefix("[paths] ") {
                rest
            } else if let Some(rest) = t.strip_prefix("paths: ") {
                rest
            } else {
                ""
            };
            for part in body.split(';') {
                let part = part.trim();
                if let Some(eq) = part.find('=') {
                    let token = part[..eq].trim().to_string();
                    let path = part[eq + 1..].trim().to_string();
                    if !token.is_empty() && !path.is_empty() && seen.insert(token.clone()) {
                        entries.push((token, path));
                    }
                }
            }
            continue;
        }
        other_lines.push(line);
    }

    (entries, other_lines.join("\n"))
}


pub(crate) fn sort_path_entries_by_token(entries: &mut [(String, String)]) {
    entries.sort_by(|a, b| {
        let na =
            a.0.strip_prefix("$P")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(usize::MAX);
        let nb =
            b.0.strip_prefix("$P")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(usize::MAX);
        na.cmp(&nb)
    });
}


pub(crate) fn render_path_dictionary_line(entries: &[(String, String)]) -> String {
    let parts: Vec<String> = entries
        .iter()
        .map(|(t, p)| format!("{}={}", t, p))
        .collect();
    format!("[paths] {}", parts.join("; "))
}


pub(crate) fn place_path_dictionary_line(body_text: &str, merged_dict: &str) -> String {
    if let Some(first) = body_text
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim_end_matches('\r'))
    {
        if is_explicit_vcs_command_line(first) {
            if let Some(pos) = body_text.find('\n') {
                let mut out = String::new();
                out.push_str(&body_text[..=pos]);
                out.push_str(merged_dict);
                out.push('\n');
                out.push_str(&body_text[(pos + 1)..]);
                return out;
            }
            return format!("{}\n{}", body_text, merged_dict);
        }
    }

    format!("{}\n{}", merged_dict, body_text)
}


pub(crate) fn rebuild_with_extended_subdir_entries(
    entries: &[(String, String)],
    body_text: &str,
) -> Option<String> {
    let mut extended = entries.to_vec();
    if !add_subdir_entries_from_text(&mut extended, body_text) {
        return None;
    }

    apply_parent_prefix_aliases_cli(&mut extended);
    let rewritten = replace_paths_with_dict(body_text, &extended);
    let merged_dict = render_path_dictionary_line(&extended);
    Some(format!("{}\n{}", merged_dict, rewritten))
}


pub(crate) fn normalize_path_dictionary_entries(entries: &mut Vec<(String, String)>) {
    // 排序：按 token 编号
    sort_path_entries_by_token(entries.as_mut_slice());
    // 为 3+ 子路径的公共前缀创建父级词典条目
    add_common_parent_entries(entries);
    // 父前缀别名（必须在 add_common_parent_entries 之后）
    apply_parent_prefix_aliases_cli(entries);
}


pub(crate) fn rewrite_body_with_path_dictionary(
    entries: &[(String, String)],
    body_text: &str,
) -> (String, Option<String>) {
    // 用合并后的字典替换所有原始路径为 $P 令牌
    let rewritten_body = replace_paths_with_dict(body_text, entries);
    let extended = rebuild_with_extended_subdir_entries(entries, &rewritten_body);
    (rewritten_body, extended)
}


/// 合并多个 [paths] / paths: 字典块为单一块（置顶），并全文本替换路径
pub(crate) fn merge_path_dictionary_blocks(text: &str) -> String {
    let (mut entries, body_text) = parse_path_dictionary_blocks(text);

    if entries.is_empty() {
        return text.to_string();
    }

    normalize_path_dictionary_entries(&mut entries);

    // 构建合并的字典行
    let merged_dict = render_path_dictionary_line(&entries);

    let (body_text, extended_result) = rewrite_body_with_path_dictionary(&entries, &body_text);

    // 跨块扫描：找出正文中多次出现的 $P_base/subdir 模式，补建专有条目
    if let Some(rebuilt) = extended_result {
        return rebuilt;
    }

    place_path_dictionary_line(&body_text, &merged_dict)
}


/// 用字典条目替换文本中的路径（先解析嵌套引用；同时支持令牌→令牌降维）
pub(crate) fn replace_paths_with_dict(text: &str, entries: &[(String, String)]) -> String {
    // 先解析嵌套引用：$P6=$P15/vcs_bzr → $P6=src/plugins/vcs_bzr
    let mut resolved: Vec<(String, String)> = entries.to_vec();
    for i in 0..resolved.len() {
        let val = &resolved[i].1;
        if !val.starts_with('$') {
            continue;
        }
        if let Some(slash) = val.find('/') {
            let token = &val[..slash];
            if let Some(target) = resolved
                .iter()
                .find(|(t, _)| t == token)
                .map(|(_, p)| p.clone())
            {
                if !target.starts_with('$') {
                    resolved[i].1 = format!("{}/{}", target, &val[slash + 1..]);
                }
            }
        } else {
            if let Some(target) = resolved
                .iter()
                .find(|(t, _)| t == val)
                .map(|(_, p)| p.clone())
            {
                if !target.starts_with('$') {
                    resolved[i].1 = target;
                }
            }
        }
    }

    // 按路径长度降序排列
    let mut sorted: Vec<&(String, String)> = resolved.iter().collect();
    sorted.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    let mut result = text.to_string();
    for (token, path) in sorted {
        // 替换原始绝对路径
        result = result.replace(path.as_str(), token.as_str());
    }

    // 第二遍：令牌→令牌降维（$P15/subdir/ → $P16/）
    // 对路径值含 $P 的条目（如 $P16=$P15/vcs_fossil_plugin），替换正文中的 $P15/vcs_fossil_plugin/ 为 $P16/
    for (token, path) in entries.iter().filter(|(_, p)| p.contains("$P")) {
        // path 是 $P15/vcs_fossil_plugin 格式
        // 在正文中查找 $P15/vcs_fossil_plugin/ 并替换为 $P16/
        if path.contains('/') {
            result = result.replace(path.as_str(), token.as_str());
        }
    }
    result
}


/// 扫描正文中 2+ 次出现的 $Pbase/subdir 模式，创建跨块专有条目
pub(crate) fn add_subdir_entries_from_text(entries: &mut Vec<(String, String)>, text: &str) -> bool {
    let parent_map: std::collections::HashMap<String, String> = entries
        .iter()
        .filter(|(_, p)| !p.starts_with('$'))
        .map(|(t, p)| (t.clone(), p.clone()))
        .collect();

    let re = regex::Regex::new(r"\$P\d+/([^/\s]+)/").unwrap();
    let mut subdir_counts: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();
    for cap in re.captures_iter(text) {
        let full = cap.get(0).unwrap().as_str();
        let subdir = cap.get(1).unwrap().as_str();
        if let Some(slash) = full.find('/') {
            let token = &full[..slash];
            *subdir_counts
                .entry((token.to_string(), subdir.to_string()))
                .or_insert(0) += 1;
        }
    }

    let existing: std::collections::HashSet<String> =
        entries.iter().map(|(_, p)| p.clone()).collect();
    let mut added = false;
    for ((parent, subdir), count) in subdir_counts {
        if count >= 2 {
            let full_path = format!("{}/{}", parent_map.get(&parent).unwrap_or(&parent), subdir);
            if !existing.contains(&full_path) {
                let n = entries
                    .iter()
                    .filter_map(|(t, _)| t.strip_prefix("$P").and_then(|s| s.parse::<usize>().ok()))
                    .max()
                    .unwrap_or(0)
                    + 1;
                entries.push((format!("$P{}", n), full_path));
                added = true;
            }
        }
    }
    if added {
        entries.sort_by(|a, b| {
            let na =
                a.0.strip_prefix("$P")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(usize::MAX);
            let nb =
                b.0.strip_prefix("$P")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(usize::MAX);
            na.cmp(&nb)
        });
    }
    added
}


pub(crate) fn add_common_parent_entries(entries: &mut Vec<(String, String)>) {
    let existing_paths: std::collections::HashSet<String> = collect_existing_paths(entries);
    let prefix_counts = collect_parent_prefix_counts(entries);
    append_common_parent_entries(entries, &existing_paths, prefix_counts);

    entries.sort_by(|a, b| {
        let na =
            a.0.strip_prefix("$P")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(usize::MAX);
        let nb =
            b.0.strip_prefix("$P")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(usize::MAX);
        na.cmp(&nb)
    });
}


pub(crate) fn collect_existing_paths(entries: &[(String, String)]) -> std::collections::HashSet<String> {
    entries.iter().map(|(_, p)| p.clone()).collect()
}


pub(crate) fn collect_parent_prefix_counts(
    entries: &[(String, String)],
) -> std::collections::HashMap<String, usize> {
    let mut prefix_counts = std::collections::HashMap::new();
    for (_, path) in entries {
        if path.starts_with('$') {
            continue;
        }
        if let Some(last_slash) = path.rfind('/') {
            *prefix_counts
                .entry(path[..last_slash].to_string())
                .or_insert(0) += 1;
        }
    }
    prefix_counts
}


pub(crate) fn next_path_token_id(entries: &[(String, String)]) -> usize {
    entries
        .iter()
        .filter_map(|(t, _)| t.strip_prefix("$P").and_then(|s| s.parse::<usize>().ok()))
        .max()
        .unwrap_or(0)
        + 1
}


pub(crate) fn append_common_parent_entries(
    entries: &mut Vec<(String, String)>,
    existing_paths: &std::collections::HashSet<String>,
    prefix_counts: std::collections::HashMap<String, usize>,
) {
    for (prefix, count) in prefix_counts {
        if count >= 3 && !existing_paths.contains(&prefix) {
            let next = next_path_token_id(entries);
            entries.push((format!("$P{}", next), prefix));
        }
    }
}


/// 父前缀嵌套别名
pub(crate) fn apply_parent_prefix_aliases_cli(entries: &mut Vec<(String, String)>) {
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..entries.len() {
            let path_i = entries[i].1.clone();
            if path_i.starts_with('$') {
                continue;
            }
            for j in 0..entries.len() {
                if i == j {
                    continue;
                }
                let (ref token_j, ref path_j) = entries[j];
                if path_j.starts_with('$') {
                    continue;
                }
                if path_i.starts_with(path_j.as_str()) && path_i.len() > path_j.len() {
                    let suffix = &path_i[path_j.len()..]
                        .trim_start_matches('/')
                        .trim_start_matches('\\');
                    if !suffix.is_empty() {
                        entries[i].1 = format!("{}/{}", token_j, suffix);
                        changed = true;
                        break;
                    }
                }
            }
        }
    }
}


/// 消除 VCS 输出语义重复
pub(crate) fn strip_duplicate_vcs_headers(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut to_remove: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // 检测 BR:X → 移除前置 "On branch X"
    let mut br_name = String::new();
    for line in &lines {
        if let Some(b) = line.trim().strip_prefix("BR:") {
            br_name = b.to_string();
            break;
        }
    }
    if !br_name.is_empty() {
        for (i, line) in lines.iter().enumerate() {
            if line.trim() == format!("On branch {}", br_name) {
                to_remove.insert(i);
            }
        }
    }

    // 检测 [changes] → 移除原始 section header
    if lines.iter().any(|l| l.trim() == "[changes]") {
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if t == "Changes not staged for commit:" || t == "Changes to be committed:" {
                to_remove.insert(i);
            }
        }
    }

    // 检测 [untracked] → 移除原始 section header
    if lines.iter().any(|l| l.trim() == "[untracked]") {
        for (i, line) in lines.iter().enumerate() {
            if line.trim() == "Untracked files:" {
                to_remove.insert(i);
            }
        }
    }

    // 检测 CH: 行 → 移除前置 "commit <hash>" 原始行
    if lines.iter().any(|l| l.trim().starts_with("CH:")) {
        for (i, line) in lines.iter().enumerate() {
            if line.trim().starts_with("commit ") && line.trim().len() > 7 {
                to_remove.insert(i);
            }
        }
    }

    if to_remove.is_empty() {
        return text.to_string();
    }

    let kept: Vec<String> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !to_remove.contains(i))
        .map(|(_, s)| s.to_string())
        .collect();
    kept.join("\n").trim_end_matches('\n').to_string()
}


pub(crate) fn token_key_as_num(token: &str) -> usize {
    token
        .strip_prefix("$P")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(usize::MAX)
}


pub(crate) fn collect_sorted_output_path_entries(
    output: &crate::core::compression::CompressionOutput,
) -> Vec<(String, String)> {
    let mut all_entries: Vec<(String, String)> = output
        .dictionary
        .paths
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    all_entries.sort_by(|a, b| token_key_as_num(&a.0).cmp(&token_key_as_num(&b.0)));
    all_entries
}


pub(crate) fn partition_path_entries_by_min_uses(
    formatted: &str,
    all_entries: Vec<(String, String)>,
    min_footer_token_uses: usize,
) -> (Vec<(String, String)>, Vec<(String, String)>) {
    let mut keep_entries: Vec<(String, String)> = Vec::new();
    let mut drop_entries: Vec<(String, String)> = Vec::new();
    for (token, path) in all_entries {
        if count_path_token_uses(formatted, &token) >= min_footer_token_uses {
            keep_entries.push((token, path));
        } else {
            drop_entries.push((token, path));
        }
    }
    (keep_entries, drop_entries)
}


pub(crate) fn render_paths_footer_line(entries: &[(String, String)]) -> Option<String> {
    if entries.is_empty() {
        return None;
    }
    let parts: Vec<String> = entries
        .iter()
        .map(|(token, path)| format!("{}={}", token, path))
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(format!("paths: {}\n", parts.join("; ")))
}


pub(crate) fn should_skip_paths_footer_append(
    formatted: &str,
    output: &crate::core::compression::CompressionOutput,
) -> bool {
    count_paths_footer_lines(formatted) > 0
        || !formatted.contains("$P")
        || output.dictionary.paths.is_empty()
}


pub(crate) fn rewrite_dropped_path_tokens(formatted: &str, drop_entries: &[(String, String)]) -> String {
    let mut rewritten = formatted.to_string();
    for (token, path) in drop_entries {
        rewritten = replace_path_token_boundary(&rewritten, token, path);
    }
    rewritten
}


pub(crate) fn append_paths_footer_line(mut rewritten: String, footer: &str) -> String {
    if !rewritten.ends_with('\n') {
        rewritten.push('\n');
    }
    rewritten.push_str(footer);
    rewritten
}


pub(crate) fn append_paths_footer_from_output_dictionary(
    formatted: &str,
    output: &crate::core::compression::CompressionOutput,
    path_options: &PathDictionaryOptions,
) -> String {
    if should_skip_paths_footer_append(formatted, output) {
        return formatted.to_string();
    }

    let all_entries = collect_sorted_output_path_entries(output);
    let (keep_entries, drop_entries) = partition_path_entries_by_min_uses(
        formatted,
        all_entries,
        path_options.min_footer_token_uses,
    );

    let rewritten = rewrite_dropped_path_tokens(formatted, &drop_entries);

    if keep_entries.is_empty() {
        return rewritten;
    }

    let footer = match render_paths_footer_line(&keep_entries) {
        Some(line) => line,
        None => return rewritten,
    };

    append_paths_footer_line(rewritten, &footer)
}


pub(crate) fn count_path_token_uses(text: &str, token: &str) -> usize {
    if token.is_empty() {
        return 0;
    }

    let mut count = 0usize;
    let mut start = 0usize;
    while let Some(pos) = text[start..].find(token) {
        let idx = start + pos;
        let end = idx + token.len();
        let next = text.as_bytes().get(end).copied();
        if is_path_token_boundary_next(next) {
            count += 1;
        }
        start = end;
    }
    count
}


pub(crate) fn should_enable_vcs_ai_compact(
    vcs_intent: Option<VcsRunIntent>,
    output_format: OutputFormat,
    preset: Option<Preset>,
) -> bool {
    vcs_intent.is_some()
        && preset.is_some()
        && matches!(output_format, OutputFormat::Text | OutputFormat::Markdown)
}


pub(crate) fn should_apply_final_paths_optimizer(
    vcs_intent: Option<VcsRunIntent>,
    output_format: OutputFormat,
    formatted: &str,
) -> bool {
    if !matches!(output_format, OutputFormat::Text | OutputFormat::Markdown) {
        return false;
    }

    let footer_count = count_paths_footer_lines(formatted);
    if footer_count >= 2 {
        return true;
    }

    if footer_count == 1 {
        // Single-footer re-optimization is only safe/needed for explicit log/diff style outputs.
        return matches!(vcs_intent, Some(VcsRunIntent::Log | VcsRunIntent::Diff))
            || vcs_intent.is_none();
    }

    false
}


pub(crate) struct RunModeCompressionContext {
    vcs_intent: Option<VcsRunIntent>,
    path_options: crate::core::path_optimizer::methods::PathDictionaryOptions,
    enable_vcs_ai_compact: bool,
    profile: crate::plugins::vcs_plugin::methods::VcsAiProfile,
    run_input: String,
}


pub(crate) fn resolve_run_path_preset(
    preset: Option<Preset>,
) -> crate::core::path_optimizer::methods::PathDictionaryPreset {
    match preset {
        Some(Preset::Fast) => {
            crate::core::path_optimizer::methods::PathDictionaryPreset::Conservative
        }
        Some(Preset::Balanced) => {
            crate::core::path_optimizer::methods::PathDictionaryPreset::Balanced
        }
        Some(Preset::Ai) => crate::core::path_optimizer::methods::PathDictionaryPreset::Aggressive,
        None => crate::core::path_optimizer::methods::PathDictionaryPreset::Balanced,
    }
}


pub(crate) fn resolve_vcs_ai_profile(
    vcs_intent: Option<VcsRunIntent>,
) -> crate::plugins::vcs_plugin::methods::VcsAiProfile {
    match vcs_intent {
        Some(VcsRunIntent::Status) => crate::plugins::vcs_plugin::methods::VcsAiProfile::Status,
        Some(VcsRunIntent::Log) => crate::plugins::vcs_plugin::methods::VcsAiProfile::Log,
        Some(VcsRunIntent::Diff) => crate::plugins::vcs_plugin::methods::VcsAiProfile::Diff,
        Some(VcsRunIntent::Other) => crate::plugins::vcs_plugin::methods::VcsAiProfile::Other,
        None => crate::plugins::vcs_plugin::methods::VcsAiProfile::None,
    }
}


pub(crate) fn build_run_mode_compression_context(
    args: &CliArgs,
    prog: &str,
    cmd_args: &[String],
    combined: &str,
) -> RunModeCompressionContext {
    let vcs_intent = detect_vcs_run_intent(prog, cmd_args);
    let path_preset = resolve_run_path_preset(args.preset);
    let path_options =
        crate::core::path_optimizer::methods::resolve_path_dictionary_options_from_files(
            path_preset,
            args.config.as_deref(),
        );
    let enable_vcs_ai_compact =
        should_enable_vcs_ai_compact(vcs_intent, args.output_format.clone(), args.preset);
    let profile = resolve_vcs_ai_profile(vcs_intent);
    let run_input = prepend_run_command_anchor_if_needed(combined, prog, cmd_args);

    RunModeCompressionContext {
        vcs_intent,
        path_options,
        enable_vcs_ai_compact,
        profile,
        run_input,
    }
}


pub(crate) fn compress_run_mode_text(
    pipeline: &mut CompressionPipeline,
    context: &RunModeCompressionContext,
) -> Result<crate::core::compression::CompressionOutput, CliError> {
    let output_res = crate::core::path_optimizer::methods::run_with_path_dictionary_options(
        context.path_options.clone(),
        || {
            crate::plugins::vcs_plugin::methods::run_with_vcs_ai_context(
                context.enable_vcs_ai_compact,
                context.profile,
                || {
                    if context.vcs_intent.is_some() {
                        Ok(compress_vcs_run_as_single_document(&context.run_input))
                    } else {
                        pipeline.compress_str(&context.run_input)
                    }
                },
            )
        },
    );
    output_res.map_err(CliError::Pipeline)
}


pub(crate) fn build_run_command_string(prog: &str, cmd_args: &[String]) -> String {
    if cmd_args.is_empty() {
        prog.to_string()
    } else {
        format!("{} {}", prog, cmd_args.join(" "))
    }
}


pub(crate) fn resolve_run_filter_name(
    prog: &str,
    cmd_args: &[String],
    vcs_intent: Option<VcsRunIntent>,
) -> String {
    let variant_filter = std::env::current_dir().ok().and_then(|cwd| {
        crate::core::filter_variants::resolve_npm_test_variant(&cwd, prog, cmd_args)
    });
    if let Some(v) = variant_filter {
        return v.as_filter_name().to_string();
    }
    if vcs_intent.is_some() {
        return "vcs_plugin".to_string();
    }
    if let Some(first) = cmd_args.first() {
        return first.clone();
    }
    prog.to_string()
}


pub(crate) fn render_run_mode_output(
    args: &CliArgs,
    output: &crate::core::compression::CompressionOutput,
    vcs_intent: Option<VcsRunIntent>,
    path_options: &crate::core::path_optimizer::methods::PathDictionaryOptions,
) -> Result<String, CliError> {
    let mut formatted = format_run_mode_tokens(args.output_format.clone(), output)?;

    if matches!(
        args.output_format,
        OutputFormat::Text | OutputFormat::Markdown
    ) {
        formatted = append_paths_footer_from_output_dictionary(&formatted, output, path_options);
    }

    Ok(apply_run_mode_text_postprocessors(
        formatted,
        vcs_intent,
        args.output_format.clone(),
        path_options,
    ))
}


pub(crate) fn format_run_mode_tokens(
    output_format: OutputFormat,
    output: &crate::core::compression::CompressionOutput,
) -> Result<String, CliError> {
    match output_format {
        OutputFormat::Json => serde_json::to_string_pretty(output).map_err(CliError::Serialization),
        OutputFormat::Markdown | OutputFormat::Text => Ok(flatten_tokens(&output.tokens)),
    }
}


pub(crate) fn apply_run_mode_text_postprocessors(
    mut formatted: String,
    vcs_intent: Option<VcsRunIntent>,
    output_format: OutputFormat,
    path_options: &crate::core::path_optimizer::methods::PathDictionaryOptions,
) -> String {
    if vcs_intent.is_some() {
        formatted = merge_path_dictionary_blocks(&formatted);
        formatted = strip_duplicate_vcs_headers(&formatted);
    }
    if should_apply_final_paths_optimizer(vcs_intent, output_format, &formatted) {
        formatted = optimize_path_dictionary_blocks_with_options(&formatted, path_options);
    }
    formatted
}


pub(crate) fn run_run_mode(
    args: &CliArgs,
    pipeline: &mut CompressionPipeline,
    program: &str,
) -> Result<(), CliError> {
    let (prog, cmd_args) = parse_run_target(program, &args.run_command)?;

    if args.explain_route {
        args.emit_text(&explain_run_route(prog, cmd_args, args), None)?;
        return Ok(());
    }

    // 启发式检测: git 交互式子命令 (commit 无 -m / rebase -i / tag -a 无 -m /
    // add -p / checkout -p / clean -i) → 放弃压缩, 透传 stdio 给 git 原生命令。
    // 不透传会让 vim/merge-tool 等编辑器读不到 tty 而卡死。
    if detect_git_interactive(prog, cmd_args) {
        eprintln!(
            "[tokenslim] 检测到交互式 git 命令 (`{} {}`), fallback 到 git 原生命令 (该命令输出含用户决策输入, 压缩无意义)。",
            prog,
            cmd_args.join(" ")
        );
        eprintln!("[tokenslim] 提示: 如需查看压缩后的输出, 请改用 `git -c color.ui=always ... | tokenslim compress` 形式。");
        let status = run_external_command_passthrough(prog, cmd_args)?;
        std::process::exit(status.code().unwrap_or(1));
    }

    let (status, combined) = run_external_command_capture(prog, cmd_args)?;

    if combined.trim().is_empty() {
        if !status.success() {
            eprintln!("{}", t1("run_command_failed_exit", status));
            std::process::exit(status.code().unwrap_or(1));
        }
        return Ok(());
    }

    let context = build_run_mode_compression_context(args, prog, cmd_args, &combined);
    let output = compress_run_mode_text(pipeline, &context)?;

    let cmd_str = build_run_command_string(prog, cmd_args);
    let filter_name = resolve_run_filter_name(prog, cmd_args, context.vcs_intent);
    let exit_code = status.code().unwrap_or(1);
    record_tracking_event(&cmd_str, Some(filter_name.as_str()), &output, exit_code);
    let formatted =
        render_run_mode_output(args, &output, context.vcs_intent, &context.path_options)?;

    let (original_size, compressed_size) = tracking_bytes(&output);
    let stats = json!({
        "original_size": original_size,
        "compressed_size": compressed_size,
    });
    args.emit_text(&formatted, Some(stats))?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}


#[cfg(test)]
#[allow(clippy::needless_raw_string_hashes)]
mod tests {
    use super::*;

    // v0.3.7 的 is_git_program / detect_git_interactive heuristic 已在 v0.4.0
    // 删除 (改用 crate::cli::whitelist 双清单 + ConPTY 转发). 相关 12 个
    // unit test 一并删除, 新的双清单 / ConPTY / 3 路分发 unit test 放在
    // crate::cli::whitelist / crate::cli::conpty_probe / crate::cli::pty_runner
    // 各自模块的 #[cfg(test)] mod tests 段.
}
