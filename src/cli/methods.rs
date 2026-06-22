//! cli 方法实现

use super::types::*;
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
use std::borrow::Cow;
use std::io::{self, IsTerminal, Read};

#[cfg(test)]
use crate::core::path_optimizer::methods::optimize_path_dictionary_blocks;

fn find_cmd_index(args: &[String]) -> Option<usize> {
    let mut cmd_index = 1;
    while cmd_index < args.len() {
        let arg = &args[cmd_index];
        if arg.starts_with('-') {
            if arg == "--dry-run" || arg == "-v" || arg == "--verbose" {
                cmd_index += 1;
                continue;
            } else {
                return None;
            }
        } else {
            return Some(cmd_index);
        }
    }
    None
}

fn maybe_parse_run_subcommand_from_argv(args: &[String]) -> Option<Vec<String>> {
    let cmd_index = find_cmd_index(args)?;

    if args[cmd_index].eq_ignore_ascii_case("run") {
        return Some(args[cmd_index + 1..].to_vec());
    }

    None
}

fn is_tokenslim_builtin_command(cmd: &str) -> bool {
    matches!(
        cmd.to_ascii_lowercase().as_str(),
        "run"
            | "compress"
            | "decompress"
            | "init"
            | "workspace"
            | "encoding"
            | "rule"
            | "env"
            | "gain"
            | "explain-plugin"
            | "explain_plugin"
            | "plugins"
            | "repair-file"
            | "repair_file"
            | "doctor"
            | "hooks"
            | "hooks-status"
    )
}

fn maybe_parse_implicit_run_command_from_argv(args: &[String]) -> Option<Vec<String>> {
    let cmd_index = find_cmd_index(args)?;
    let cmd = &args[cmd_index];

    if is_tokenslim_builtin_command(cmd) {
        return None;
    }

    Some(args[cmd_index..].to_vec())
}

fn program_name_from_argv0(argv0: &str) -> String {
    std::path::Path::new(argv0)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("tokenslim")
        .to_string()
}

fn should_show_quick_usage(argv: &[String], stdin_is_terminal: bool) -> bool {
    if argv.len() == 2 && (argv[1] == "--help" || argv[1] == "-h") {
        return true;
    }
    argv.len() <= 1 && stdin_is_terminal
}

fn render_global_usage(program: &str) -> String {
    let capability_summary = String::new();
    let shorthand_text = t("cli_help_shorthand").replace("{program}", program);
    format!(
        "tokenslim {}

{}:
  {program} [--dry-run] [--verbose] <command> [args...]
  {program} {:<32} {}

{}:
  {program} {:<32} {}
  {program} {:<32} {}
  {program} {:<32} {}
  {program} {:<32} {}

{}:
  {program} {:<32} {}
  {program} {:<32} {}
  {program} {:<32} {}
  {program} {:<32} {}
  {program} {:<32} {}

{}:
  {program} {:<32} {}
  {program} {:<32} {}
  {program} {:<32} {}
  {program} {:<32} {}

{}:
  {program} git status
  {program} run cargo test
  {program} workspace --inject

{}",
        env!("CARGO_PKG_VERSION"),
        t("cli_help_usage"),
        "<external-command> [args...]",
        shorthand_text,
        t("cli_help_core_commands"),
        "run <command> [args...]",
        t("cli_desc_run"),
        "compress",
        t("cli_desc_compress"),
        "decompress",
        t("cli_desc_decompress"),
        "repair-file",
        t("cli_desc_repair_file"),
        t("cli_help_workspace_diag"),
        "init",
        t("cli_desc_init"),
        "workspace",
        t("cli_desc_workspace"),
        "encoding",
        t("cli_desc_encoding"),
        "rule",
        t("cli_desc_rule"),
        "env",
        t("cli_desc_env"),
        t("cli_help_utilities"),
        "gain",
        t("cli_desc_gain"),
        "plugins",
        t("cli_desc_plugins"),
        "explain-plugin",
        t("cli_desc_explain_plugin"),
        "hooks",
        t("cli_desc_hooks"),
        t("cli_help_common_examples"),
        capability_summary
    )
}

fn intercept_help_request(argv: &[String], program: &str) -> Option<String> {
    if !argv.iter().any(|a| a == "-h" || a == "--help") {
        return None;
    }

    // Skip argv[0] which is the executable name
    let first_arg = argv.iter().skip(1).find(|a| !a.starts_with("-"));

    let help_text = match first_arg.map(|s| s.as_str()) {
        Some("run") => render_run_usage(program),
        Some("compress") => render_compress_usage(program),
        Some("decompress") => render_decompress_usage(program),
        Some("repair-file") | Some("repair_file") => render_repair_file_usage(program),
        Some("workspace") => render_workspace_usage(program),
        Some("encoding") => render_encoding_usage(program),
        Some("rule") => render_rule_usage(program),
        Some("env") => render_env_usage(program),
        Some("init") => render_init_usage(program),
        Some("gain") => render_gain_usage(program),
        Some("hooks") => render_hooks_usage(program),
        Some("plugins") => render_plugins_usage(program),
        Some("explain-plugin") | Some("explain_plugin") => render_explain_plugin_usage(program),
        _ => render_global_usage(program),
    };

    Some(help_text)
}

fn render_compress_usage(program: &str) -> String {
    format!(
        "{} compress

{}

{}:
  {} compress [{}]

{}:
  {:<23} {}
  {:<23} {}
  {:<23} {}
  {:<23} {}
  {:<23} {}

{}:
  {} compress -i input.log -o output.json --format json",
        program,
        t("cli_desc_compress"),
        t("cli_help_usage"),
        program,
        t("cli_help_options_placeholder"),
        t("cli_help_options"),
        "-i, --input <PATH>",
        t("cli_opt_input"),
        "-o, --output <PATH>",
        t("cli_opt_output"),
        "--format <FORMAT>",
        t("cli_opt_format_compress"),
        "--preset <PRESET>",
        t("cli_opt_preset"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program
    )
}

fn render_decompress_usage(program: &str) -> String {
    format!(
        "{} decompress

{}

{}:
  {} decompress [{}]

{}:
  {:<23} {}
  {:<23} {}
  {:<23} {}
  {:<23} {}
  {:<23} {}

{}:
  {} decompress -i output.json --ai-signal",
        program,
        t("cli_desc_decompress"),
        t("cli_help_usage"),
        program,
        t("cli_help_options_placeholder"),
        t("cli_help_options"),
        "-i, --input <PATH>",
        t("cli_opt_input"),
        "-o, --output <PATH>",
        t("cli_opt_output"),
        "--ai-export",
        t("cli_opt_ai_export"),
        "--ai-signal",
        t("cli_opt_ai_signal"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program
    )
}

fn render_repair_file_usage(program: &str) -> String {
    format!(
        "{} repair-file

{}

{}:
  {} repair-file <PATH> [{}]

{}:
  {:<23} {}
  {:<23} {}
  {:<23} {}
  {:<23} {}
  {:<23} {}

{}:
  {} repair-file \"file.txt\" --inplace --backup",
        program,
        t("cli_desc_repair_file"),
        t("cli_help_usage"),
        program,
        t("cli_help_options_placeholder"),
        t("cli_help_options"),
        "--inplace",
        t("cli_opt_inplace"),
        "--backup",
        t("cli_opt_backup"),
        "--include <PATTERN>",
        t("cli_opt_include"),
        "--exclude <PATTERN>",
        t("cli_opt_exclude"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program
    )
}

fn render_run_usage(program: &str) -> String {
    format!(
        "{} run

{}

{}:
  {} run {}

{}:
  {:<23} {}

{}:
  {} run cargo test
  {} run git status",
        program,
        t("cli_desc_run"),
        t("cli_help_usage"),
        program,
        t("cli_run_external_command"),
        t("cli_help_options"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program,
        program
    )
}

fn render_hooks_usage(program: &str) -> String {
    format!(
        "{} hooks

{}

{}:
  {} hooks {}

{}:
  {:<23} {}
  {:<23} {}

{}:
  {} hooks install --shell powershell
  {} hooks status",
        program,
        t("cli_desc_hooks"),
        t("cli_help_usage"),
        program,
        t("cli_hooks_cmd"),
        t("cli_help_options"),
        "--shell <SHELL>",
        t("cli_opt_shell"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program,
        program
    )
}

fn render_workspace_usage(program: &str) -> String {
    format!(
        "{} workspace

{}

{}:
  {} workspace [{}]

{}:
  {:<23} {}
  {:<23} {}
  {:<23} {}

{}:
  {} workspace --inject
  {} workspace --format json",
        program,
        t("cli_desc_workspace"),
        t("cli_help_usage"),
        program,
        t("cli_help_options_placeholder"),
        t("cli_help_options"),
        "--inject",
        t("cli_opt_workspace_inject"),
        "--format <FORMAT>",
        t("cli_opt_format_diag"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program,
        program
    )
}

fn render_encoding_usage(program: &str) -> String {
    format!(
        "{} encoding

{}

{}:
  {} encoding [{}]

{}:
  {:<23} {}
  {:<23} {}
  {:<23} {}

{}:
  {} encoding --fix",
        program,
        t("cli_desc_encoding"),
        t("cli_help_usage"),
        program,
        t("cli_help_options_placeholder"),
        t("cli_help_options"),
        "--fix",
        t("cli_opt_encoding_fix"),
        "--format <FORMAT>",
        t("cli_opt_format_diag"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program
    )
}

fn render_rule_usage(program: &str) -> String {
    format!(
        "{} rule

{}

{}:
  {} rule [{}]

{}:
  {:<23} {}
  {:<23} {}

{}:
  {} rule --format json",
        program,
        t("cli_desc_rule"),
        t("cli_help_usage"),
        program,
        t("cli_help_options_placeholder"),
        t("cli_help_options"),
        "--format <FORMAT>",
        t("cli_opt_format_diag"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program
    )
}

fn render_env_usage(program: &str) -> String {
    format!(
        "{} env

{}

{}:
  {} env [{}]

{}:
  {:<23} {}
  {:<23} {}

{}:
  {} env --format json",
        program,
        t("cli_desc_env"),
        t("cli_help_usage"),
        program,
        t("cli_help_options_placeholder"),
        t("cli_help_options"),
        "--format <FORMAT>",
        t("cli_opt_format_diag"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program
    )
}

fn render_gain_usage(program: &str) -> String {
    format!(
        "{} gain

{}

{}:
  {} gain [{}]

{}:
  {:<23} {}
  {:<23} {}
  {:<23} {}
  {:<23} {}
  {:<23} {}

{}:
  {} gain --daily
  {} gain --by-filter --json",
        program,
        t("cli_desc_gain"),
        t("cli_help_usage"),
        program,
        t("cli_help_options_placeholder"),
        t("cli_help_options"),
        "--daily",
        t("cli_opt_gain_daily"),
        "--by-filter",
        t("cli_opt_gain_by_filter"),
        "--json",
        t("cli_opt_gain_json"),
        "--days <NUM>",
        t("cli_opt_gain_days"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program,
        program
    )
}

fn render_plugins_usage(program: &str) -> String {
    format!(
        "{}:\n  {} plugins\n\n{}:\n  {} plugins [{}...]\n",
        t("cli_help_usage"),
        program,
        t("cli_help_examples"),
        program,
        t("cli_help_options_placeholder")
    )
}

fn render_explain_plugin_usage(program: &str) -> String {
    format!(
        "{} explain-plugin

{}

{}:
  {} explain-plugin [{}]

{}:
  {:<23} {}
  {:<23} {}
  {:<23} {}
  {:<23} {}

{}:
  {} explain-plugin --input \"test.log\"",
        program,
        t("cli_desc_explain_plugin"),
        t("cli_help_usage"),
        program,
        t("cli_help_options_placeholder"),
        t("cli_help_options"),
        "-i, --input <PATH>",
        t("cli_opt_explain_input"),
        "--explain-command <CMD>",
        t("cli_opt_explain_cmd"),
        "--explain-replay-out <P>",
        t("cli_opt_explain_replay"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program
    )
}

fn render_init_usage(program: &str) -> String {
    format!(
        "{} init

{}

{}:
  {} init [{}]

{}:
  {:<23} {}
  {:<23} {}

{}:
  {} init
  {} init --force",
        program,
        t("cli_desc_init"),
        t("cli_help_usage"),
        program,
        t("cli_help_options_placeholder"),
        t("cli_help_options"),
        "--force",
        t("cli_opt_init_force"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program,
        program
    )
}

fn split_run_explain_route_flag(mut run_cmd: Vec<String>) -> (bool, Vec<String>) {
    if run_cmd.first().is_some_and(|arg| arg == "--explain-route") {
        run_cmd.remove(0);
        if run_cmd.first().is_some_and(|arg| arg == "--") {
            run_cmd.remove(0);
        }
        (true, run_cmd)
    } else {
        (false, run_cmd)
    }
}

fn render_run_command_hint(program: &str) -> String {
    format!(
        "运行模式需要外部命令。\n示例:\n  {program} run git status\n  {program} git status\n\nRun mode expects an external command.\nExamples:\n  {program} run git status\n  {program} git status"
    )
}

fn format_invalid_args_message(
    code: &'static str,
    zh: impl Into<String>,
    en: impl Into<String>,
    hint_zh: Option<String>,
    hint_en: Option<String>,
) -> String {
    render_user_facing_terminal_message(UserFacingMessage {
        code,
        message_zh: zh.into(),
        message_en: en.into(),
        hint_zh,
        hint_en,
    })
}

fn parse_run_target<'a>(
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

fn run_external_command_capture(
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

fn tracking_bytes(output: &CompressionOutput) -> (usize, usize) {
    let input_bytes = output.metadata.original_size;
    let output_bytes = if output.metadata.compressed_size > 0 {
        output.metadata.compressed_size
    } else {
        output.tokens.iter().map(|t| t.estimated_size()).sum()
    };
    (input_bytes, output_bytes)
}

fn record_tracking_event(
    command: &str,
    filter_name: Option<&str>,
    output: &CompressionOutput,
    exit_code: i32,
) {
    let (input_bytes, output_bytes) = tracking_bytes(output);
    let tracking_event = crate::core::tracking::TrackingEvent::new(
        command,
        filter_name,
        input_bytes,
        output_bytes,
        exit_code,
    );
    match crate::core::tracking::Tracker::open_default() {
        Ok(tracker) => {
            if let Err(e) = tracker.auto_cleanup() {
                log::warn!("{}", t1("tracking_cleanup_skipped", e));
            }
            if let Err(e) = tracker.record(&tracking_event) {
                log::warn!("{}", t1("tracking_record_failed", e));
            }
        }
        Err(e) => {
            log::warn!("{}", t1("tracking_open_failed", e));
        }
    }
}

fn argv_has_long_flag(args: &[String], long: &str) -> bool {
    let eq_prefix = format!("{}=", long);
    args.iter()
        .any(|arg| arg == long || arg.starts_with(&eq_prefix))
}

fn argv_has_format_flag(args: &[String]) -> bool {
    args.iter().any(|arg| {
        arg == "-f"
            || arg == "--format"
            || arg.starts_with("--format=")
            || (arg.starts_with("-f") && arg.len() > 2)
    })
}

fn argv_has_output_flag(args: &[String]) -> bool {
    args.iter().any(|arg| {
        arg == "-o"
            || arg == "--output"
            || arg.starts_with("--output=")
            || (arg.starts_with("-o") && arg.len() > 2)
    })
}

fn default_repair_output_path_from_input_arg(input_arg: &str) -> String {
    let path = std::path::Path::new(input_arg);
    let parent = path.parent().unwrap_or(std::path::Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("repaired");
    let file_name = if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        format!("{stem}.repaired.{ext}")
    } else {
        format!("{stem}.repaired.txt")
    };
    parent.join(file_name).to_string_lossy().to_string()
}

fn default_backup_output_path(input: &std::path::Path) -> std::path::PathBuf {
    let parent = input.parent().unwrap_or(std::path::Path::new("."));
    let file_name = input
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| format!("{s}.bak"))
        .unwrap_or_else(|| "backup.bak".to_string());
    parent.join(file_name)
}

fn normalize_match_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p = normalize_match_path(pattern).to_ascii_lowercase();
    let t = normalize_match_path(text).to_ascii_lowercase();
    let p = p.as_bytes();
    let t = t.as_bytes();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut match_t = 0usize;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            pi += 1;
            match_t = ti;
        } else if let Some(s) = star {
            pi = s + 1;
            match_t += 1;
            ti = match_t;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

fn should_repair_path(
    root: &std::path::Path,
    path: &std::path::Path,
    includes: &[String],
    excludes: &[String],
) -> bool {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let rel_s = normalize_match_path(&rel.to_string_lossy());
    let file_s = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(normalize_match_path)
        .unwrap_or_default();

    let include_ok = if includes.is_empty() {
        true
    } else {
        includes
            .iter()
            .any(|pat| wildcard_match(pat, &rel_s) || wildcard_match(pat, &file_s))
    };
    if !include_ok {
        return false;
    }
    !excludes
        .iter()
        .any(|pat| wildcard_match(pat, &rel_s) || wildcard_match(pat, &file_s))
}

#[derive(Debug, Clone)]
struct RepairOutcome {
    path: std::path::PathBuf,
    detected_enc: String,
    confidence: String,
    strategy: String,
    repair_chain: String,
    steps: Vec<String>,
    evidence_items: Vec<String>,
    evidence: String,
    changed: bool,
    skipped: bool,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
struct RepairJsonRecord {
    path: String,
    status: String,
    detected_encoding: String,
    confidence: String,
    strategy: String,
    repair_chain: String,
    changed: bool,
    skipped: bool,
    reason: String,
    steps: Vec<String>,
    evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RepairJsonSummary {
    changed: usize,
    unchanged: usize,
    skipped: usize,
    failures: usize,
}

#[derive(Debug, Clone, Serialize)]
struct RepairJsonReport {
    kind: &'static str,
    version: &'static str,
    input: String,
    directory_mode: bool,
    dry_run: bool,
    summary: RepairJsonSummary,
    records: Vec<RepairJsonRecord>,
    failures: Vec<String>,
    stdout_payload: Option<String>,
}

fn classify_repair_strategy(
    detected_enc: &str,
    steps: &[String],
    changed: bool,
    skipped: bool,
    reason: &str,
    confidence: &str,
) -> String {
    if skipped {
        if reason == "binary-guard" {
            return "manual_review_binary_guard".to_string();
        }
        return "manual_review_skipped".to_string();
    }
    if !changed {
        if steps.is_empty() {
            return "no_change".to_string();
        }
        return "cleanup_only".to_string();
    }
    if steps.iter().any(|s| s.contains("mojibake-repair-pass")) {
        if confidence == "high" {
            return "reencode_recover_high".to_string();
        }
        if confidence == "medium" {
            return "reencode_recover_medium".to_string();
        }
        return "reencode_recover_low".to_string();
    }
    if detected_enc.eq_ignore_ascii_case("mixed-auto") {
        return "mixed_decode_adjustment".to_string();
    }
    if confidence == "low" {
        return "manual_review_low_confidence".to_string();
    }
    "cleanup_or_reencode_general".to_string()
}

fn to_repair_json_record(outcome: &RepairOutcome) -> RepairJsonRecord {
    RepairJsonRecord {
        path: outcome.path.display().to_string(),
        status: if outcome.skipped {
            "skipped".to_string()
        } else if outcome.changed {
            "changed".to_string()
        } else {
            "unchanged".to_string()
        },
        detected_encoding: outcome.detected_enc.clone(),
        confidence: outcome.confidence.clone(),
        strategy: outcome.strategy.clone(),
        repair_chain: outcome.repair_chain.clone(),
        changed: outcome.changed,
        skipped: outcome.skipped,
        reason: outcome.reason.clone(),
        steps: outcome.steps.clone(),
        evidence: outcome.evidence_items.clone(),
    }
}

fn collect_repair_targets(
    root: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
) -> std::io::Result<()> {
    let entries = std::fs::read_dir(root)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_file() {
            out.push(path);
        } else if meta.is_dir() {
            collect_repair_targets(&path, out)?;
        }
    }
    Ok(())
}

fn run_single_repair(
    input_path: &std::path::Path,
    target_path: Option<&std::path::Path>,
    create_backup: bool,
    dry_run: bool,
) -> Result<RepairOutcome, CliError> {
    let bytes = std::fs::read(input_path).map_err(CliError::Io)?;
    if crate::core::encoding_fallback::is_probable_binary_bytes(&bytes) {
        return Ok(build_binary_guard_outcome(input_path));
    }

    let result = compute_single_repair_result(&bytes);
    persist_repaired_text(
        input_path,
        target_path,
        create_backup,
        dry_run,
        &result.repaired,
    )?;
    Ok(build_single_repair_outcome(input_path, result))
}

struct SingleRepairResult {
    detected_enc: String,
    repaired: String,
    steps: Vec<String>,
    confidence: String,
    evidence_items: Vec<String>,
    changed: bool,
}

fn compute_single_repair_result(bytes: &[u8]) -> SingleRepairResult {
    let (decoded, detected_enc) = crate::core::encoding_fallback::decode_with_fallback(bytes);
    let (repaired, steps) = crate::core::encoding_fallback::repair_text_for_display(&decoded);
    let (confidence, evidence_items) =
        crate::core::encoding_fallback::evaluate_repair_confidence(&decoded, &repaired, &steps);
    let changed = decoded != repaired;
    SingleRepairResult {
        detected_enc: detected_enc.to_string(),
        repaired,
        steps,
        confidence,
        evidence_items,
        changed,
    }
}

fn build_single_repair_outcome(
    input_path: &std::path::Path,
    result: SingleRepairResult,
) -> RepairOutcome {
    let summary = if result.steps.is_empty() {
        "none".to_string()
    } else {
        result.steps.join(", ")
    };
    let strategy = classify_repair_strategy(
        &result.detected_enc,
        &result.steps,
        result.changed,
        false,
        "",
        &result.confidence,
    );
    let evidence = format!("repairs={summary} | {}", result.evidence_items.join(" | "));
    RepairOutcome {
        path: input_path.to_path_buf(),
        detected_enc: result.detected_enc,
        confidence: result.confidence,
        strategy,
        repair_chain: summary.clone(),
        steps: result.steps,
        evidence_items: result.evidence_items,
        evidence,
        changed: result.changed,
        skipped: false,
        reason: String::new(),
    }
}

fn build_binary_guard_outcome(input_path: &std::path::Path) -> RepairOutcome {
    let steps = vec!["binary-guard-skip-repair".to_string()];
    let evidence_items = vec!["binary-guard=true".to_string()];
    RepairOutcome {
        path: input_path.to_path_buf(),
        detected_enc: "binary".to_string(),
        confidence: "low".to_string(),
        strategy: classify_repair_strategy("binary", &steps, false, true, "binary-guard", "low"),
        repair_chain: "none".to_string(),
        steps,
        evidence_items: evidence_items.clone(),
        evidence: evidence_items.join(" | "),
        changed: false,
        skipped: true,
        reason: "binary-guard".to_string(),
    }
}

fn persist_repaired_text(
    input_path: &std::path::Path,
    target_path: Option<&std::path::Path>,
    create_backup: bool,
    dry_run: bool,
    repaired: &str,
) -> Result<(), CliError> {
    if let Some(target) = target_path {
        if create_backup && !dry_run {
            let backup_path = default_backup_output_path(input_path);
            std::fs::copy(input_path, &backup_path).map_err(CliError::Io)?;
        }
        if !dry_run {
            crate::core::encoding_fallback::write_utf8(target, repaired).map_err(CliError::Io)?;
        }
    }
    Ok(())
}

fn apply_run_mode_defaults_from_argv(mut parsed: CliArgs, argv: &[String]) -> CliArgs {
    if !matches!(parsed.mode, CliMode::Run) {
        return parsed;
    }

    if !argv_has_format_flag(argv) {
        parsed.output_format = OutputFormat::Text;
    }
    if !argv_has_long_flag(argv, "--preset") {
        parsed.preset = Some(Preset::Ai);
    }

    parsed
}

fn rewrite_command_alias_to_flags(args: &[String]) -> Result<Option<Vec<String>>, CliError> {
    let cmd_index = match find_cmd_index(args) {
        Some(idx) => idx,
        None => return Ok(None),
    };

    let prog = args[0].clone();
    let cmd = args[cmd_index].to_ascii_lowercase();

    if !is_tokenslim_builtin_command(&cmd) {
        return Ok(None);
    }

    let before_cmd = &args[1..cmd_index];
    let rest: Vec<String> = args[cmd_index + 1..].to_vec();
    let doctor_rest = rewrite_doctor_flags(&rest);

    let mut rewritten = vec![prog];
    rewritten.extend_from_slice(before_cmd);
    match cmd.as_str() {
        "compress" => {
            rewritten.push("--mode".to_string());
            rewritten.push("compress".to_string());
            rewritten.extend(rest);
            Ok(Some(rewritten))
        }
        "decompress" => {
            rewritten.push("--mode".to_string());
            rewritten.push("decompress".to_string());
            rewritten.extend(rest);
            Ok(Some(rewritten))
        }
        "init" => {
            rewritten.push("--mode".to_string());
            rewritten.push("init".to_string());
            rewritten.extend(rest);
            Ok(Some(rewritten))
        }
        "workspace" => {
            rewritten.push("--doctor".to_string());
            rewritten.push("workspace".to_string());
            rewritten.extend(doctor_rest);
            Ok(Some(rewritten))
        }
        "encoding" => {
            rewritten.push("--doctor".to_string());
            rewritten.push("encoding".to_string());
            rewritten.extend(doctor_rest);
            Ok(Some(rewritten))
        }
        "rule" => {
            rewritten.push("--doctor".to_string());
            rewritten.push("rule".to_string());
            rewritten.extend(doctor_rest);
            Ok(Some(rewritten))
        }
        "env" => {
            rewritten.push("--doctor".to_string());
            rewritten.push("env".to_string());
            rewritten.extend(doctor_rest);
            Ok(Some(rewritten))
        }
        "gain" => {
            rewritten.push("--gain".to_string());
            rewritten.extend(rewrite_gain_flags(&rest));
            Ok(Some(rewritten))
        }
        "explain-plugin" | "explain_plugin" => {
            rewritten.push("--mode".to_string());
            rewritten.push("explain-plugin".to_string());
            rewritten.extend(rest);
            Ok(Some(rewritten))
        }
        "plugins" => {
            rewritten.push("--mode".to_string());
            rewritten.push("plugins".to_string());
            rewritten.extend(rest);
            Ok(Some(rewritten))
        }
        "repair-file" | "repair_file" => {
            if rest.is_empty() {
                return Err(CliError::InvalidArgs(
                    "repair-file requires an input path".to_string(),
                ));
            }
            let input_path = rest[0].clone();
            let tail = rest[1..].to_vec();
            rewritten.push("--mode".to_string());
            rewritten.push("repair-file".to_string());
            rewritten.push("--input".to_string());
            rewritten.push(input_path.clone());
            rewritten.extend(tail.clone());
            let inplace = argv_has_long_flag(&tail, "--inplace");
            if !argv_has_output_flag(&tail) && !inplace {
                rewritten.push("--output".to_string());
                rewritten.push(default_repair_output_path_from_input_arg(&input_path));
            }
            Ok(Some(rewritten))
        }
        "hooks" => {
            let action = if rest.is_empty() {
                ""
            } else {
                rest[0].as_str()
            };
            match action {
                "install" => {
                    rewritten.push("--init-hooks".to_string());
                    let mut i = 1;
                    while i < rest.len() {
                        if rest[i] == "--shell" && i + 1 < rest.len() {
                            rewritten.push("--hook-shell".to_string());
                            rewritten.push(rest[i + 1].clone());
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    Ok(Some(rewritten))
                }
                "uninstall" => {
                    rewritten.push("--uninstall-hooks".to_string());
                    let mut i = 1;
                    while i < rest.len() {
                        if rest[i] == "--shell" && i + 1 < rest.len() {
                            rewritten.push("--hook-shell".to_string());
                            rewritten.push(rest[i + 1].clone());
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    Ok(Some(rewritten))
                }
                "status" => {
                    rewritten.push("--mode".to_string());
                    rewritten.push("hooks-status".to_string());
                    let mut i = 1;
                    while i < rest.len() {
                        if rest[i] == "--shell" && i + 1 < rest.len() {
                            rewritten.push("--hook-shell".to_string());
                            rewritten.push(rest[i + 1].clone());
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    Ok(Some(rewritten))
                }
                _ => Err(CliError::InvalidArgs("Invalid hooks action".to_string())),
            }
        }
        "doctor" => Err(CliError::InvalidArgs(
            "`doctor` command has been removed. Use one of: `workspace`, `encoding`, `rule`, `env`"
                .to_string(),
        )),
        _ => Ok(None),
    }
}

fn rewrite_doctor_flags(args: &[String]) -> Vec<String> {
    args.iter()
        .map(|arg| match arg.as_str() {
            "--format" | "-f" => "--doctor-format".to_string(),
            _ => arg.clone(),
        })
        .collect()
}

fn rewrite_gain_flags(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut i = 0usize;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--daily" => out.push("--gain-daily".to_string()),
            "--by-filter" => out.push("--gain-by-filter".to_string()),
            "--json" => out.push("--gain-json".to_string()),
            "--days" => {
                out.push("--gain-days".to_string());
                if i + 1 < args.len() {
                    i += 1;
                    out.push(args[i].clone());
                }
            }
            _ if arg.starts_with("--days=") => {
                out.push(arg.replacen("--days=", "--gain-days=", 1));
            }
            _ => out.push(arg.clone()),
        }
        i += 1;
    }
    out
}

fn detect_external_like_first_arg(argv: &[String]) -> (String, String, bool) {
    let program = argv
        .first()
        .map(|s| program_name_from_argv0(s))
        .unwrap_or_else(|| "tokenslim".to_string());
    let first_user_arg = argv.get(1).cloned().unwrap_or_default();
    let is_external_like = !first_user_arg.is_empty()
        && !first_user_arg.starts_with('-')
        && !is_tokenslim_builtin_command(&first_user_arg);
    (program, first_user_arg, is_external_like)
}

fn map_clap_error(err: clap::Error, argv: &[String]) -> CliError {
    let (program, first_user_arg, is_external_like) = detect_external_like_first_arg(argv);

    let mut hints = String::new();
    if is_external_like {
        let friendly = format_invalid_args_message(
            "E_CLI_ARG_UNKNOWN_EXTERNAL",
            format!("检测到未知参数 `{}`，它看起来像外部命令。", first_user_arg),
            format!(
                "Detected unknown argument `{}`. It looks like an external command.",
                first_user_arg
            ),
            Some(format!(
                "可尝试: {program} run {first_user_arg} ... 或 {program} {first_user_arg} ..."
            )),
            Some(format!(
                "Try: {program} run {first_user_arg} ... or {program} {first_user_arg} ..."
            )),
        );
        hints.push_str(&friendly);
        hints.push_str("\n\n");
    } else {
        let friendly = format_invalid_args_message(
            "E_CLI_INVALID_ARGS",
            "命令参数无效。",
            "Invalid command arguments.",
            Some(format!("运行 `{program} --help` 查看可用命令。")),
            Some(format!(
                "Run `{program} --help` to view supported commands."
            )),
        );
        hints.push_str(&friendly);
        hints.push_str("\n\n");
        hints.push_str("Invalid arguments.\n\n");
    }

    hints.push_str(&render_global_usage(&program));
    hints.push_str("\n\n");
    hints.push_str(&err.to_string());
    CliError::InvalidArgs(hints)
}

fn reject_legacy_flags(args: &[String]) -> Result<(), CliError> {
    let has_mode = args.iter().any(|a| a == "--mode");
    let has_doctor = args.iter().any(|a| a == "--doctor");
    let has_doctor_format = args.iter().any(|a| a == "--doctor-format");

    if has_mode || has_doctor || has_doctor_format {
        return Err(CliError::InvalidArgs(format_invalid_args_message(
            "E_CLI_LEGACY_FLAGS_REMOVED",
            "旧参数 `--mode/--doctor/--doctor-format` 已移除，请改用子命令。",
            "Legacy flags `--mode/--doctor/--doctor-format` were removed. Please use command-style subcommands.",
            Some("示例: tokenslim run git status；tokenslim workspace --format llm".to_string()),
            Some("Example: tokenslim run git status; tokenslim workspace --format llm".to_string()),
        )));
    }

    Ok(())
}

const HOOK_BEGIN: &str = "# >>> tokenslim hook >>>";
const HOOK_END: &str = "# <<< tokenslim hook <<<";

fn build_run_mode_args(run_command: Vec<String>, explain_route: bool) -> CliArgs {
    CliArgs {
        mode: CliMode::Run,
        input: InputSource::Stdin,
        output: OutputTarget::Stdout,
        verbose: false,
        calc_tokens: false,
        reorder: false,
        semantic: false,
        normalize: false,
        ai_export: true,
        ai_signal: true,
        output_format: OutputFormat::Text,
        verify_rule: None,
        verify_fixture: None,
        verify_expected: None,
        init_hooks: false,
        uninstall_hooks: false,
        hook_shell: None,
        dry_run: false,
        init: false,
        no_hooks: false,
        force: false,
        gain: false,
        gain_daily: false,
        gain_by_filter: false,
        gain_json: false,
        gain_days: 7,
        doctor: None,
        doctor_format: DoctorOutputFormat::Text,
        doctor_strict: false,
        inject: false,
        config: None,
        run_command,
        explain_route,
        explain_command: None,
        explain_fallback_gap: 0.15,
        explain_replay_out: None,
        preset: Some(Preset::Ai),
        fix: false,
        safety: false,
        rewrite: None,
        discover: Vec::new(),
        inplace: false,
        backup: false,
        include: Vec::new(),
        exclude: Vec::new(),
    }
}

fn parse_output_format_arg(format: &str) -> Result<OutputFormat, CliError> {
    format.parse().map_err(|e: String| CliError::InvalidArgs(e))
}

fn parse_doctor_output_format_arg(format: &str) -> Result<DoctorOutputFormat, CliError> {
    format
        .parse::<DoctorOutputFormat>()
        .map_err(CliError::InvalidArgs)
}

fn parse_preset_arg(preset: Option<&str>) -> Result<Option<Preset>, CliError> {
    preset
        .map(|p| p.parse::<Preset>().map_err(CliError::InvalidArgs))
        .transpose()
}

fn build_cli_args_from_raw(
    cli: CliRawArgs,
    mode: CliMode,
    hook_shell: Option<HookShell>,
    doctor: Option<DoctorKind>,
    output_format: OutputFormat,
    doctor_format: DoctorOutputFormat,
    preset: Option<Preset>,
) -> CliArgs {
    let input = match cli.input {
        Some(path) => InputSource::File(path),
        None => InputSource::Stdin,
    };
    let output = match cli.output {
        Some(path) => OutputTarget::File(path),
        None => OutputTarget::Stdout,
    };

    CliArgs {
        mode,
        input,
        output,
        verbose: cli.verbose,
        calc_tokens: cli.calc_tokens,
        reorder: cli.reorder,
        semantic: cli.semantic,
        normalize: cli.normalize,
        ai_export: cli.ai_export,
        ai_signal: cli.ai_signal,
        output_format,
        verify_rule: cli.verify_rule,
        verify_fixture: cli.verify_fixture,
        verify_expected: cli.verify_expected,
        init_hooks: cli.init_hooks,
        uninstall_hooks: cli.uninstall_hooks,
        hook_shell,
        dry_run: cli.dry_run,
        init: cli.init,
        no_hooks: cli.no_hooks,
        force: cli.force,
        gain: cli.gain,
        gain_daily: cli.gain_daily,
        gain_by_filter: cli.gain_by_filter,
        gain_json: cli.gain_json,
        gain_days: cli.gain_days,
        doctor,
        doctor_format,
        doctor_strict: cli.strict,
        inject: cli.inject,
        config: cli.config,
        run_command: cli.run_command,
        explain_route: cli.explain_route,
        explain_command: cli.explain_command,
        explain_fallback_gap: cli.explain_fallback_gap,
        explain_replay_out: cli.explain_replay_out,
        preset,
        fix: cli.fix,
        safety: cli.safety,
        rewrite: cli.rewrite,
        discover: cli.discover,
        inplace: cli.inplace,
        backup: cli.backup,
        include: cli.include,
        exclude: cli.exclude,
    }
}

impl CliArgs {
    pub fn parse_args() -> Result<Self, CliError> {
        let argv: Vec<String> = std::env::args().collect();
        parse_args_from_argv(&argv)
    }

    fn from_raw(cli: CliRawArgs) -> Result<Self, CliError> {
        let mode = resolve_cli_mode(&cli);
        validate_repair_file_scoped_args(&cli, &mode)?;
        validate_exclusive_feature_flags(&cli)?;
        validate_verify_triplet(&cli)?;
        let hook_shell = parse_optional_hook_shell(cli.hook_shell.as_deref())?;
        let doctor = parse_optional_doctor(cli.doctor.as_deref())?;
        let output_format = parse_output_format_arg(&cli.format)?;
        let doctor_format = parse_doctor_output_format_arg(&cli.doctor_format)?;
        let preset = parse_preset_arg(cli.preset.as_deref())?;

        Ok(build_cli_args_from_raw(
            cli,
            mode,
            hook_shell,
            doctor,
            output_format,
            doctor_format,
            preset,
        ))
    }
}

fn parse_args_from_argv(argv: &[String]) -> Result<CliArgs, CliError> {
    if let Some(run_args) = parse_run_mode_args_from_argv(argv) {
        return Ok(run_args);
    }

    reject_legacy_flags(argv)?;

    let parsed = if let Some(rewritten) = rewrite_command_alias_to_flags(argv)? {
        parse_cli_args_with_clap(&rewritten)?
    } else {
        parse_cli_args_with_clap(argv)?
    };

    Ok(apply_run_mode_defaults_from_argv(parsed, argv))
}

fn parse_run_mode_args_from_argv(argv: &[String]) -> Option<CliArgs> {
    if let Some(run_cmd) = maybe_parse_run_subcommand_from_argv(argv) {
        let (explain_route, run_cmd) = split_run_explain_route_flag(run_cmd);
        return Some(build_run_mode_args(run_cmd, explain_route));
    }
    if let Some(run_cmd) = maybe_parse_implicit_run_command_from_argv(argv) {
        return Some(build_run_mode_args(run_cmd, false));
    }
    None
}

fn parse_cli_args_with_clap(argv: &[String]) -> Result<CliArgs, CliError> {
    let cli = <CliRawArgs as clap::Parser>::try_parse_from(argv.to_vec())
        .map_err(|e| map_clap_error(e, argv))?;
    CliArgs::from_raw(cli)
}

fn resolve_cli_mode(cli: &CliRawArgs) -> CliMode {
    match cli.mode.as_deref() {
        Some("compress") => CliMode::Compress,
        Some("decompress") => CliMode::Decompress,
        Some("init") => CliMode::Init,

        Some("run") => CliMode::Run,
        Some("hooks-status") => CliMode::HooksStatus,
        Some("explain-plugin") | Some("explain_plugin") => CliMode::ExplainPlugin,
        Some("plugins") => CliMode::Plugins,
        Some("repair-file") | Some("repair_file") => CliMode::RepairFile,
        _ => {
            if !cli.run_command.is_empty() {
                CliMode::Run
            } else {
                CliMode::Compress
            }
        }
    }
}

fn validate_repair_file_scoped_args(cli: &CliRawArgs, mode: &CliMode) -> Result<(), CliError> {
    if cli.inplace && !matches!(mode, CliMode::RepairFile) {
        return Err(CliError::InvalidArgs(
            "--inplace is only available for `repair-file` mode".to_string(),
        ));
    }
    if cli.backup && !matches!(mode, CliMode::RepairFile) {
        return Err(CliError::InvalidArgs(
            "--backup is only available for `repair-file` mode".to_string(),
        ));
    }
    if (!cli.include.is_empty() || !cli.exclude.is_empty()) && !matches!(mode, CliMode::RepairFile)
    {
        return Err(CliError::InvalidArgs(
            "--include/--exclude are only available for `repair-file` mode".to_string(),
        ));
    }
    Ok(())
}

fn validate_exclusive_feature_flags(cli: &CliRawArgs) -> Result<(), CliError> {
    if cli.ai_export && cli.ai_signal {
        return Err(CliError::InvalidArgs(
            "--ai-export and --ai-signal cannot be used together".to_string(),
        ));
    }
    if cli.init_hooks && cli.uninstall_hooks {
        return Err(CliError::InvalidArgs(
            "--init-hooks and --uninstall-hooks cannot be used together".to_string(),
        ));
    }
    Ok(())
}

fn validate_verify_triplet(cli: &CliRawArgs) -> Result<(), CliError> {
    let verify_params = [
        cli.verify_rule.is_some(),
        cli.verify_fixture.is_some(),
        cli.verify_expected.is_some(),
    ];
    let verify_count = verify_params.iter().filter(|v| **v).count();
    if verify_count != 0 && verify_count != 3 {
        return Err(CliError::InvalidArgs(
            "--verify-rule/--verify-fixture/--verify-expected must be provided together"
                .to_string(),
        ));
    }
    Ok(())
}

fn parse_optional_hook_shell(shell: Option<&str>) -> Result<Option<HookShell>, CliError> {
    if let Some(shell) = shell {
        return HookShell::parse(shell)
            .ok_or_else(|| {
                CliError::InvalidArgs(format!(
                    "unsupported --hook-shell: {shell} (expected bash|zsh|fish)"
                ))
            })
            .map(Some);
    }
    Ok(None)
}

fn parse_optional_doctor(doctor: Option<&str>) -> Result<Option<DoctorKind>, CliError> {
    if let Some(d) = doctor {
        return d
            .parse::<DoctorKind>()
            .map(Some)
            .map_err(CliError::InvalidArgs);
    }
    Ok(None)
}

fn detect_shell() -> HookShell {
    let shell_env = std::env::var("SHELL")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if shell_env.contains("zsh") {
        return HookShell::Zsh;
    }
    if shell_env.contains("fish") {
        return HookShell::Fish;
    }
    if shell_env.contains("bash") {
        return HookShell::Bash;
    }
    if std::env::var("PSModulePath").is_ok() || cfg!(target_os = "windows") {
        return HookShell::PowerShell;
    }
    HookShell::Bash
}

fn resolve_home_dir() -> Result<std::path::PathBuf, CliError> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .map_err(|_| CliError::Config("unable to resolve HOME/USERPROFILE".to_string()))
}

fn shell_rc_paths(shell: HookShell) -> Result<Vec<std::path::PathBuf>, CliError> {
    if shell == HookShell::PowerShell {
        let mut paths = Vec::new();
        let home = resolve_home_dir()?;

        let pwsh_path = if cfg!(target_os = "windows") {
            home.join("Documents\\PowerShell\\Microsoft.PowerShell_profile.ps1")
        } else {
            home.join(".config/powershell/Microsoft.PowerShell_profile.ps1")
        };
        paths.push(pwsh_path);

        if cfg!(target_os = "windows") {
            paths.push(home.join("Documents\\WindowsPowerShell\\Microsoft.PowerShell_profile.ps1"));
        }

        for ps_exe in ["pwsh", "powershell"] {
            if let Ok(output) = std::process::Command::new(ps_exe)
                .arg("-NoProfile")
                .arg("-Command")
                .arg("Write-Output $PROFILE")
                .output()
            {
                if output.status.success() {
                    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !path_str.is_empty() {
                        let path = std::path::PathBuf::from(path_str);
                        if !paths.contains(&path) {
                            paths.push(path);
                        }
                    }
                }
            }
        }
        return Ok(paths);
    }

    let home = resolve_home_dir()?;
    let file = match shell {
        HookShell::Bash => ".bashrc",
        HookShell::Zsh => ".zshrc",
        HookShell::Fish => ".config/fish/config.fish",
        HookShell::PowerShell => unreachable!(),
    };
    Ok(vec![home.join(file)])
}

fn hook_block(shell: HookShell) -> String {
    let content = crate::core::init_command::generate_hook_content(shell.as_str());
    format!("{HOOK_BEGIN}\n{content}\n{HOOK_END}\n")
}

fn remove_hook_block(content: &str) -> String {
    if let (Some(start), Some(end)) = (content.find(HOOK_BEGIN), content.find(HOOK_END)) {
        let end_with_marker = end + HOOK_END.len();
        let mut out = String::new();
        out.push_str(&content[..start]);
        out.push_str(content[end_with_marker..].trim_start_matches(['\r', '\n']));
        return out;
    }
    content.to_string()
}

fn install_hooks(shell: HookShell, dry_run: bool) -> Result<(), CliError> {
    let rc_paths = shell_rc_paths(shell)?;
    let block = hook_block(shell);

    for rc_path in rc_paths {
        if dry_run {
            println!(
                "[init-hooks][dry-run] shell={} rc={}",
                shell.as_str(),
                rc_path.display()
            );
            println!("{block}");
            continue;
        }

        let existing = std::fs::read_to_string(&rc_path).unwrap_or_default();
        let cleaned = remove_hook_block(&existing);

        if let Some(parent) = rc_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let mut merged = cleaned.trim_end().to_string();
        if !merged.is_empty() {
            merged.push('\n');
        }
        merged.push_str(&block);
        if let Err(e) = std::fs::write(&rc_path, merged) {
            eprintln!("Failed to write {}: {}", rc_path.display(), e);
            continue;
        }
        println!(
            "{}",
            crate::utils::i18n::t2("hooks_init_installed", shell.as_str(), rc_path.display())
        );
    }

    if !dry_run {
        let reload_cmd = match shell {
            HookShell::Bash | HookShell::Zsh | HookShell::Fish => "source ~/.bashrc".to_string(),
            HookShell::PowerShell => ". $PROFILE".to_string(),
        };
        println!(
            "👉 {} `{}`",
            crate::utils::i18n::t("hooks_reload_hint"),
            reload_cmd
        );
    }
    Ok(())
}

fn check_hooks_status(shell: HookShell) -> Result<(), CliError> {
    let rc_paths = shell_rc_paths(shell)?;
    let mut installed_anywhere = false;

    for rc_path in rc_paths {
        if !rc_path.exists() {
            println!(
                "[hooks-status] shell={} rc={} installed=false (file not found)",
                shell.as_str(),
                rc_path.display()
            );
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&rc_path) {
            let installed = content.contains(HOOK_BEGIN);
            if installed {
                installed_anywhere = true;
            }
            println!(
                "[hooks-status] shell={} rc={} installed={}",
                shell.as_str(),
                rc_path.display(),
                installed
            );
        } else {
            println!(
                "[hooks-status] shell={} rc={} installed=false (read error)",
                shell.as_str(),
                rc_path.display()
            );
        }
    }

    if installed_anywhere {
        println!(
            "\nTokenSlim hooks are currently INSTALLED for {}.",
            shell.as_str()
        );
    } else {
        println!(
            "\nTokenSlim hooks are NOT installed for {}.",
            shell.as_str()
        );
    }

    Ok(())
}

fn uninstall_hooks(shell: HookShell, dry_run: bool) -> Result<(), CliError> {
    let rc_paths = shell_rc_paths(shell)?;

    for rc_path in rc_paths {
        if dry_run {
            println!(
                "{}",
                crate::utils::i18n::t2(
                    "hooks_uninstall_dry_run",
                    shell.as_str(),
                    rc_path.display()
                )
            );
            continue;
        }

        let existing = std::fs::read_to_string(&rc_path).unwrap_or_default();
        let cleaned = remove_hook_block(&existing);

        if existing != cleaned {
            if let Err(e) = std::fs::write(&rc_path, cleaned) {
                eprintln!("Failed to write {}: {}", rc_path.display(), e);
                continue;
            }
            println!(
                "{}",
                crate::utils::i18n::t1(
                    "hooks_uninstall_removed",
                    rc_path.display().to_string().as_str()
                )
            );
        } else {
            println!(
                "{}",
                crate::utils::i18n::t1(
                    "hooks_uninstall_not_found",
                    rc_path.display().to_string().as_str()
                )
            );
        }
    }
    if !dry_run {
        println!("👉 {}", crate::utils::i18n::t("hooks_restart_hint"));
    }
    Ok(())
}

fn validate_repair_file_request(
    input_path: &std::path::Path,
    args: &CliArgs,
) -> Result<(), CliError> {
    if args.backup && !args.inplace {
        return Err(CliError::InvalidArgs(
            "--backup requires --inplace in repair-file mode".to_string(),
        ));
    }

    if input_path.is_dir() {
        if !args.inplace {
            return Err(CliError::InvalidArgs(
                "repair-file directory mode requires --inplace".to_string(),
            ));
        }
        if matches!(args.output, OutputTarget::File(_)) {
            return Err(CliError::InvalidArgs(
                "repair-file directory mode does not support --output file".to_string(),
            ));
        }
    }

    Ok(())
}

fn resolve_single_repair_target<'a>(
    input_path: &'a std::path::Path,
    args: &'a CliArgs,
) -> Option<&'a std::path::Path> {
    if args.inplace {
        return Some(input_path);
    }
    match &args.output {
        OutputTarget::File(path) => Some(path.as_path()),
        OutputTarget::Stdout => None,
    }
}

fn run_repair_file_command(args: &CliArgs) -> Result<(), CliError> {
    let json_mode = matches!(args.output_format, OutputFormat::Json);
    let input_path = match &args.input {
        InputSource::File(path) => path,
        InputSource::Stdin => {
            return Err(CliError::InvalidArgs(
                "repair-file requires an input file path".to_string(),
            ));
        }
    };
    validate_repair_file_request(input_path, args)?;

    if input_path.is_dir() {
        return run_repair_file_directory_mode(args, input_path, json_mode);
    }

    run_repair_file_single_mode(args, input_path, json_mode)
}

fn run_repair_file_directory_mode(
    args: &CliArgs,
    input_path: &std::path::Path,
    json_mode: bool,
) -> Result<(), CliError> {
    let mut targets = Vec::new();
    collect_repair_targets(input_path, &mut targets).map_err(CliError::Io)?;
    targets.sort();

    let mut changed = 0usize;
    let mut unchanged = 0usize;
    let mut skipped = 0usize;
    let mut failures = 0usize;
    let mut records = Vec::<RepairJsonRecord>::new();
    let mut failure_messages = Vec::<String>::new();
    if !json_mode {
        println!(
            "repair-file: scanning directory `{}` (files={}) dry_run={}",
            input_path.display(),
            targets.len(),
            args.dry_run
        );
    }

    for path in &targets {
        if !should_repair_path(input_path, path, &args.include, &args.exclude) {
            continue;
        }
        match run_single_repair(path, Some(path), args.backup, args.dry_run) {
            Ok(outcome) => {
                records.push(to_repair_json_record(&outcome));
                if outcome.skipped {
                    skipped += 1;
                    if !json_mode {
                        println!(
                            "[SKIP] {} reason={} strategy={} chain={} evidence={}",
                            outcome.path.display(),
                            outcome.reason,
                            outcome.strategy,
                            outcome.repair_chain,
                            outcome.evidence
                        );
                    }
                } else if outcome.changed {
                    changed += 1;
                    if !json_mode {
                        println!(
                            "[CHANGED] {} enc={} confidence={} strategy={} chain={} {}",
                            outcome.path.display(),
                            outcome.detected_enc,
                            outcome.confidence,
                            outcome.strategy,
                            outcome.repair_chain,
                            outcome.evidence
                        );
                    }
                } else {
                    unchanged += 1;
                    if !json_mode {
                        println!(
                            "[UNCHANGED] {} enc={} confidence={} strategy={} chain={} {}",
                            outcome.path.display(),
                            outcome.detected_enc,
                            outcome.confidence,
                            outcome.strategy,
                            outcome.repair_chain,
                            outcome.evidence
                        );
                    }
                }
            }
            Err(err) => {
                failures += 1;
                let msg = format!("{}: {}", path.display(), err);
                failure_messages.push(msg.clone());
                if !json_mode {
                    eprintln!("{}", t1("repair_error", msg));
                }
            }
        }
    }

    if json_mode {
        let report = RepairJsonReport {
            kind: "repair_file_report",
            version: "repair.v1",
            input: input_path.display().to_string(),
            directory_mode: true,
            dry_run: args.dry_run,
            summary: RepairJsonSummary {
                changed,
                unchanged,
                skipped,
                failures,
            },
            records,
            failures: failure_messages,
            stdout_payload: None,
        };
        let json = serde_json::to_string_pretty(&report).map_err(CliError::Serialization)?;
        println!("{}", json);
    } else {
        println!(
            "repair-file: summary changed={} unchanged={} skipped={} failures={} dry_run={}",
            changed, unchanged, skipped, failures, args.dry_run
        );
    }
    if failures > 0 {
        return Err(CliError::Config(format!(
            "repair-file directory mode finished with {} failures",
            failures
        )));
    }
    Ok(())
}

fn run_repair_file_single_mode(
    args: &CliArgs,
    input_path: &std::path::Path,
    json_mode: bool,
) -> Result<(), CliError> {
    if should_skip_single_repair_by_filters(args, input_path) {
        if json_mode {
            let skipped = build_include_exclude_skipped_outcome(input_path);
            emit_single_mode_json_report(args, input_path, &skipped, None)?;
        } else {
            println!(
                "repair-file: skipped `{}` by include/exclude filter.",
                input_path.display()
            );
        }
        return Ok(());
    }

    let target = resolve_single_repair_target(input_path, args);
    let outcome = run_single_repair(input_path, target, args.backup, args.dry_run)?;
    if outcome.skipped {
        if json_mode {
            emit_single_mode_json_report(args, input_path, &outcome, None)?;
        } else {
            println!(
                "repair-file: skipped `{}` reason={} evidence={}",
                outcome.path.display(),
                outcome.reason,
                outcome.evidence
            );
        }
        return Ok(());
    }

    let mut stdout_payload = None::<String>;
    match target {
        Some(t) => {
            if !json_mode {
                println!(
                    "repair-file: {} `{}` (decoded by {}, confidence={}, strategy={}, chain={}, dry_run={}).",
                    if args.dry_run {
                        "would write"
                    } else {
                        "written"
                    },
                    t.display(),
                    outcome.detected_enc,
                    outcome.confidence,
                    outcome.strategy,
                    outcome.repair_chain,
                    args.dry_run
                );
                println!("{}", t1("repair_evidence", &outcome.evidence));
            }
        }
        None => {
            let bytes = std::fs::read(input_path).map_err(CliError::Io)?;
            let (decoded, _) = crate::core::encoding_fallback::decode_with_fallback(&bytes);
            let (repaired, _) = crate::core::encoding_fallback::repair_text_for_display(&decoded);
            if json_mode {
                stdout_payload = Some(repaired.clone());
            } else {
                println!("{}", repaired);
                eprintln!(
                    "repair-file: decoded-by={}, confidence={}, strategy={}, chain={}, evidence={}",
                    outcome.detected_enc,
                    outcome.confidence,
                    outcome.strategy,
                    outcome.repair_chain,
                    outcome.evidence
                );
            }
        }
    }

    if json_mode {
        emit_single_mode_json_report(args, input_path, &outcome, stdout_payload)?;
    }

    Ok(())
}

fn should_skip_single_repair_by_filters(args: &CliArgs, input_path: &std::path::Path) -> bool {
    if args.include.is_empty() && args.exclude.is_empty() {
        return false;
    }
    !should_repair_path(
        input_path.parent().unwrap_or(std::path::Path::new(".")),
        input_path,
        &args.include,
        &args.exclude,
    )
}

fn build_include_exclude_skipped_outcome(input_path: &std::path::Path) -> RepairOutcome {
    RepairOutcome {
        path: input_path.to_path_buf(),
        detected_enc: "unknown".to_string(),
        confidence: "low".to_string(),
        strategy: "manual_review_skipped".to_string(),
        repair_chain: "none".to_string(),
        steps: Vec::new(),
        evidence_items: vec!["include-exclude-filter=true".to_string()],
        evidence: "include-exclude-filter=true".to_string(),
        changed: false,
        skipped: true,
        reason: "include-exclude-filter".to_string(),
    }
}

fn build_single_mode_json_report(
    args: &CliArgs,
    input_path: &std::path::Path,
    outcome: &RepairOutcome,
    stdout_payload: Option<String>,
) -> RepairJsonReport {
    let (changed, unchanged, skipped) = if outcome.skipped {
        (0, 0, 1)
    } else if outcome.changed {
        (1, 0, 0)
    } else {
        (0, 1, 0)
    };
    RepairJsonReport {
        kind: "repair_file_report",
        version: "repair.v1",
        input: input_path.display().to_string(),
        directory_mode: false,
        dry_run: args.dry_run,
        summary: RepairJsonSummary {
            changed,
            unchanged,
            skipped,
            failures: 0,
        },
        records: vec![to_repair_json_record(outcome)],
        failures: Vec::new(),
        stdout_payload,
    }
}

fn emit_single_mode_json_report(
    args: &CliArgs,
    input_path: &std::path::Path,
    outcome: &RepairOutcome,
    stdout_payload: Option<String>,
) -> Result<(), CliError> {
    let report = build_single_mode_json_report(args, input_path, outcome, stdout_payload);
    let json = serde_json::to_string_pretty(&report).map_err(CliError::Serialization)?;
    println!("{}", json);
    Ok(())
}

fn normalize_for_compare(s: &str) -> String {
    s.replace("\r\n", "\n").trim_end().to_string()
}

fn verify_text_pair(actual: &str, expected: &str) -> Result<(), String> {
    let actual = normalize_for_compare(actual);
    let expected = normalize_for_compare(expected);

    if actual == expected {
        return Ok(());
    }

    let prefix_len = actual
        .chars()
        .zip(expected.chars())
        .take_while(|(a, b)| a == b)
        .count();
    Err(format!(
        "[verify] FAIL at char {prefix_len}\n--- expected ---\n{expected}\n--- actual ---\n{actual}"
    ))
}

fn flatten_tokens(tokens: &[Token<'_>]) -> String {
    let mut out = String::new();
    for token in tokens {
        match token {
            Token::Text(s) => out.push_str(s.as_ref()),
            Token::DictRef(s) => out.push_str(s.as_ref()),
            Token::Marker { value, .. } => out.push_str(value.as_ref()),
            Token::Repeat { token, count } => {
                let inner = flatten_tokens(std::slice::from_ref(token));
                for _ in 0..*count {
                    out.push_str(&inner);
                }
            }
            Token::Diff { base, patch } => {
                out.push_str(base.as_ref());
                out.push_str(patch.as_ref());
            }
        }
    }
    out
}

fn should_quote_run_anchor_token(token: &str) -> bool {
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

fn quote_run_anchor_token(token: &str) -> String {
    if !should_quote_run_anchor_token(token) {
        return token.to_string();
    }
    let escaped = token.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn tokenize_command_line(line: &str) -> Option<Vec<String>> {
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

fn is_equivalent_run_anchor_line(line: &str, prog: &str, cmd_args: &[String]) -> bool {
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

fn build_run_command_anchor(prog: &str, cmd_args: &[String]) -> String {
    let mut parts = Vec::with_capacity(cmd_args.len() + 1);
    parts.push(quote_run_anchor_token(prog));
    for arg in cmd_args {
        parts.push(quote_run_anchor_token(arg));
    }
    parts.join(" ")
}

fn is_explicit_vcs_command_line(line: &str) -> bool {
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

fn is_explicit_run_command_line(line: &str, prog: &str, cmd_args: &[String]) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return false;
    }

    if is_equivalent_run_anchor_line(trimmed, prog, cmd_args) {
        return true;
    }

    false
}

fn prepend_run_command_anchor_if_needed(combined: &str, prog: &str, cmd_args: &[String]) -> String {
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

fn build_single_document_slice<'a>(input: &'a str, line_count: usize) -> Slice<'a> {
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

fn build_compression_metadata_from_tokens(
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

fn compress_vcs_run_as_single_document(input: &str) -> CompressionOutput {
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
enum VcsRunIntent {
    Status,
    Log,
    Diff,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunPluginRoute {
    Vcs,
    Node,
    Build,
    Generic,
}

fn command_keyword(prog: &str) -> String {
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
fn load_run_routes() -> Vec<RunRouteCapability> {
    let config_dir = std::path::Path::new("config").join("plugins");
    plugin_config_loader::load_run_route_capabilities(if config_dir.exists() {
        Some(&config_dir)
    } else {
        None
    })
}

fn detect_run_plugin_route(prog: &str, cmd_args: &[String]) -> RunPluginRoute {
    let caps = load_run_routes();
    let route = plugin_config_loader::resolve_run_route(&caps, prog, cmd_args);
    match route.route_group.as_str() {
        "vcs" => RunPluginRoute::Vcs,
        "node" => RunPluginRoute::Node,
        "build" => RunPluginRoute::Build,
        _ => RunPluginRoute::Generic,
    }
}

fn remove_vcs_plugins(plugins: Vec<Box<dyn Plugin>>) -> Vec<Box<dyn Plugin>> {
    plugins
        .into_iter()
        .filter(|p| !matches!(p.name(), "vcs" | "git_diff"))
        .collect()
}

fn keep_generic_run_plugins(plugins: Vec<Box<dyn Plugin>>) -> Vec<Box<dyn Plugin>> {
    plugins
        .into_iter()
        .filter(|p| matches!(p.name(), "generic_text" | "ansi_cleaner" | "noise_filter"))
        .collect()
}

fn plugins_for_run_command(prog: &str, cmd_args: &[String]) -> Vec<Box<dyn Plugin>> {
    let route = detect_run_plugin_route(prog, cmd_args);
    let plugins = get_plugins();

    match route {
        RunPluginRoute::Vcs => plugins,
        RunPluginRoute::Node | RunPluginRoute::Build => remove_vcs_plugins(plugins),
        RunPluginRoute::Generic => keep_generic_run_plugins(plugins),
    }
}

fn explain_run_route(prog: &str, cmd_args: &[String], args: &CliArgs) -> String {
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

/// 获取 VCS 意图（兼容旧代码的 Option 签名）
#[derive(Debug, Clone, Default)]
struct PluginCapabilityEvidence {
    description: String,
    tags: String,
    route_group: String,
    sample_cases: u64,
    showcase_cases: u64,
    audit_cases: u64,
    frozen_cases: u64,
    coverage_status: String,
    detect_patterns: Vec<String>,
}

fn capability_index_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("audit")
        .join("plugin_capability_index.json")
}

fn json_u64(value: &serde_json::Value, key: &str) -> u64 {
    value.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

fn json_string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn parse_detect_patterns(value: &serde_json::Value) -> Vec<String> {
    value
        .get("detect_patterns")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .take(5)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn find_plugin_entry<'a>(
    plugins: &'a [serde_json::Value],
    plugin_name: &str,
) -> Option<&'a serde_json::Value> {
    plugins.iter().find(|plugin| {
        plugin
            .get("name")
            .and_then(|n| n.as_str())
            .is_some_and(|name| name == plugin_name)
    })
}

fn parse_plugin_capability_evidence(plugin: &serde_json::Value) -> PluginCapabilityEvidence {
    PluginCapabilityEvidence {
        description: json_string(plugin, "description"),
        tags: json_string(plugin, "capability_tags"),
        route_group: json_string(plugin, "route_group"),
        sample_cases: json_u64(plugin, "sample_cases"),
        showcase_cases: json_u64(plugin, "showcase_cases"),
        audit_cases: json_u64(plugin, "audit_cases"),
        frozen_cases: json_u64(plugin, "frozen_cases"),
        coverage_status: json_string(plugin, "coverage_status"),
        detect_patterns: parse_detect_patterns(plugin),
    }
}

fn load_plugin_capability_evidence(plugin_name: &str) -> Option<PluginCapabilityEvidence> {
    let bytes = std::fs::read(capability_index_path()).ok()?;
    let content = decode_capability_index_text(&bytes);
    let root: serde_json::Value = serde_json::from_str(&content).ok()?;
    let plugins = root.get("plugins")?.as_array()?;
    let plugin = find_plugin_entry(plugins, plugin_name)?;
    Some(parse_plugin_capability_evidence(plugin))
}

fn decode_capability_index_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16_lossy(&units)
            .trim_start_matches('\u{feff}')
            .to_string();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16_lossy(&units)
            .trim_start_matches('\u{feff}')
            .to_string();
    }
    String::from_utf8_lossy(bytes)
        .trim_start_matches('\u{feff}')
        .to_string()
}

fn sanitize_explain_field(value: &str) -> String {
    value
        .replace('\r', " ")
        .replace('\n', " ")
        .replace('|', "/")
        .trim()
        .to_string()
}

fn parse_explain_report_pairs(report: &str) -> Vec<(String, String)> {
    report
        .lines()
        .filter_map(|line| {
            line.split_once('=')
                .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

fn parse_explain_pipe_attributes(value: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut attrs = serde_json::Map::new();
    let mut parts = value.split('|');
    if let Some(first) = parts.next() {
        let first = first.trim();
        if !first.is_empty() {
            if let Some((k, v)) = first.split_once('=') {
                attrs.insert(
                    k.trim().to_string(),
                    serde_json::Value::String(v.trim().to_string()),
                );
            } else if let Some((k, v)) = first.split_once(':') {
                attrs.insert(
                    k.trim().to_string(),
                    serde_json::Value::String(v.trim().to_string()),
                );
            } else {
                attrs.insert(
                    "primary".to_string(),
                    serde_json::Value::String(first.to_string()),
                );
            }
        }
    }
    for part in parts {
        if let Some((k, v)) = part.split_once('=') {
            attrs.insert(
                k.trim().to_string(),
                serde_json::Value::String(v.trim().to_string()),
            );
        } else if let Some((k, v)) = part.split_once(':') {
            attrs.insert(
                k.trim().to_string(),
                serde_json::Value::String(v.trim().to_string()),
            );
        }
    }
    attrs
}

fn parse_explain_alternative_index(key: &str) -> Option<usize> {
    let suffix = key.strip_prefix("alternative_")?;
    let index_part = suffix.split('_').next()?;
    index_part.parse::<usize>().ok().filter(|idx| *idx > 0)
}

fn parse_capability_line_to_json(raw: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut attrs = parse_explain_pipe_attributes(raw);
    if let Some(primary) = attrs.remove("primary") {
        attrs.insert("description".to_string(), primary);
    }
    attrs
}

fn explain_required_fields() -> &'static [&'static str] {
    &[
        "input_kind",
        "selected_plugin",
        "fallback_decision",
        "retry_plugin",
        "recommendation_primary",
        "recommendation_confidence",
        "recommendation_action",
        "recommendation_reason",
        "confidence_gap",
        "confidence_gap_source",
        "alternatives",
    ]
}

fn explain_field_str<'a>(
    fields: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
    default: &'a str,
) -> &'a str {
    fields.get(key).and_then(|v| v.as_str()).unwrap_or(default)
}

fn build_selected_section(
    fields: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut selected = serde_json::Map::new();
    selected.insert(
        "plugin".to_string(),
        serde_json::Value::String(explain_field_str(fields, "selected_plugin", "none").to_string()),
    );
    if let Some(why) = fields.get("why").and_then(|v| v.as_str()) {
        selected.insert(
            "why".to_string(),
            serde_json::Value::Object(parse_explain_pipe_attributes(why)),
        );
    }
    if let Some(cap) = fields.get("selected_capability").and_then(|v| v.as_str()) {
        selected.insert(
            "capability".to_string(),
            serde_json::Value::Object(parse_capability_line_to_json(cap)),
        );
    }
    if let Some(patterns) = fields
        .get("selected_declared_patterns")
        .and_then(|v| v.as_str())
    {
        selected.insert(
            "declared_patterns".to_string(),
            serde_json::Value::String(patterns.to_string()),
        );
    }
    selected
}

fn build_alternatives_section(
    pairs: &[(String, String)],
    fields: &serde_json::Map<String, serde_json::Value>,
) -> Vec<serde_json::Value> {
    let mut alternatives = Vec::new();
    for (k, v) in pairs {
        if let Some(entry) = build_alternative_entry(k, v, fields) {
            alternatives.push(entry);
        }
    }
    sort_alternative_entries(&mut alternatives);
    alternatives
}

fn build_alternative_entry(
    key: &str,
    raw: &str,
    fields: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    if !is_alternative_rank_entry_key(key) {
        return None;
    }

    let index = parse_explain_alternative_index(key)?;
    let mut alt = parse_explain_pipe_attributes(raw);
    alt.insert(
        "rank".to_string(),
        serde_json::Value::Number(serde_json::Number::from(index as u64)),
    );
    alt.insert(
        "key".to_string(),
        serde_json::Value::String(key.to_string()),
    );
    alt.insert(
        "raw".to_string(),
        serde_json::Value::String(raw.to_string()),
    );

    if let Some(plugin_name) = alt
        .get("primary")
        .and_then(|x| x.as_str())
        .map(ToString::to_string)
    {
        attach_alternative_capability_and_patterns(&mut alt, fields, index);
        alt.insert("plugin".to_string(), serde_json::Value::String(plugin_name));
    }
    Some(serde_json::Value::Object(alt))
}

fn sort_alternative_entries(entries: &mut [serde_json::Value]) {
    entries.sort_by(|a, b| {
        let a_rank = a.get("rank").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
        let b_rank = b.get("rank").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
        a_rank.cmp(&b_rank)
    });
}

fn is_alternative_rank_entry_key(key: &str) -> bool {
    if !key.starts_with("alternative_") || key == "alternatives" {
        return false;
    }
    !key.ends_with("_capability") && !key.ends_with("_declared_patterns")
}

fn attach_alternative_capability_and_patterns(
    alt: &mut serde_json::Map<String, serde_json::Value>,
    fields: &serde_json::Map<String, serde_json::Value>,
    index: usize,
) {
    let cap_key = format!("alternative_{}_capability", index);
    if let Some(cap) = fields.get(&cap_key).and_then(|x| x.as_str()) {
        alt.insert(
            "capability".to_string(),
            serde_json::Value::Object(parse_capability_line_to_json(cap)),
        );
    }
    let patterns_key = format!("alternative_{}_declared_patterns", index);
    if let Some(patterns) = fields.get(&patterns_key).and_then(|x| x.as_str()) {
        alt.insert(
            "declared_patterns".to_string(),
            serde_json::Value::String(patterns.to_string()),
        );
    }
}

fn build_recommendation_section(
    fields: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "primary": explain_field_str(fields, "recommendation_primary", "none"),
        "confidence": explain_field_str(fields, "recommendation_confidence", "unknown"),
        "action": explain_field_str(fields, "recommendation_action", "none"),
        "alternative_1": explain_field_str(fields, "recommendation_alternative_1", "none"),
        "alternative_2": explain_field_str(fields, "recommendation_alternative_2", "none"),
        "confidence_gap": explain_field_str(fields, "confidence_gap", "not_available"),
        "confidence_gap_source": explain_field_str(fields, "confidence_gap_source", "unknown"),
        "reason": explain_field_str(fields, "recommendation_reason", ""),
    })
}

fn build_explain_contract(
    fields: &serde_json::Map<String, serde_json::Value>,
) -> (bool, Vec<serde_json::Value>) {
    let required_fields = explain_required_fields();
    let missing_required_fields = required_fields
        .iter()
        .filter(|key| !fields.contains_key(**key))
        .map(|key| serde_json::Value::String((*key).to_string()))
        .collect::<Vec<_>>();
    (missing_required_fields.is_empty(), missing_required_fields)
}

fn collect_explain_fields(
    report: &str,
) -> (
    Vec<(String, String)>,
    serde_json::Map<String, serde_json::Value>,
) {
    let pairs = parse_explain_report_pairs(report);
    let mut fields = serde_json::Map::new();
    for (k, v) in &pairs {
        fields.insert(k.to_string(), serde_json::Value::String(v.to_string()));
    }
    (pairs, fields)
}

fn build_explain_report_json_value(
    pairs: &[(String, String)],
    fields: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    let selected = build_selected_section(fields);
    let alternatives = build_alternatives_section(pairs, fields);
    let recommendation = build_recommendation_section(fields);
    let (contract_ok, missing_required_fields) = build_explain_contract(fields);
    let required_fields = explain_required_fields();

    serde_json::json!({
        "contract_version": "explain.v1",
        "contract_ok": contract_ok,
        "required_fields": required_fields,
        "missing_required_fields": missing_required_fields,
        "kind": explain_field_str(fields, "plugin_selection", "plugin_selection"),
        "input_kind": explain_field_str(fields, "input_kind", "unknown"),
        "selected_plugin": explain_field_str(fields, "selected_plugin", "none"),
        "fallback_decision": explain_field_str(fields, "fallback_decision", "none"),
        "retry_plugin": explain_field_str(fields, "retry_plugin", "none"),
        "selected": selected,
        "recommendation": recommendation,
        "alternatives": alternatives,
        "fields": fields,
    })
}

fn render_explain_report_json(report: &str) -> Result<String, CliError> {
    let (pairs, fields) = collect_explain_fields(report);
    let value = build_explain_report_json_value(&pairs, &fields);

    serde_json::to_string_pretty(&value).map_err(CliError::Serialization)
}

fn render_explain_report_markdown(report: &str) -> String {
    let pairs = parse_explain_report_pairs(report);
    let mut map = std::collections::BTreeMap::new();
    for (k, v) in pairs {
        map.insert(k, v);
    }

    let mut out = String::new();
    out.push_str("# Plugin Selection\n\n");
    out.push_str(&format!(
        "- input_kind: `{}`\n",
        map.get("input_kind")
            .map(String::as_str)
            .unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "- selected_plugin: `{}`\n",
        map.get("selected_plugin")
            .map(String::as_str)
            .unwrap_or("none")
    ));
    out.push_str(&format!(
        "- fallback_decision: `{}`\n",
        map.get("fallback_decision")
            .map(String::as_str)
            .unwrap_or("none")
    ));
    out.push_str(&format!(
        "- retry_plugin: `{}`\n\n",
        map.get("retry_plugin")
            .map(String::as_str)
            .unwrap_or("none")
    ));

    out.push_str("## Recommendation\n\n");
    out.push_str(&format!(
        "- primary: `{}`\n",
        map.get("recommendation_primary")
            .map(String::as_str)
            .unwrap_or("none")
    ));
    out.push_str(&format!(
        "- confidence: `{}`\n",
        map.get("recommendation_confidence")
            .map(String::as_str)
            .unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "- action: `{}`\n",
        map.get("recommendation_action")
            .map(String::as_str)
            .unwrap_or("none")
    ));
    out.push_str(&format!(
        "- alternative_1: `{}`\n",
        map.get("recommendation_alternative_1")
            .map(String::as_str)
            .unwrap_or("none")
    ));
    out.push_str(&format!(
        "- alternative_2: `{}`\n",
        map.get("recommendation_alternative_2")
            .map(String::as_str)
            .unwrap_or("none")
    ));
    out.push_str(&format!(
        "- confidence_gap: `{}`\n",
        map.get("confidence_gap")
            .map(String::as_str)
            .unwrap_or("not_available")
    ));
    out.push_str(&format!(
        "- confidence_gap_source: `{}`\n",
        map.get("confidence_gap_source")
            .map(String::as_str)
            .unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "- reason: `{}`\n",
        map.get("recommendation_reason")
            .map(String::as_str)
            .unwrap_or("")
    ));

    if let Some(cap) = map.get("selected_capability") {
        out.push_str("\n## Evidence\n\n");
        out.push_str(&format!("- selected_capability: `{}`\n", cap));
        if let Some(patterns) = map.get("selected_declared_patterns") {
            out.push_str(&format!("- selected_declared_patterns: `{}`\n", patterns));
        }
    }

    out.push_str("\n## Alternatives\n\n");
    let mut i = 1usize;
    loop {
        let key = format!("alternative_{}", i);
        if let Some(alt) = map.get(&key) {
            out.push_str(&format!("- {}: `{}`\n", key, alt));
            let cap_key = format!("{}_capability", key);
            if let Some(cap) = map.get(&cap_key) {
                out.push_str(&format!("  - capability: `{}`\n", cap));
            }
            let patterns_key = format!("{}_declared_patterns", key);
            if let Some(patterns) = map.get(&patterns_key) {
                out.push_str(&format!("  - declared_patterns: `{}`\n", patterns));
            }
            i += 1;
            continue;
        }
        break;
    }

    out
}

fn render_explain_report_by_format(
    report: &str,
    format: &OutputFormat,
) -> Result<String, CliError> {
    match format {
        OutputFormat::Text => Ok(report.to_string()),
        OutputFormat::Markdown => Ok(render_explain_report_markdown(report)),
        OutputFormat::Json => render_explain_report_json(report),
    }
}

fn render_capability_evidence_line(prefix: &str, plugin_name: &str, out: &mut String) {
    if let Some(evidence) = load_plugin_capability_evidence(plugin_name) {
        out.push_str(&format!(
            "{}_capability=description:{}|tags:{}|route:{}|samples:{}|showcase:{}|audit:{}|frozen:{}|status:{}\n",
            prefix,
            sanitize_explain_field(&evidence.description),
            sanitize_explain_field(&evidence.tags),
            if evidence.route_group.is_empty() {
                "none"
            } else {
                evidence.route_group.as_str()
            },
            evidence.sample_cases,
            evidence.showcase_cases,
            evidence.audit_cases,
            evidence.frozen_cases,
            if evidence.coverage_status.is_empty() {
                "unknown"
            } else {
                evidence.coverage_status.as_str()
            }
        ));
        if !evidence.detect_patterns.is_empty() {
            out.push_str(&format!(
                "{}_declared_patterns={}\n",
                prefix,
                sanitize_explain_field(&evidence.detect_patterns.join(" ; "))
            ));
        }
    } else {
        out.push_str(&format!("{}_capability=missing_index_entry\n", prefix));
    }
}

fn parse_plugin_explain_command_line(line: &str) -> Option<Vec<String>> {
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
                } else if ch == '\'' {
                    mode = QuoteMode::Single;
                } else if ch == '"' {
                    mode = QuoteMode::Double;
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

    if !matches!(mode, QuoteMode::None) {
        return None;
    }
    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    (!tokens.is_empty()).then_some(tokens)
}

fn explain_recommendation_confidence_for_command(
    route_fallback: bool,
    matched_by: &str,
) -> &'static str {
    if route_fallback {
        "low"
    } else if matches!(matched_by, "command_exact" | "arg_prefix" | "arg_exact") {
        "high"
    } else {
        "medium"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandRouteRecommendation {
    retry_plugin: String,
    fallback_decision: &'static str,
    recommendation_confidence: &'static str,
    recommendation_action: &'static str,
    recommendation_alternative_1: String,
    recommendation_alternative_2: String,
    confidence_gap: String,
    recommendation_reason: String,
}

fn build_command_route_recommendation(
    route: &plugin_config_loader::RunRouteDecision,
    alternatives: &[&plugin_config_loader::RunRouteDecision],
) -> CommandRouteRecommendation {
    let retry_plugin = if route.is_fallback {
        alternatives
            .first()
            .map(|candidate| candidate.plugin_name.as_str())
            .unwrap_or("none")
            .to_string()
    } else {
        "none".to_string()
    };
    let fallback_decision = if route.is_fallback {
        "fallback_selected"
    } else {
        "stable_route"
    };
    let recommendation_confidence =
        explain_recommendation_confidence_for_command(route.is_fallback, &route.matched_by);
    let recommendation_action = if route.is_fallback {
        "review_and_retry"
    } else {
        "accept"
    };
    let recommendation_alternative_1 = alternatives
        .first()
        .map(|candidate| candidate.plugin_name.as_str())
        .unwrap_or("none")
        .to_string();
    let recommendation_alternative_2 = alternatives
        .get(1)
        .map(|candidate| candidate.plugin_name.as_str())
        .unwrap_or("none")
        .to_string();
    let route_priority_gap = match (
        route.priority,
        alternatives.first().and_then(|c| c.priority),
    ) {
        (Some(selected_priority), Some(top_alt_priority)) => {
            i64::from(selected_priority) - i64::from(top_alt_priority)
        }
        _ => 0,
    };
    let confidence_gap = if alternatives.is_empty() {
        "not_applicable".to_string()
    } else {
        route_priority_gap.to_string()
    };
    let recommendation_reason = if route.is_fallback {
        format!(
            "fallback_route_selected|retry_plugin:{}|top_alternative:{}",
            retry_plugin, recommendation_alternative_1
        )
    } else {
        format!(
            "route_match:{}|pattern:{}|intent:{}|priority:{}",
            route.matched_by,
            route.matched_pattern.as_deref().unwrap_or("none"),
            route.intent.as_deref().unwrap_or("none"),
            route
                .priority
                .map(|p| p.to_string())
                .unwrap_or_else(|| "none".to_string())
        )
    };

    CommandRouteRecommendation {
        retry_plugin,
        fallback_decision,
        recommendation_confidence,
        recommendation_action,
        recommendation_alternative_1,
        recommendation_alternative_2,
        confidence_gap,
        recommendation_reason,
    }
}

fn append_command_alternatives_report(
    out: &mut String,
    alternatives: &[&plugin_config_loader::RunRouteDecision],
) {
    for (idx, candidate) in alternatives.iter().enumerate() {
        out.push_str(&format!(
            "alternative_{}={}|group={}|matched_by={}|pattern={}|intent={}|priority={}|fallback={}\n",
            idx + 1,
            candidate.plugin_name,
            candidate.route_group,
            candidate.matched_by,
            candidate.matched_pattern.as_deref().unwrap_or("none"),
            candidate.intent.as_deref().unwrap_or("none"),
            candidate
                .priority
                .map(|p| p.to_string())
                .unwrap_or_else(|| "none".to_string()),
            candidate.is_fallback
        ));
        render_capability_evidence_line(
            &format!("alternative_{}", idx + 1),
            &candidate.plugin_name,
            out,
        );
    }
}

struct CommandExplainContext {
    prog: String,
    cmd_args: Vec<String>,
    route: plugin_config_loader::RunRouteDecision,
    alternatives: Vec<plugin_config_loader::RunRouteDecision>,
    chain: String,
}

fn invalid_command_explain_report() -> String {
    "plugin_selection\ninput_kind=command\nselected_plugin=none\nreason=invalid_command_line\nalternatives=0\n".to_string()
}

fn build_command_explain_context(command_line: &str) -> Option<CommandExplainContext> {
    let tokens = parse_plugin_explain_command_line(command_line)?;
    let prog = tokens[0].clone();
    let cmd_args = tokens.iter().skip(1).cloned().collect::<Vec<_>>();
    let caps = load_run_routes();
    let route = plugin_config_loader::resolve_run_route(&caps, &prog, &cmd_args);
    let route_candidates =
        plugin_config_loader::explain_run_route_candidates(&caps, &prog, &cmd_args);
    let chain = plugins_for_run_command(&prog, &cmd_args)
        .iter()
        .map(|plugin| plugin.name())
        .collect::<Vec<_>>()
        .join(", ");

    let alternatives = route_candidates
        .iter()
        .filter(|candidate| candidate.plugin_name != route.plugin_name)
        .cloned()
        .collect::<Vec<_>>();

    Some(CommandExplainContext {
        prog,
        cmd_args,
        route,
        alternatives,
        chain,
    })
}

fn render_command_plugin_selection_report(
    command_line: &str,
    args: &CliArgs,
    context: &CommandExplainContext,
) -> String {
    let alt_refs = context.alternatives.iter().collect::<Vec<_>>();
    let recommendation = build_command_route_recommendation(&context.route, &alt_refs);

    let mut out = String::new();
    out.push_str("plugin_selection\n");
    out.push_str("input_kind=command\n");
    out.push_str(&format!(
        "command={}\n",
        sanitize_explain_field(&build_run_command_anchor(&context.prog, &context.cmd_args))
    ));
    out.push_str(&format!("selected_plugin={}\n", context.route.plugin_name));
    out.push_str(&format!("route_group={}\n", context.route.route_group));
    out.push_str(&format!(
        "why=command_tool:{} matched_by:{} pattern:{} intent:{} priority:{} fallback:{}\n",
        context.route.command_keyword,
        context.route.matched_by,
        context.route.matched_pattern.as_deref().unwrap_or("none"),
        context.route.intent.as_deref().unwrap_or("none"),
        context
            .route
            .priority
            .map(|p| p.to_string())
            .unwrap_or_else(|| "none".to_string()),
        context.route.is_fallback
    ));
    render_capability_evidence_line("selected", &context.route.plugin_name, &mut out);
    out.push_str(&format!(
        "fallback_decision={}\n",
        recommendation.fallback_decision
    ));
    out.push_str(&format!(
        "top_score_gap={}\n",
        recommendation.confidence_gap
    ));
    out.push_str(&format!(
        "confidence_gap={}\n",
        recommendation.confidence_gap
    ));
    out.push_str("confidence_gap_source=route_priority\n");
    out.push_str(&format!(
        "fallback_threshold={:.3}\n",
        args.explain_fallback_gap
    ));
    out.push_str(&format!("retry_plugin={}\n", recommendation.retry_plugin));
    out.push_str(&format!(
        "recommendation_primary={}\n",
        context.route.plugin_name
    ));
    out.push_str(&format!(
        "recommendation_confidence={}\n",
        recommendation.recommendation_confidence
    ));
    out.push_str(&format!(
        "recommendation_action={}\n",
        recommendation.recommendation_action
    ));
    out.push_str(&format!(
        "recommendation_alternative_1={}\n",
        recommendation.recommendation_alternative_1
    ));
    out.push_str(&format!(
        "recommendation_alternative_2={}\n",
        recommendation.recommendation_alternative_2
    ));
    out.push_str(&format!(
        "recommendation_reason={}\n",
        sanitize_explain_field(&recommendation.recommendation_reason)
    ));
    out.push_str(&format!("alternatives={}\n", context.alternatives.len()));
    append_command_alternatives_report(&mut out, &alt_refs);
    out.push_str(&format!(
        "candidate_plugin_chain={}\n",
        sanitize_explain_field(&context.chain)
    ));
    out.push_str(&format!(
        "run_route_view=available_with:tokenslim run --explain-route {}\n",
        sanitize_explain_field(command_line)
    ));
    out.push_str(&format!(
        "output_format={}\n",
        match args.output_format {
            OutputFormat::Json => "json",
            OutputFormat::Markdown => "markdown",
            OutputFormat::Text => "text",
        }
    ));
    out.push_str("replay_case_template=available_with:--explain-replay-out <path>\n");
    out
}

fn explain_plugin_for_command_line(command_line: &str, args: &CliArgs) -> String {
    let Some(context) = build_command_explain_context(command_line) else {
        return invalid_command_explain_report();
    };
    render_command_plugin_selection_report(command_line, args, &context)
}

fn is_retryable_explain_plugin(name: &str) -> bool {
    !matches!(
        name,
        "ansi_cleaner"
            | "generic_text"
            | "noise_filter"
            | "smart_code"
            | "smart_path"
            | "static_rule"
            | "template_driven"
    )
}

#[derive(Debug, Clone, PartialEq)]
struct LogExplainRecommendation {
    selected: (String, u8, f32),
    alternatives: Vec<(String, u8, f32)>,
    top_score_gap: f32,
    retry_score_gap: f32,
    fallback_decision: &'static str,
    retry_plugin: String,
    recommendation_confidence: &'static str,
    recommendation_action: &'static str,
    recommendation_alternative_1: String,
    recommendation_alternative_2: String,
    recommendation_reason: String,
    fallback_note: Option<String>,
}

fn collect_log_detections(slice: &Slice<'_>) -> Vec<(String, u8, f32)> {
    let mut detections = get_plugins()
        .into_iter()
        .filter_map(|plugin| {
            plugin
                .detect(slice)
                .filter(|score| *score > 0.1)
                .map(|score| (plugin.name().to_string(), plugin.priority(), score))
        })
        .collect::<Vec<_>>();
    detections.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.0.cmp(&b.0))
    });
    detections
}

fn build_log_explain_recommendation(
    detections: &[(String, u8, f32)],
    fallback_gap_threshold: f32,
) -> LogExplainRecommendation {
    let selected = detections
        .first()
        .cloned()
        .unwrap_or_else(|| ("generic_text".to_string(), 255, 0.0));
    let alternatives = detections
        .iter()
        .skip(1)
        .take(7)
        .cloned()
        .collect::<Vec<_>>();
    let top_score_gap = alternatives
        .first()
        .map(|(_, _, score)| selected.2 - *score)
        .unwrap_or(selected.2);
    let retry_candidate = alternatives
        .iter()
        .find(|(name, _, _)| is_retryable_explain_plugin(name));
    let retry_score_gap = retry_candidate
        .map(|(_, _, score)| selected.2 - *score)
        .unwrap_or(selected.2);
    let fallback_decision = if detections.is_empty() {
        "fallback_selected"
    } else if retry_candidate.is_some() && retry_score_gap < fallback_gap_threshold {
        "review_recommended"
    } else {
        "stable_detector"
    };
    let retry_plugin = if fallback_decision == "review_recommended" {
        retry_candidate
            .map(|(name, _, _)| name.as_str())
            .unwrap_or("none")
            .to_string()
    } else {
        "none".to_string()
    };
    let recommendation_confidence = if detections.is_empty() {
        "low"
    } else if fallback_decision == "review_recommended" {
        "medium"
    } else if top_score_gap >= fallback_gap_threshold {
        "high"
    } else {
        "medium"
    };
    let recommendation_action = if detections.is_empty() {
        "review_generic_fallback"
    } else if fallback_decision == "review_recommended" {
        "review_and_retry"
    } else {
        "accept"
    };
    let recommendation_alternative_1 = alternatives
        .first()
        .map(|(name, _, _)| name.as_str())
        .unwrap_or("none")
        .to_string();
    let recommendation_alternative_2 = alternatives
        .get(1)
        .map(|(name, _, _)| name.as_str())
        .unwrap_or("none")
        .to_string();
    let recommendation_reason = if detections.is_empty() {
        "no_detector_above_threshold".to_string()
    } else if fallback_decision == "review_recommended" {
        format!(
            "close_competitor|retry_plugin:{}|retry_score_gap:{:.3}|threshold:{:.3}",
            retry_plugin, retry_score_gap, fallback_gap_threshold
        )
    } else {
        format!(
            "detector_stable|selected_score:{:.3}|top_score_gap:{:.3}|threshold:{:.3}",
            selected.2, top_score_gap, fallback_gap_threshold
        )
    };
    let fallback_note = if fallback_decision == "stable_detector" {
        alternatives.first().and_then(|(name, _, _)| {
            if !is_retryable_explain_plugin(name) && top_score_gap < fallback_gap_threshold {
                Some(format!("nearest_candidate_non_retryable:{}", name))
            } else {
                None
            }
        })
    } else {
        None
    };

    LogExplainRecommendation {
        selected,
        alternatives,
        top_score_gap,
        retry_score_gap,
        fallback_decision,
        retry_plugin,
        recommendation_confidence,
        recommendation_action,
        recommendation_alternative_1,
        recommendation_alternative_2,
        recommendation_reason,
        fallback_note,
    }
}

fn explain_plugin_for_log_text(text: &str, fallback_gap_threshold: f32) -> String {
    let slice = Slice {
        id: 1,
        text: Cow::Borrowed(text),
        slice_type: SliceType::LogBlock,
        offset: 0,
        line_start: 1,
        line_end: text.lines().count().max(1),
        file_metadata: None,
        flags: SliceFlags::default(),
    };

    let detections = collect_log_detections(&slice);
    let recommendation = build_log_explain_recommendation(&detections, fallback_gap_threshold);

    let mut out = String::new();
    out.push_str("plugin_selection\n");
    out.push_str("input_kind=log\n");
    out.push_str(&format!("line_count={}\n", text.lines().count()));
    out.push_str(&format!("byte_count={}\n", text.len()));
    out.push_str(&format!("selected_plugin={}\n", recommendation.selected.0));
    out.push_str(&format!(
        "why=content_detector_score:{:.3}|plugin_priority:{}|candidate_rank:1\n",
        recommendation.selected.2, recommendation.selected.1
    ));
    render_capability_evidence_line("selected", &recommendation.selected.0, &mut out);
    out.push_str(&format!(
        "fallback_decision={}\n",
        recommendation.fallback_decision
    ));
    out.push_str(&format!(
        "top_score_gap={:.3}\n",
        recommendation.top_score_gap
    ));
    out.push_str(&format!(
        "confidence_gap={:.3}\n",
        recommendation.top_score_gap
    ));
    out.push_str("confidence_gap_source=detector_score\n");
    out.push_str(&format!(
        "retry_score_gap={:.3}\n",
        recommendation.retry_score_gap
    ));
    out.push_str(&format!(
        "fallback_threshold={:.3}\n",
        fallback_gap_threshold
    ));
    out.push_str(&format!("retry_plugin={}\n", recommendation.retry_plugin));
    out.push_str(&format!(
        "recommendation_primary={}\n",
        recommendation.selected.0
    ));
    out.push_str(&format!(
        "recommendation_confidence={}\n",
        recommendation.recommendation_confidence
    ));
    out.push_str(&format!(
        "recommendation_action={}\n",
        recommendation.recommendation_action
    ));
    out.push_str(&format!(
        "recommendation_alternative_1={}\n",
        recommendation.recommendation_alternative_1
    ));
    out.push_str(&format!(
        "recommendation_alternative_2={}\n",
        recommendation.recommendation_alternative_2
    ));
    out.push_str(&format!(
        "recommendation_reason={}\n",
        sanitize_explain_field(&recommendation.recommendation_reason)
    ));
    if let Some(note) = recommendation.fallback_note.as_deref() {
        out.push_str(&format!("fallback_note={}\n", note));
    }
    out.push_str(&format!(
        "alternatives={}\n",
        recommendation.alternatives.len()
    ));
    for (idx, (name, priority, score)) in recommendation.alternatives.iter().enumerate() {
        out.push_str(&format!(
            "alternative_{}={}|score={:.3}|priority={}\n",
            idx + 1,
            name,
            score,
            priority
        ));
        render_capability_evidence_line(&format!("alternative_{}", idx + 1), name, &mut out);
    }
    if detections.is_empty() {
        out.push_str("fallback_reason=no_plugin_detector_above_threshold\n");
    }
    out.push_str("replay_case_template=available_with:--explain-replay-out <path>\n");
    out
}

fn write_explain_replay_template(
    path: &std::path::Path,
    input_kind: &str,
    replay_input: &str,
    report: &str,
) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(CliError::Io)?;
        }
    }

    let replay_command = if input_kind == "command" {
        format!(
            "tokenslim explain-plugin --explain-command \"{}\"",
            replay_input.replace('"', "\\\"")
        )
    } else {
        "tokenslim explain-plugin --input <log_file>".to_string()
    };

    let template = format!(
        "# Route Misclassification Replay Case\n\n\
status: todo\n\
input_kind: {input_kind}\n\
expected_plugin: <fill_when_known>\n\
observed_plugin: <copy_from_explain_output>\n\
retry_plugin: <copy_from_retry_plugin>\n\
recommendation_confidence: <copy_from_recommendation_confidence>\n\
recommendation_action: <copy_from_recommendation_action>\n\
decision: pass | needs_route_fix | needs_detector_fix | waived\n\n\
## Replay Command\n\n```powershell\n{replay_command}\n```\n\n\
## Input\n\n```text\n{replay_input}\n```\n\n\
## Explain Output\n\n```text\n{report}\n```\n\n\
## Audit Notes\n\n\
- Confirm whether `selected_plugin` is correct for this input.\n\
- Inspect `recommendation_primary/recommendation_confidence/recommendation_action/recommendation_reason` before deciding route vs detector fix.\n\
- If `fallback_decision=review_recommended`, replay with the `retry_plugin` parser path or add a focused sample case.\n\
- If this is a real misroute, create or update the plugin's sample/showcase/audit case before freezing.\n"
    );

    std::fs::write(path, template).map_err(CliError::Io)
}

fn read_explain_input_text(input: &InputSource) -> Result<String, CliError> {
    match input {
        InputSource::File(path) => {
            let bytes = std::fs::read(path).map_err(CliError::Io)?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
        InputSource::Stdin => {
            if std::io::stdin().is_terminal() {
                return Err(CliError::InvalidArgs(
                    crate::utils::i18n::t("err_explain_plugin_requires_input").to_string(),
                ));
            }
            let mut buffer = Vec::new();
            io::stdin().read_to_end(&mut buffer).map_err(CliError::Io)?;
            Ok(String::from_utf8_lossy(&buffer).into_owned())
        }
    }
}

fn get_vcs_intent(prog: &str, args: &[String]) -> Option<VcsRunIntent> {
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
fn detect_vcs_run_intent(prog: &str, cmd_args: &[String]) -> Option<VcsRunIntent> {
    get_vcs_intent(prog, cmd_args)
}

fn count_paths_footer_lines(text: &str) -> usize {
    text.lines()
        .filter(|line| line.starts_with("paths: ") || line.starts_with("[paths]"))
        .count()
}

fn parse_path_dictionary_blocks(text: &str) -> (Vec<(String, String)>, String) {
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

fn sort_path_entries_by_token(entries: &mut [(String, String)]) {
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

fn render_path_dictionary_line(entries: &[(String, String)]) -> String {
    let parts: Vec<String> = entries
        .iter()
        .map(|(t, p)| format!("{}={}", t, p))
        .collect();
    format!("[paths] {}", parts.join("; "))
}

fn place_path_dictionary_line(body_text: &str, merged_dict: &str) -> String {
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

fn rebuild_with_extended_subdir_entries(
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

fn normalize_path_dictionary_entries(entries: &mut Vec<(String, String)>) {
    // 排序：按 token 编号
    sort_path_entries_by_token(entries.as_mut_slice());
    // 为 3+ 子路径的公共前缀创建父级词典条目
    add_common_parent_entries(entries);
    // 父前缀别名（必须在 add_common_parent_entries 之后）
    apply_parent_prefix_aliases_cli(entries);
}

fn rewrite_body_with_path_dictionary(
    entries: &[(String, String)],
    body_text: &str,
) -> (String, Option<String>) {
    // 用合并后的字典替换所有原始路径为 $P 令牌
    let rewritten_body = replace_paths_with_dict(body_text, entries);
    let extended = rebuild_with_extended_subdir_entries(entries, &rewritten_body);
    (rewritten_body, extended)
}

/// 合并多个 [paths] / paths: 字典块为单一块（置顶），并全文本替换路径
fn merge_path_dictionary_blocks(text: &str) -> String {
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
fn replace_paths_with_dict(text: &str, entries: &[(String, String)]) -> String {
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
fn add_subdir_entries_from_text(entries: &mut Vec<(String, String)>, text: &str) -> bool {
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

fn add_common_parent_entries(entries: &mut Vec<(String, String)>) {
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

fn collect_existing_paths(entries: &[(String, String)]) -> std::collections::HashSet<String> {
    entries.iter().map(|(_, p)| p.clone()).collect()
}

fn collect_parent_prefix_counts(
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

fn next_path_token_id(entries: &[(String, String)]) -> usize {
    entries
        .iter()
        .filter_map(|(t, _)| t.strip_prefix("$P").and_then(|s| s.parse::<usize>().ok()))
        .max()
        .unwrap_or(0)
        + 1
}

fn append_common_parent_entries(
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
fn apply_parent_prefix_aliases_cli(entries: &mut Vec<(String, String)>) {
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
fn strip_duplicate_vcs_headers(text: &str) -> String {
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

fn token_key_as_num(token: &str) -> usize {
    token
        .strip_prefix("$P")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(usize::MAX)
}

fn collect_sorted_output_path_entries(
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

fn partition_path_entries_by_min_uses(
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

fn render_paths_footer_line(entries: &[(String, String)]) -> Option<String> {
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

fn should_skip_paths_footer_append(
    formatted: &str,
    output: &crate::core::compression::CompressionOutput,
) -> bool {
    count_paths_footer_lines(formatted) > 0
        || !formatted.contains("$P")
        || output.dictionary.paths.is_empty()
}

fn rewrite_dropped_path_tokens(formatted: &str, drop_entries: &[(String, String)]) -> String {
    let mut rewritten = formatted.to_string();
    for (token, path) in drop_entries {
        rewritten = replace_path_token_boundary(&rewritten, token, path);
    }
    rewritten
}

fn append_paths_footer_line(mut rewritten: String, footer: &str) -> String {
    if !rewritten.ends_with('\n') {
        rewritten.push('\n');
    }
    rewritten.push_str(footer);
    rewritten
}

fn append_paths_footer_from_output_dictionary(
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

fn count_path_token_uses(text: &str, token: &str) -> usize {
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

fn should_enable_vcs_ai_compact(
    vcs_intent: Option<VcsRunIntent>,
    output_format: OutputFormat,
    preset: Option<Preset>,
) -> bool {
    vcs_intent.is_some()
        && preset.is_some()
        && matches!(output_format, OutputFormat::Text | OutputFormat::Markdown)
}

fn should_apply_final_paths_optimizer(
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

fn is_verify_fixture_file(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .as_deref(),
        Some("log") | Some("fixture") | Some("input")
    )
}

fn expected_file_for_fixture(
    expected_dir: &std::path::Path,
    fixture_file: &std::path::Path,
) -> Option<std::path::PathBuf> {
    let stem = fixture_file.file_stem()?.to_string_lossy();
    let mapped = if let Some(prefix) = stem.strip_suffix("_fixture") {
        expected_dir.join(format!("{}_expected.txt", prefix))
    } else {
        expected_dir.join(format!("{}.expected", stem))
    };

    if mapped.exists() {
        return Some(mapped);
    }

    let file_name = fixture_file.file_name()?.to_string_lossy();
    let fallback = expected_dir.join(file_name.as_ref());
    if fallback.exists() {
        Some(fallback)
    } else {
        None
    }
}

fn collect_verify_fixture_files(
    fixture_dir: &std::path::Path,
) -> Result<Vec<std::path::PathBuf>, CliError> {
    let mut entries = std::fs::read_dir(fixture_dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_verify_fixture_file(p))
        .collect::<Vec<_>>();
    entries.sort();
    Ok(entries)
}

fn verify_single_fixture_with_plugin(
    plugin: &crate::plugins::static_rule_plugin::SimpleRulePlugin,
    fixture_text: String,
    expected_text: String,
    safety: bool,
) -> Result<(), CliError> {
    let slice = Slice {
        id: 1,
        text: Cow::Owned(fixture_text.clone()),
        slice_type: SliceType::LogBlock,
        offset: 0,
        line_start: 1,
        line_end: fixture_text.lines().count().max(1),
        file_metadata: None,
        flags: Default::default(),
    };

    let mut dict = DictionaryEngine::new();
    let mut dedup = DedupEngine::new(DedupConfig::default());
    let arena = Bump::new();
    let result = plugin.compress(&slice, &mut dict, &mut dedup, &arena);
    let actual = flatten_tokens(&result.tokens);
    if safety {
        let mut warnings = Vec::new();
        for check in crate::core::safety_check::ALL_CHECKS {
            warnings.extend(check.check_output(&fixture_text, &actual));
        }
        if !warnings.is_empty() {
            let details = warnings
                .into_iter()
                .map(|w| format!("[{}] {}", w.check, w.message))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(CliError::InvalidArgs(format!(
                "[verify][safety] 输出检测到风险:\n{}",
                details
            )));
        }
    }
    verify_text_pair(&actual, &expected_text).map_err(CliError::InvalidArgs)
}

fn load_verify_plugin(
    rule_path: &std::path::Path,
    safety: bool,
) -> Result<crate::plugins::static_rule_plugin::SimpleRulePlugin, CliError> {
    use crate::plugins::static_rule_plugin::SimpleRulePlugin;

    let toml_text = std::fs::read_to_string(rule_path)?;
    if safety {
        let warnings = crate::core::safety_check::run_safety_checks_on_config(&toml_text);
        if !warnings.is_empty() {
            let details = warnings
                .into_iter()
                .map(|w| format!("[{}] {}", w.check, w.message))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(CliError::InvalidArgs(format!(
                "[verify][safety] 检测到风险配置:\n{}",
                details
            )));
        }
    }

    SimpleRulePlugin::from_toml(&toml_text).map_err(CliError::Config)
}

fn run_static_rule_verify_file_mode(
    plugin: &crate::plugins::static_rule_plugin::SimpleRulePlugin,
    rule_path: &std::path::Path,
    fixture_path: &std::path::Path,
    expected_path: &std::path::Path,
    safety: bool,
) -> Result<(), CliError> {
    let fixture = std::fs::read_to_string(fixture_path)?;
    let expected = std::fs::read_to_string(expected_path)?;
    verify_single_fixture_with_plugin(plugin, fixture, expected, safety)?;
    println!(
        "{}",
        t1(
            "verify_pass_rule",
            rule_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("<rule>")
        )
    );
    Ok(())
}

fn run_static_rule_verify_directory_mode(
    plugin: &crate::plugins::static_rule_plugin::SimpleRulePlugin,
    fixture_path: &std::path::Path,
    expected_path: &std::path::Path,
    safety: bool,
) -> Result<(), CliError> {
    let entries = collect_verify_fixture_files(fixture_path)?;

    if entries.is_empty() {
        return Err(CliError::InvalidArgs(t1(
            "verify_no_fixture_found",
            fixture_path.display(),
        )));
    }

    let mut pass = 0usize;
    let mut fail_msgs = Vec::new();
    for fixture_file in entries {
        let file_name = fixture_file
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| fixture_file.display().to_string());
        let Some(expected_file) = expected_file_for_fixture(expected_path, &fixture_file) else {
            fail_msgs.push(format!(
                "missing expected file for {}",
                fixture_file.display()
            ));
            continue;
        };

        let fixture = std::fs::read_to_string(&fixture_file)?;
        let expected = std::fs::read_to_string(&expected_file)?;
        match verify_single_fixture_with_plugin(plugin, fixture, expected, safety) {
            Ok(()) => {
                pass += 1;
                println!("{}", t1("verify_pass_rule", file_name));
            }
            Err(err) => {
                fail_msgs.push(format!("{} => {}", file_name, err));
            }
        }
    }

    if fail_msgs.is_empty() {
        println!("{}", t1("verify_pass_fixture_total", pass));
        return Ok(());
    }

    Err(CliError::InvalidArgs(format!(
        "{}\n{}",
        t2("verify_failed_summary_brief", pass, fail_msgs.len()),
        fail_msgs.join("\n")
    )))
}

fn run_static_rule_verify(
    rule_path: &std::path::Path,
    fixture_path: &std::path::Path,
    expected_path: &std::path::Path,
    safety: bool,
) -> Result<(), CliError> {
    let plugin = load_verify_plugin(rule_path, safety)?;

    if fixture_path.is_file() && expected_path.is_file() {
        return run_static_rule_verify_file_mode(
            &plugin,
            rule_path,
            fixture_path,
            expected_path,
            safety,
        );
    }

    if fixture_path.is_dir() && expected_path.is_dir() {
        return run_static_rule_verify_directory_mode(&plugin, fixture_path, expected_path, safety);
    }

    Err(CliError::InvalidArgs(
        t("verify_must_match_path_types").to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn base_cli_args(mode: CliMode) -> CliArgs {
        CliArgs {
            mode,
            input: InputSource::Stdin,
            output: OutputTarget::Stdout,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            ai_export: false,
            ai_signal: false,
            output_format: OutputFormat::Text,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: DoctorOutputFormat::Text,
            doctor_strict: false,
            inject: false,
            config: None,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }

    fn make_temp_test_dir(prefix: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "tokenslim_cli_methods_{}_{}_{}",
            prefix,
            std::process::id(),
            nonce
        ));
        std::fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    #[test]
    fn renders_quick_usage_with_program_name() {
        let usage = render_global_usage("tokenslim.exe");
        assert!(usage.contains("tokenslim"));
        assert!(usage.contains("Usage:"));
        assert!(usage.contains("tokenslim.exe run <command>"));
        assert!(usage.contains("shorthand for"));
        assert!(usage.contains("--help"));
    }

    #[test]
    fn should_show_quick_usage_for_help_flag() {
        let argv = vec!["tokenslim.exe".to_string(), "--help".to_string()];
        assert!(should_show_quick_usage(&argv, true));
    }

    #[test]
    fn should_show_quick_usage_for_empty_interactive() {
        let argv = vec!["tokenslim.exe".to_string()];
        assert!(should_show_quick_usage(&argv, true));
        assert!(!should_show_quick_usage(&argv, false));
    }

    #[test]
    fn verify_fixture_file_extension_filter_works() {
        assert!(is_verify_fixture_file(std::path::Path::new("a.log")));
        assert!(is_verify_fixture_file(std::path::Path::new("a.fixture")));
        assert!(is_verify_fixture_file(std::path::Path::new("a.input")));
        assert!(!is_verify_fixture_file(std::path::Path::new("a.txt")));
    }

    #[test]
    fn expected_file_for_fixture_prefers_mapped_then_fallback() {
        let fixture_dir = make_temp_test_dir("fixture_map");
        let expected_dir = make_temp_test_dir("expected_map");
        let fixture_file = fixture_dir.join("case_001_fixture.log");
        std::fs::write(&fixture_file, "fixture").expect("write fixture");

        let mapped_expected = expected_dir.join("case_001_expected.txt");
        std::fs::write(&mapped_expected, "expected mapped").expect("write expected");
        let mapped = expected_file_for_fixture(&expected_dir, &fixture_file);
        assert_eq!(mapped.as_deref(), Some(mapped_expected.as_path()));

        std::fs::remove_file(&mapped_expected).expect("remove mapped");
        let fallback_expected = expected_dir.join("case_001_fixture.log");
        std::fs::write(&fallback_expected, "expected fallback").expect("write fallback");
        let fallback = expected_file_for_fixture(&expected_dir, &fixture_file);
        assert_eq!(fallback.as_deref(), Some(fallback_expected.as_path()));

        std::fs::remove_dir_all(&fixture_dir).ok();
        std::fs::remove_dir_all(&expected_dir).ok();
    }

    #[test]
    fn collect_verify_fixture_files_filters_and_sorts() {
        let fixture_dir = make_temp_test_dir("fixture_collect");
        let a = fixture_dir.join("b_case.log");
        let b = fixture_dir.join("a_case.fixture");
        let c = fixture_dir.join("ignore.txt");
        std::fs::write(&a, "1").expect("write a");
        std::fs::write(&b, "2").expect("write b");
        std::fs::write(&c, "3").expect("write c");

        let files = collect_verify_fixture_files(&fixture_dir).expect("collect fixtures");
        let names: Vec<String> = files
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .collect();
        assert_eq!(
            names,
            vec!["a_case.fixture".to_string(), "b_case.log".to_string()]
        );

        std::fs::remove_dir_all(&fixture_dir).ok();
    }

    #[test]
    fn run_static_rule_verify_file_mode_passes_with_simple_rule() {
        let temp_dir = make_temp_test_dir("verify_single_mode");
        let rule_path = temp_dir.join("rule.toml");
        let fixture_path = temp_dir.join("fixture.log");
        let expected_path = temp_dir.join("expected.txt");

        let rule = r#"
[[sections]]
name = "ERR"
enter = "^BEGIN$"
exit = "^END$"
keep = ["^ERR:"]
"#;

        std::fs::write(&rule_path, rule).expect("write rule");
        std::fs::write(&fixture_path, "BEGIN\nERR: boom\nEND\n").expect("write fixture");
        std::fs::write(&expected_path, "[ERR] ERR: boom").expect("write expected");

        run_static_rule_verify(&rule_path, &fixture_path, &expected_path, false)
            .expect("verify should pass");

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn run_static_rule_verify_directory_mode_reports_missing_expected() {
        let temp_dir = make_temp_test_dir("verify_dir_missing_expected");
        let fixture_dir = temp_dir.join("fixtures");
        let expected_dir = temp_dir.join("expected");
        let rule_path = temp_dir.join("rule.toml");
        std::fs::create_dir_all(&fixture_dir).expect("create fixture dir");
        std::fs::create_dir_all(&expected_dir).expect("create expected dir");

        let rule = r#"
[[sections]]
name = "ERR"
enter = "^BEGIN$"
exit = "^END$"
keep = ["^ERR:"]
"#;
        std::fs::write(&rule_path, rule).expect("write rule");
        std::fs::write(fixture_dir.join("case_001.log"), "BEGIN\nERR: boom\nEND\n")
            .expect("write fixture");

        let err = run_static_rule_verify(&rule_path, &fixture_dir, &expected_dir, false)
            .expect_err("should report missing expected");
        let msg = match err {
            CliError::InvalidArgs(s) => s,
            other => panic!("expected invalid args, got {other:?}"),
        };
        assert!(msg.contains("missing expected file for"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn validate_repair_file_request_rejects_backup_without_inplace() {
        let temp_dir = make_temp_test_dir("repair_validate_backup");
        let file = temp_dir.join("a.log");
        std::fs::write(&file, "x").expect("write file");
        let mut args = base_cli_args(CliMode::RepairFile);
        args.input = InputSource::File(file.clone());
        args.backup = true;
        args.inplace = false;
        let err = validate_repair_file_request(&file, &args).expect_err("should reject");
        let msg = match err {
            CliError::InvalidArgs(s) => s,
            other => panic!("expected invalid args, got {other:?}"),
        };
        assert!(msg.contains("--backup requires --inplace"));
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn validate_repair_file_request_rejects_directory_without_inplace() {
        let temp_dir = make_temp_test_dir("repair_validate_dir");
        let mut args = base_cli_args(CliMode::RepairFile);
        args.input = InputSource::File(temp_dir.clone());
        args.inplace = false;
        let err = validate_repair_file_request(&temp_dir, &args).expect_err("should reject");
        let msg = match err {
            CliError::InvalidArgs(s) => s,
            other => panic!("expected invalid args, got {other:?}"),
        };
        assert!(msg.contains("directory mode requires --inplace"));
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn validate_repair_file_request_rejects_directory_output_file_mode() {
        let temp_dir = make_temp_test_dir("repair_validate_dir_output");
        let mut args = base_cli_args(CliMode::RepairFile);
        args.input = InputSource::File(temp_dir.clone());
        args.inplace = true;
        args.output = OutputTarget::File(temp_dir.join("out.txt"));
        let err = validate_repair_file_request(&temp_dir, &args).expect_err("should reject");
        let msg = match err {
            CliError::InvalidArgs(s) => s,
            other => panic!("expected invalid args, got {other:?}"),
        };
        assert!(msg.contains("directory mode does not support --output file"));
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn resolve_single_repair_target_follows_inplace_and_output() {
        let temp_dir = make_temp_test_dir("repair_target");
        let input = temp_dir.join("in.log");
        let output = temp_dir.join("out.log");
        std::fs::write(&input, "x").expect("write input");

        let mut args = base_cli_args(CliMode::RepairFile);
        args.inplace = true;
        args.output = OutputTarget::File(output.clone());
        assert_eq!(
            resolve_single_repair_target(&input, &args),
            Some(input.as_path())
        );

        args.inplace = false;
        assert_eq!(
            resolve_single_repair_target(&input, &args),
            Some(output.as_path())
        );

        args.output = OutputTarget::Stdout;
        assert_eq!(resolve_single_repair_target(&input, &args), None);
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn should_skip_single_repair_by_filters_respects_include_exclude() {
        let temp_dir = make_temp_test_dir("repair_single_filters");
        let input = temp_dir.join("app.log");
        std::fs::write(&input, "x").expect("write input");

        let mut args = base_cli_args(CliMode::RepairFile);
        args.include = vec!["*.txt".to_string()];
        assert!(should_skip_single_repair_by_filters(&args, &input));

        args.include = vec!["*.log".to_string()];
        args.exclude = vec!["app.*".to_string()];
        assert!(should_skip_single_repair_by_filters(&args, &input));

        args.exclude.clear();
        assert!(!should_skip_single_repair_by_filters(&args, &input));
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn build_single_mode_json_report_counts_skipped_and_changed() {
        let temp_dir = make_temp_test_dir("repair_single_report");
        let input = temp_dir.join("bad.log");
        std::fs::write(&input, "x").expect("write input");
        let args = base_cli_args(CliMode::RepairFile);

        let skipped = build_include_exclude_skipped_outcome(&input);
        let skipped_report = build_single_mode_json_report(&args, &input, &skipped, None);
        assert_eq!(skipped_report.summary.skipped, 1);
        assert_eq!(skipped_report.summary.changed, 0);
        assert_eq!(skipped_report.summary.unchanged, 0);

        let changed = RepairOutcome {
            path: input.clone(),
            detected_enc: "gbk".to_string(),
            confidence: "high".to_string(),
            strategy: "reencode_recover_high".to_string(),
            repair_chain: "decode_with_fallback".to_string(),
            steps: vec!["decode".to_string()],
            evidence_items: vec!["encoding=gbk".to_string()],
            evidence: "encoding=gbk".to_string(),
            changed: true,
            skipped: false,
            reason: "repaired".to_string(),
        };
        let changed_report = build_single_mode_json_report(&args, &input, &changed, None);
        assert_eq!(changed_report.summary.changed, 1);
        assert_eq!(changed_report.summary.unchanged, 0);
        assert_eq!(changed_report.summary.skipped, 0);
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn should_show_compress_quick_usage_only_for_empty_stdin_without_args() {
        assert!(should_show_compress_quick_usage(true, true, " \n\t "));
        assert!(!should_show_compress_quick_usage(false, true, ""));
        assert!(!should_show_compress_quick_usage(true, false, ""));
        assert!(!should_show_compress_quick_usage(true, true, "git status"));
    }

    #[test]
    fn read_compress_input_reads_file_and_marks_non_stdin() {
        let temp_dir = make_temp_test_dir("compress_input_file");
        let input_path = temp_dir.join("input.log");
        std::fs::write(&input_path, "hello\nworld").expect("write input file");
        let (text, is_stdin) =
            read_compress_input(&InputSource::File(input_path)).expect("read compress input");
        assert_eq!(text, "hello\nworld");
        assert!(!is_stdin);
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn select_pre_pipeline_action_prioritizes_inject() {
        let mut args = base_cli_args(CliMode::Compress);
        args.inject = true;
        args.doctor = Some(DoctorKind::Encoding);
        assert_eq!(select_pre_pipeline_action(&args), PrePipelineAction::Inject);
    }

    #[test]
    fn select_pre_pipeline_action_detects_verify_rule() {
        let mut args = base_cli_args(CliMode::Compress);
        args.verify_rule = Some("rule.toml".into());
        args.verify_fixture = Some("fixture.log".into());
        args.verify_expected = Some("expected.txt".into());
        assert_eq!(
            select_pre_pipeline_action(&args),
            PrePipelineAction::VerifyRule
        );
    }

    #[test]
    fn select_pre_pipeline_action_detects_repair_file_mode() {
        let args = base_cli_args(CliMode::RepairFile);
        assert_eq!(
            select_pre_pipeline_action(&args),
            PrePipelineAction::RepairFile
        );
    }

    #[test]
    fn select_pre_pipeline_action_continue_for_pipeline_modes() {
        let args = base_cli_args(CliMode::Compress);
        assert_eq!(
            select_pre_pipeline_action(&args),
            PrePipelineAction::Continue
        );
    }

    #[test]
    fn split_run_explain_route_flag_extracts_flag() {
        let run_cmd = vec![
            "--explain-route".to_string(),
            "git".to_string(),
            "status".to_string(),
        ];
        let (flag, remain) = split_run_explain_route_flag(run_cmd);
        assert!(flag);
        assert_eq!(remain, vec!["git".to_string(), "status".to_string()]);
    }

    #[test]
    fn build_run_mode_args_applies_run_defaults() {
        let args = build_run_mode_args(vec!["git".to_string(), "status".to_string()], true);
        assert!(matches!(args.mode, CliMode::Run));
        assert_eq!(
            args.run_command,
            vec!["git".to_string(), "status".to_string()]
        );
        assert!(args.ai_export);
        assert!(args.ai_signal);
        assert!(args.explain_route);
        assert!(matches!(args.output_format, OutputFormat::Text));
        assert_eq!(args.preset, Some(Preset::Ai));
    }

    #[test]
    fn parse_args_from_argv_maps_compress_alias_command() {
        let argv = vec![
            "tokenslim.exe".to_string(),
            "compress".to_string(),
            "--format".to_string(),
            "text".to_string(),
        ];
        let parsed = parse_args_from_argv(&argv).expect("compress alias should parse");
        assert!(matches!(parsed.mode, CliMode::Compress));
        assert!(matches!(parsed.output_format, OutputFormat::Text));
    }

    #[test]
    fn parse_args_from_argv_maps_run_subcommand_and_explain_route() {
        let argv = vec![
            "tokenslim.exe".to_string(),
            "run".to_string(),
            "--explain-route".to_string(),
            "git".to_string(),
            "status".to_string(),
        ];
        let parsed = parse_args_from_argv(&argv).expect("run args should parse");
        assert!(matches!(parsed.mode, CliMode::Run));
        assert!(parsed.explain_route);
        assert_eq!(
            parsed.run_command,
            vec!["git".to_string(), "status".to_string()]
        );
    }

    #[test]
    fn resolve_run_path_preset_maps_cli_preset() {
        assert!(matches!(
            resolve_run_path_preset(Some(Preset::Fast)),
            crate::core::path_optimizer::methods::PathDictionaryPreset::Conservative
        ));
        assert!(matches!(
            resolve_run_path_preset(Some(Preset::Balanced)),
            crate::core::path_optimizer::methods::PathDictionaryPreset::Balanced
        ));
        assert!(matches!(
            resolve_run_path_preset(Some(Preset::Ai)),
            crate::core::path_optimizer::methods::PathDictionaryPreset::Aggressive
        ));
        assert!(matches!(
            resolve_run_path_preset(None),
            crate::core::path_optimizer::methods::PathDictionaryPreset::Balanced
        ));
    }

    #[test]
    fn resolve_vcs_ai_profile_maps_run_intent() {
        assert!(matches!(
            resolve_vcs_ai_profile(Some(VcsRunIntent::Status)),
            crate::plugins::vcs_plugin::methods::VcsAiProfile::Status
        ));
        assert!(matches!(
            resolve_vcs_ai_profile(Some(VcsRunIntent::Log)),
            crate::plugins::vcs_plugin::methods::VcsAiProfile::Log
        ));
        assert!(matches!(
            resolve_vcs_ai_profile(Some(VcsRunIntent::Diff)),
            crate::plugins::vcs_plugin::methods::VcsAiProfile::Diff
        ));
        assert!(matches!(
            resolve_vcs_ai_profile(Some(VcsRunIntent::Other)),
            crate::plugins::vcs_plugin::methods::VcsAiProfile::Other
        ));
        assert!(matches!(
            resolve_vcs_ai_profile(None),
            crate::plugins::vcs_plugin::methods::VcsAiProfile::None
        ));
    }

    #[test]
    fn resolve_run_filter_name_prefers_vcs_plugin_for_vcs_intent() {
        let filter =
            resolve_run_filter_name("git", &["status".to_string()], Some(VcsRunIntent::Status));
        assert_eq!(filter, "vcs_plugin");
    }

    #[test]
    fn resolve_run_filter_name_falls_back_to_first_arg_then_program() {
        let with_args = resolve_run_filter_name("python", &["script.py".to_string()], None);
        assert_eq!(with_args, "script.py");

        let no_args = resolve_run_filter_name("python", &[], None);
        assert_eq!(no_args, "python");
    }

    #[test]
    fn build_run_command_string_joins_program_and_args() {
        assert_eq!(
            build_run_command_string("git", &["status".to_string()]),
            "git status"
        );
        assert_eq!(build_run_command_string("cargo", &[]), "cargo");
    }

    #[test]
    fn maybe_parse_implicit_run_command_accepts_external_command() {
        let argv = vec![
            "tokenslim.exe".to_string(),
            "git".to_string(),
            "remote".to_string(),
            "-v".to_string(),
        ];
        let parsed = maybe_parse_implicit_run_command_from_argv(&argv);
        assert_eq!(
            parsed,
            Some(vec![
                "git".to_string(),
                "remote".to_string(),
                "-v".to_string()
            ])
        );
    }

    #[test]
    fn maybe_parse_implicit_run_command_skips_builtin_command() {
        let argv = vec![
            "tokenslim.exe".to_string(),
            "gain".to_string(),
            "--daily".to_string(),
        ];
        let parsed = maybe_parse_implicit_run_command_from_argv(&argv);
        assert!(parsed.is_none());
    }

    #[test]
    fn maybe_parse_implicit_run_command_skips_long_flag() {
        let argv = vec!["tokenslim.exe".to_string(), "--help".to_string()];
        let parsed = maybe_parse_implicit_run_command_from_argv(&argv);
        assert!(parsed.is_none());
    }

    #[test]
    fn parse_run_target_rejects_empty_command() {
        let args = Vec::<String>::new();
        let err = parse_run_target("tokenslim.exe", &args).expect_err("should reject empty run");
        let msg = match err {
            CliError::InvalidArgs(s) => s,
            other => panic!("expected invalid args, got: {other:?}"),
        };
        assert!(msg.contains("E_CLI_RUN_EMPTY"));
        assert!(msg.contains("No external command was provided for run mode"));
        assert!(msg.contains("tokenslim.exe run git status"));
    }

    #[test]
    fn parse_run_target_rejects_option_as_command() {
        let args = vec!["--gain".to_string()];
        let err = parse_run_target("tokenslim.exe", &args).expect_err("should reject option");
        let msg = match err {
            CliError::InvalidArgs(s) => s,
            other => panic!("expected invalid args, got: {other:?}"),
        };
        assert!(msg.contains("E_CLI_RUN_INVALID_TARGET"));
        assert!(msg.contains("`--gain` is not a valid executable command"));
        assert!(msg.contains("tokenslim.exe gain"));
        assert!(msg.contains("tokenslim.exe git status"));
    }

    #[test]
    fn parse_run_target_accepts_external_command() {
        let args = vec!["git".to_string(), "status".to_string()];
        let (prog, tail) = parse_run_target("tokenslim.exe", &args).expect("valid run target");
        assert_eq!(prog, "git");
        assert_eq!(tail, &["status".to_string()]);
    }

    #[test]
    fn map_clap_error_adds_external_command_hint() {
        let argv = vec![
            "tokenslim.exe".to_string(),
            "gitx".to_string(),
            "status".to_string(),
        ];
        let clap_err = <CliRawArgs as clap::Parser>::try_parse_from(argv.clone())
            .expect_err("should fail for unknown argument style input");
        let err = map_clap_error(clap_err, &argv);
        let msg = match err {
            CliError::InvalidArgs(s) => s,
            other => panic!("expected invalid args, got: {other:?}"),
        };
        assert!(msg.contains("It looks like an external command."));
        assert!(msg.contains("tokenslim.exe run gitx ..."));
        assert!(msg.contains("tokenslim.exe gitx ..."));
    }

    #[test]
    fn detect_external_like_first_arg_distinguishes_builtin_and_external() {
        let external = vec!["tokenslim.exe".to_string(), "gitx".to_string()];
        let (program, first, is_external) = detect_external_like_first_arg(&external);
        assert_eq!(program, "tokenslim.exe");
        assert_eq!(first, "gitx");
        assert!(is_external);

        let builtin = vec!["tokenslim.exe".to_string(), "gain".to_string()];
        let (_, _, is_external_builtin) = detect_external_like_first_arg(&builtin);
        assert!(!is_external_builtin);
    }

    #[test]
    fn parse_rejects_ai_export_and_ai_signal_together() {
        let raw = CliRawArgs {
            mode: Some("compress".to_string()),
            input: None,
            output: None,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            format: "json".to_string(),
            ai_export: true,
            ai_signal: true,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: "text".to_string(),
            strict: false,
            inject: false,
            config: None,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };

        let result = CliArgs::from_raw(raw);
        assert!(matches!(result, Err(CliError::InvalidArgs(_))));
    }

    #[test]
    fn parse_accepts_ai_signal_only() {
        let raw = CliRawArgs {
            mode: Some("decompress".to_string()),
            input: None,
            output: None,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            format: "json".to_string(),
            config: None,
            ai_export: false,
            ai_signal: true,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: "text".to_string(),
            strict: false,
            inject: false,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };

        let parsed = CliArgs::from_raw(raw).expect("ai-signal should be accepted");
        assert!(parsed.ai_signal);
        assert!(!parsed.ai_export);
    }

    #[test]
    fn parse_accepts_init_mode() {
        let raw = CliRawArgs {
            mode: Some("init".to_string()),
            input: None,
            output: None,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            format: "json".to_string(),
            config: None,
            ai_export: false,
            ai_signal: false,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: "text".to_string(),
            strict: false,
            inject: false,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };
        let parsed = CliArgs::from_raw(raw).expect("init mode should parse");
        assert!(matches!(parsed.mode, CliMode::Init));
    }

    #[test]
    fn from_raw_infers_run_mode_when_mode_missing_and_run_command_present() {
        let raw = CliRawArgs {
            mode: None,
            input: None,
            output: None,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            format: "json".to_string(),
            config: None,
            ai_export: false,
            ai_signal: false,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: "text".to_string(),
            strict: false,
            inject: false,
            run_command: vec!["git".to_string(), "status".to_string()],
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };

        let parsed = CliArgs::from_raw(raw).expect("run mode should be inferred");
        assert!(matches!(parsed.mode, CliMode::Run));
        assert_eq!(
            parsed.run_command,
            vec!["git".to_string(), "status".to_string()]
        );
    }

    #[test]
    fn parse_accepts_repair_file_mode() {
        let raw = CliRawArgs {
            mode: Some("repair-file".to_string()),
            input: Some("sample.log".into()),
            output: Some("sample.repaired.log".into()),
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            format: "text".to_string(),
            config: None,
            ai_export: false,
            ai_signal: false,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: "text".to_string(),
            strict: false,
            inject: false,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };
        let parsed = CliArgs::from_raw(raw).expect("repair-file mode should parse");
        assert!(matches!(parsed.mode, CliMode::RepairFile));
    }

    #[test]
    fn parse_preset_arg_rejects_invalid_value() {
        let err = parse_preset_arg(Some("unknown")).expect_err("preset should be invalid");
        match err {
            CliError::InvalidArgs(msg) => assert!(!msg.trim().is_empty()),
            other => panic!("expected invalid args, got {other:?}"),
        }
    }

    #[test]
    fn parse_output_format_arg_accepts_text() {
        let fmt = parse_output_format_arg("text").expect("text format should parse");
        assert!(matches!(fmt, OutputFormat::Text));
    }

    #[test]
    fn rewrite_repair_file_alias_adds_default_output() {
        let args = vec![
            "tokenslim".to_string(),
            "repair-file".to_string(),
            "logs/app.log".to_string(),
        ];
        let rewritten = rewrite_command_alias_to_flags(&args)
            .expect("rewrite should succeed")
            .expect("rewrite should be applied");
        assert!(rewritten.contains(&"--mode".to_string()));
        assert!(rewritten.contains(&"repair-file".to_string()));
        assert!(rewritten.contains(&"--input".to_string()));
        assert!(rewritten.contains(&"logs/app.log".to_string()));
        assert!(rewritten.contains(&"--output".to_string()));
        assert!(
            rewritten
                .iter()
                .any(|x| x.ends_with("app.repaired.log") || x.ends_with("app.repaired.log")),
            "rewritten={rewritten:?}"
        );
    }

    #[test]
    fn rewrite_repair_file_alias_with_inplace_does_not_add_output() {
        let args = vec![
            "tokenslim".to_string(),
            "repair-file".to_string(),
            "logs/app.log".to_string(),
            "--inplace".to_string(),
        ];
        let rewritten = rewrite_command_alias_to_flags(&args)
            .expect("rewrite should succeed")
            .expect("rewrite should be applied");
        assert!(rewritten.contains(&"--inplace".to_string()));
        assert!(!rewritten.contains(&"--output".to_string()));
    }

    #[test]
    fn rewrite_workspace_alias_preserves_inject_flag() {
        let args = vec![
            "tokenslim".to_string(),
            "workspace".to_string(),
            "--inject".to_string(),
        ];
        let rewritten = rewrite_command_alias_to_flags(&args)
            .expect("rewrite should succeed")
            .expect("rewrite should be applied");
        assert_eq!(
            rewritten,
            vec![
                "tokenslim".to_string(),
                "--doctor".to_string(),
                "workspace".to_string(),
                "--inject".to_string(),
            ]
        );
    }

    #[test]
    fn parse_rejects_inplace_outside_repair_mode() {
        let raw = CliRawArgs {
            mode: Some("compress".to_string()),
            input: None,
            output: None,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            format: "json".to_string(),
            config: None,
            ai_export: false,
            ai_signal: false,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: "text".to_string(),
            strict: false,
            inject: false,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: true,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };
        let result = CliArgs::from_raw(raw);
        assert!(matches!(result, Err(CliError::InvalidArgs(_))));
    }

    #[test]
    fn parse_rejects_include_outside_repair_mode() {
        let raw = CliRawArgs {
            mode: Some("compress".to_string()),
            input: None,
            output: None,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            format: "json".to_string(),
            config: None,
            ai_export: false,
            ai_signal: false,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: "text".to_string(),
            strict: false,
            inject: false,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: vec!["*.log".to_string()],
            exclude: Vec::new(),
        };
        let result = CliArgs::from_raw(raw);
        assert!(matches!(result, Err(CliError::InvalidArgs(_))));
    }

    #[test]
    fn run_single_repair_binary_guard_skips_and_does_not_write_target() {
        let temp_dir = make_temp_test_dir("repair_binary_guard");
        let input = temp_dir.join("binary.bin");
        let target = temp_dir.join("binary.repaired.txt");
        let bytes = vec![0, 159, 146, 150, 0, 255, 16];
        std::fs::write(&input, &bytes).expect("write binary fixture");

        let outcome =
            run_single_repair(&input, Some(target.as_path()), false, false).expect("run repair");
        assert!(outcome.skipped);
        assert_eq!(outcome.reason, "binary-guard");
        assert_eq!(outcome.detected_enc, "binary");
        assert_eq!(outcome.strategy, "manual_review_binary_guard");
        assert!(!target.exists());
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn run_single_repair_changes_mojibake_text_and_writes_target() {
        let temp_dir = make_temp_test_dir("repair_single_changed");
        let input = temp_dir.join("bad.log");
        let target = temp_dir.join("bad.repaired.log");
        std::fs::write(&input, "Ã¤Â¸Â­Ã¦â€“â€¡".as_bytes()).expect("write mojibake input");

        let outcome =
            run_single_repair(&input, Some(target.as_path()), false, false).expect("run repair");
        assert!(!outcome.skipped);
        assert!(outcome.changed);
        assert!(!outcome.detected_enc.is_empty());
        assert!(outcome.evidence.contains("repairs="));

        let repaired = std::fs::read_to_string(&target).expect("read repaired");
        assert_eq!(repaired.trim(), "中文");
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn repair_file_inplace_with_backup_creates_bak_and_updates_file() {
        let temp_dir = std::env::temp_dir().join(format!(
            "tokenslim-repair-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let input = temp_dir.join("bad.log");
        let original = "Ã¤Â¸Â­Ã¦â€“â€¡";
        std::fs::write(&input, original.as_bytes()).expect("write input");

        let args = CliArgs {
            mode: CliMode::RepairFile,
            input: InputSource::File(input.clone()),
            output: OutputTarget::Stdout,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            ai_export: false,
            ai_signal: false,
            output_format: OutputFormat::Text,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: DoctorOutputFormat::Text,
            doctor_strict: false,
            inject: false,
            config: None,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: true,
            backup: true,
            include: Vec::new(),
            exclude: Vec::new(),
        };

        run_repair_file_command(&args).expect("repair-file should succeed");

        let repaired = std::fs::read_to_string(&input).expect("read repaired");
        assert_eq!(repaired.trim(), "中文");

        let backup = input.with_file_name("bad.log.bak");
        let backup_text = std::fs::read_to_string(&backup).expect("read backup");
        assert_eq!(backup_text, original);

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn repair_file_directory_dry_run_does_not_modify_files() {
        let temp_dir = std::env::temp_dir().join(format!(
            "tokenslim-repair-dir-dry-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let nested = temp_dir.join("nested");
        std::fs::create_dir_all(&nested).expect("create nested");

        let f1 = temp_dir.join("a.log");
        let f2 = nested.join("b.log");
        let b1 = temp_dir.join("x.bin");
        let bad = "Ã¤Â¸Â­Ã¦â€“â€¡";
        std::fs::write(&f1, bad.as_bytes()).expect("write f1");
        std::fs::write(&f2, bad.as_bytes()).expect("write f2");
        std::fs::write(&b1, [0x7F, b'E', b'L', b'F', 0, 1, 2, 3]).expect("write binary");

        let args = CliArgs {
            mode: CliMode::RepairFile,
            input: InputSource::File(temp_dir.clone()),
            output: OutputTarget::Stdout,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            ai_export: false,
            ai_signal: false,
            output_format: OutputFormat::Text,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: true,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: DoctorOutputFormat::Text,
            doctor_strict: false,
            inject: false,
            config: None,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: true,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };
        run_repair_file_command(&args).expect("dry run should succeed");

        let c1 = std::fs::read_to_string(&f1).expect("read f1");
        let c2 = std::fs::read_to_string(&f2).expect("read f2");
        assert_eq!(c1, bad);
        assert_eq!(c2, bad);
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn repair_file_directory_inplace_modifies_text_files() {
        let temp_dir = std::env::temp_dir().join(format!(
            "tokenslim-repair-dir-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let f1 = temp_dir.join("a.log");
        let bad = "Ã¤Â¸Â­Ã¦â€“â€¡";
        std::fs::write(&f1, bad.as_bytes()).expect("write f1");

        let args = CliArgs {
            mode: CliMode::RepairFile,
            input: InputSource::File(temp_dir.clone()),
            output: OutputTarget::Stdout,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            ai_export: false,
            ai_signal: false,
            output_format: OutputFormat::Text,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: DoctorOutputFormat::Text,
            doctor_strict: false,
            inject: false,
            config: None,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: true,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };
        run_repair_file_command(&args).expect("directory repair should succeed");
        let c1 = std::fs::read_to_string(&f1).expect("read f1");
        assert_eq!(c1.trim(), "中文");
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn repair_file_directory_include_filter_only_changes_matching_files() {
        let temp_dir = std::env::temp_dir().join(format!(
            "tokenslim-repair-dir-filter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let f1 = temp_dir.join("a.log");
        let f2 = temp_dir.join("b.txt");
        let bad = "Ã¤Â¸Â­Ã¦â€“â€¡";
        std::fs::write(&f1, bad.as_bytes()).expect("write f1");
        std::fs::write(&f2, bad.as_bytes()).expect("write f2");

        let args = CliArgs {
            mode: CliMode::RepairFile,
            input: InputSource::File(temp_dir.clone()),
            output: OutputTarget::Stdout,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            ai_export: false,
            ai_signal: false,
            output_format: OutputFormat::Text,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: DoctorOutputFormat::Text,
            doctor_strict: false,
            inject: false,
            config: None,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: true,
            backup: false,
            include: vec!["*.log".to_string()],
            exclude: Vec::new(),
        };
        run_repair_file_command(&args).expect("directory repair with include should succeed");

        let c1 = std::fs::read_to_string(&f1).expect("read f1");
        let c2 = std::fs::read_to_string(&f2).expect("read f2");
        assert_eq!(c1.trim(), "中文");
        assert_eq!(c2, bad);
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn repair_file_directory_exclude_filter_skips_matching_files() {
        let temp_dir = std::env::temp_dir().join(format!(
            "tokenslim-repair-dir-exclude-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let f1 = temp_dir.join("a.log");
        let f2 = temp_dir.join("b.log");
        let bad = "Ã¤Â¸Â­Ã¦â€“â€¡";
        std::fs::write(&f1, bad.as_bytes()).expect("write f1");
        std::fs::write(&f2, bad.as_bytes()).expect("write f2");

        let args = CliArgs {
            mode: CliMode::RepairFile,
            input: InputSource::File(temp_dir.clone()),
            output: OutputTarget::Stdout,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            ai_export: false,
            ai_signal: false,
            output_format: OutputFormat::Text,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: DoctorOutputFormat::Text,
            doctor_strict: false,
            inject: false,
            config: None,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: true,
            backup: false,
            include: vec!["*.log".to_string()],
            exclude: vec!["a.log".to_string()],
        };
        run_repair_file_command(&args).expect("directory repair with exclude should succeed");

        let c1 = std::fs::read_to_string(&f1).expect("read f1");
        let c2 = std::fs::read_to_string(&f2).expect("read f2");
        assert_eq!(c1, bad);
        assert_eq!(c2.trim(), "中文");
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn repair_file_single_file_filter_skip_keeps_original() {
        let temp_dir = std::env::temp_dir().join(format!(
            "tokenslim-repair-single-filter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let file_path = temp_dir.join("a.log");
        let bad = "Ã¤Â¸Â­Ã¦â€“â€¡";
        std::fs::write(&file_path, bad.as_bytes()).expect("write file");

        let args = CliArgs {
            mode: CliMode::RepairFile,
            input: InputSource::File(file_path.clone()),
            output: OutputTarget::Stdout,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            ai_export: false,
            ai_signal: false,
            output_format: OutputFormat::Text,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: DoctorOutputFormat::Text,
            doctor_strict: false,
            inject: false,
            config: None,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: true,
            backup: false,
            include: vec!["*.txt".to_string()],
            exclude: Vec::new(),
        };
        run_repair_file_command(&args)
            .expect("single-file repair with non-matching include should succeed");

        let c = std::fs::read_to_string(&file_path).expect("read file");
        assert_eq!(c, bad);
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn repair_json_record_contains_repair_chain() {
        let outcome = RepairOutcome {
            path: std::path::PathBuf::from("sample.log"),
            detected_enc: "windows-1252".to_string(),
            confidence: "high".to_string(),
            strategy: "reencode_recover_high".to_string(),
            repair_chain: "mojibake-repair-pass-1:windows-1252->utf8".to_string(),
            steps: vec!["mojibake-repair-pass-1:windows-1252->utf8".to_string()],
            evidence_items: vec!["confidence=high".to_string()],
            evidence: "confidence=high".to_string(),
            changed: true,
            skipped: false,
            reason: String::new(),
        };
        let record = to_repair_json_record(&outcome);
        assert_eq!(
            record.repair_chain,
            "mojibake-repair-pass-1:windows-1252->utf8"
        );
    }

    #[test]
    fn parse_rejects_partial_verify_args() {
        let raw = CliRawArgs {
            mode: Some("decompress".to_string()),
            input: None,
            output: None,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            format: "json".to_string(),
            config: None,
            ai_export: false,
            ai_signal: false,
            verify_rule: Some("rule.toml".into()),
            verify_fixture: Some("fixture.log".into()),
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: "text".to_string(),
            strict: false,
            inject: false,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };

        let result = CliArgs::from_raw(raw);
        assert!(matches!(result, Err(CliError::InvalidArgs(_))));
    }

    #[test]
    fn parse_rejects_conflicting_hook_actions() {
        let raw = CliRawArgs {
            mode: Some("compress".to_string()),
            input: None,
            output: None,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            format: "json".to_string(),
            config: None,
            ai_export: false,
            ai_signal: false,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: true,
            uninstall_hooks: true,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: "text".to_string(),
            strict: false,
            inject: false,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };
        let result = CliArgs::from_raw(raw);
        assert!(matches!(result, Err(CliError::InvalidArgs(_))));
    }

    #[test]
    fn detects_vcs_run_intent_across_tools() {
        assert_eq!(
            detect_vcs_run_intent("git", &["status".to_string()]),
            Some(VcsRunIntent::Status)
        );
        assert_eq!(
            detect_vcs_run_intent("git", &["log".to_string()]),
            Some(VcsRunIntent::Log)
        );
        assert_eq!(
            detect_vcs_run_intent("svn", &["diff".to_string()]),
            Some(VcsRunIntent::Diff)
        );
        assert_eq!(
            detect_vcs_run_intent("hg", &["summary".to_string()]),
            Some(VcsRunIntent::Status)
        );
        assert_eq!(detect_vcs_run_intent("cargo", &["test".to_string()]), None);
    }

    #[test]
    fn detects_vcs_run_intent_for_git_after_global_options() {
        assert_eq!(
            detect_vcs_run_intent(
                "C:\\Program Files\\Git\\cmd\\git.exe",
                &[
                    "-C".to_string(),
                    "C:\\repo".to_string(),
                    "--git-dir".to_string(),
                    ".git".to_string(),
                    "log".to_string(),
                    "-n".to_string(),
                    "2".to_string(),
                ],
            ),
            Some(VcsRunIntent::Log)
        );
    }

    #[test]
    fn detect_run_plugin_route_maps_keywords() {
        assert_eq!(detect_run_plugin_route("git", &[]), RunPluginRoute::Vcs);
        assert_eq!(
            detect_run_plugin_route("npm.cmd", &[]),
            RunPluginRoute::Node
        );
        assert_eq!(
            detect_run_plugin_route("cargo.exe", &[]),
            RunPluginRoute::Build
        );
        assert_eq!(
            detect_run_plugin_route("unknown-tool", &[]),
            RunPluginRoute::Generic
        );
    }

    #[test]
    fn split_run_explain_route_flag_strips_flag_before_command() {
        let (explain, command) = split_run_explain_route_flag(vec![
            "--explain-route".to_string(),
            "cargo".to_string(),
            "test".to_string(),
        ]);
        assert!(explain);
        assert_eq!(command, vec!["cargo".to_string(), "test".to_string()]);
    }

    #[test]
    fn explain_run_route_prints_decision_and_plugin_chain() {
        let args = CliArgs {
            mode: CliMode::Run,
            input: InputSource::Stdin,
            output: OutputTarget::Stdout,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            ai_export: false,
            ai_signal: false,
            output_format: OutputFormat::Text,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: DoctorOutputFormat::Text,
            doctor_strict: false,
            inject: false,
            config: None,
            run_command: vec!["cargo".to_string(), "test".to_string()],
            explain_route: true,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: Some(Preset::Ai),
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };

        let out = explain_run_route("cargo", &["test".to_string()], &args);
        assert!(out.contains("route_group=build"));
        assert!(out.contains("normalized_tool=cargo"));
        assert!(out.contains("matched_by=keyword"));
        assert!(out.contains("route_candidates=1"));
        assert!(out.contains("route_candidate_1=build|group=build"));
        assert!(out.contains("plugin_chain="));
        assert!(out.contains("rust_go"));
        assert!(!out.contains("vcs,"));
    }

    #[test]
    fn explain_run_route_shows_arg_prefix_candidate_before_keyword_candidate() {
        let args = CliArgs {
            mode: CliMode::Run,
            input: InputSource::Stdin,
            output: OutputTarget::Stdout,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            ai_export: false,
            ai_signal: false,
            output_format: OutputFormat::Text,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: DoctorOutputFormat::Text,
            doctor_strict: false,
            inject: false,
            config: None,
            run_command: vec![
                "az".to_string(),
                "pipelines".to_string(),
                "runs".to_string(),
                "show".to_string(),
            ],
            explain_route: true,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };

        let cmd_args = vec![
            "pipelines".to_string(),
            "runs".to_string(),
            "show".to_string(),
        ];
        let out = explain_run_route("az", &cmd_args, &args);
        assert!(out.contains("route_plugin=ci_log"));
        assert!(out.contains("matched_by=arg_prefix"));
        assert!(out.contains("route_candidates=2"));
        assert!(out.contains("route_candidate_1=ci_log|group=build"));
        assert!(out.contains("route_candidate_2=vcs|group=vcs"));
    }

    #[test]
    fn explain_plugin_for_command_line_shows_selected_alternatives_and_evidence() {
        let args = CliArgs {
            mode: CliMode::ExplainPlugin,
            input: InputSource::Stdin,
            output: OutputTarget::Stdout,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            ai_export: false,
            ai_signal: false,
            output_format: OutputFormat::Text,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: DoctorOutputFormat::Text,
            doctor_strict: false,
            inject: false,
            config: None,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: Some("az pipelines runs show".to_string()),
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };

        let out = explain_plugin_for_command_line("az pipelines runs show", &args);
        assert!(out.contains("plugin_selection"));
        assert!(out.contains("input_kind=command"));
        assert!(out.contains("selected_plugin=ci_log"));
        assert!(out.contains("why=command_tool:az matched_by:arg_prefix"));
        assert!(out.contains("selected_capability="));
        assert!(out.contains("recommendation_primary=ci_log"));
        assert!(out.contains("recommendation_confidence=high"));
        assert!(out.contains("recommendation_action=accept"));
        assert!(out.contains("recommendation_alternative_1=vcs"));
        assert!(out.contains("recommendation_alternative_2=none"));
        assert!(out.contains("recommendation_reason=route_match:arg_prefix"));
        assert!(out.contains("confidence_gap_source=route_priority"));
        assert!(out.contains("alternative_1=vcs|group=vcs"));
        assert!(out.contains("candidate_plugin_chain="));
    }

    #[test]
    fn explain_plugin_for_command_line_handles_invalid_input() {
        let mut args = base_cli_args(CliMode::ExplainPlugin);
        args.output_format = OutputFormat::Text;
        let out = explain_plugin_for_command_line("   ", &args);
        assert!(out.contains("selected_plugin=none"));
        assert!(out.contains("reason=invalid_command_line"));
    }

    #[test]
    fn build_command_route_recommendation_for_stable_route_accepts() {
        let route = crate::core::plugin_config_loader::RunRouteDecision {
            plugin_name: "ci_log".to_string(),
            route_group: "build".to_string(),
            intent: Some("log".to_string()),
            is_fallback: false,
            command_keyword: "az".to_string(),
            matched_by: "arg_prefix".to_string(),
            matched_pattern: Some("pipelines runs".to_string()),
            priority: Some(120),
        };
        let alt = crate::core::plugin_config_loader::RunRouteDecision {
            plugin_name: "vcs".to_string(),
            route_group: "vcs".to_string(),
            intent: Some("log".to_string()),
            is_fallback: false,
            command_keyword: "az".to_string(),
            matched_by: "keyword".to_string(),
            matched_pattern: Some("az".to_string()),
            priority: Some(80),
        };
        let alts = vec![&alt];
        let rec = build_command_route_recommendation(&route, &alts);
        assert_eq!(rec.fallback_decision, "stable_route");
        assert_eq!(rec.recommendation_action, "accept");
        assert_eq!(rec.recommendation_confidence, "high");
        assert_eq!(rec.retry_plugin, "none");
        assert_eq!(rec.confidence_gap, "40");
        assert!(rec.recommendation_reason.contains("route_match:arg_prefix"));
    }

    #[test]
    fn build_command_route_recommendation_for_fallback_requests_retry() {
        let route = crate::core::plugin_config_loader::RunRouteDecision {
            plugin_name: "generic_text".to_string(),
            route_group: "fallback".to_string(),
            intent: None,
            is_fallback: true,
            command_keyword: "unknown".to_string(),
            matched_by: "fallback".to_string(),
            matched_pattern: None,
            priority: Some(0),
        };
        let alt1 = crate::core::plugin_config_loader::RunRouteDecision {
            plugin_name: "ci_log".to_string(),
            route_group: "build".to_string(),
            intent: Some("log".to_string()),
            is_fallback: false,
            command_keyword: "unknown".to_string(),
            matched_by: "keyword".to_string(),
            matched_pattern: Some("ci".to_string()),
            priority: Some(80),
        };
        let alt2 = crate::core::plugin_config_loader::RunRouteDecision {
            plugin_name: "vcs".to_string(),
            route_group: "vcs".to_string(),
            intent: Some("status".to_string()),
            is_fallback: false,
            command_keyword: "unknown".to_string(),
            matched_by: "keyword".to_string(),
            matched_pattern: Some("git".to_string()),
            priority: Some(70),
        };
        let alts = vec![&alt1, &alt2];
        let rec = build_command_route_recommendation(&route, &alts);
        assert_eq!(rec.fallback_decision, "fallback_selected");
        assert_eq!(rec.recommendation_action, "review_and_retry");
        assert_eq!(rec.recommendation_confidence, "low");
        assert_eq!(rec.retry_plugin, "ci_log");
        assert_eq!(rec.recommendation_alternative_1, "ci_log");
        assert_eq!(rec.recommendation_alternative_2, "vcs");
        assert_eq!(rec.confidence_gap, "-80");
        assert!(rec
            .recommendation_reason
            .contains("fallback_route_selected"));
    }

    #[test]
    fn explain_plugin_for_log_text_shows_detector_scores_and_evidence() {
        let log = r#"203.0.113.7 - - [13/May/2026:08:13:39 +0000] "GET /health HTTP/1.1" 200 2 "-" "kube-probe/1.29" 0.001
203.0.113.8 - - [13/May/2026:08:13:40 +0000] "GET /api/v1/orders HTTP/1.1" 503 41 "-" "Mozilla/5.0" 1.532"#;

        let out = explain_plugin_for_log_text(log, 0.15);
        assert!(out.contains("plugin_selection"));
        assert!(out.contains("input_kind=log"));
        assert!(out.contains("selected_plugin=web_log"));
        assert!(out.contains("why=content_detector_score:"));
        assert!(out.contains("fallback_decision=stable_detector"));
        assert!(out.contains("retry_plugin=none"));
        assert!(out.contains("recommendation_primary=web_log"));
        assert!(out.contains("recommendation_confidence=medium"));
        assert!(out.contains("recommendation_action=accept"));
        assert!(out.contains("recommendation_alternative_1=smart_path"));
        assert!(out.contains("recommendation_alternative_2="));
        assert!(out.contains("recommendation_reason=detector_stable"));
        assert!(out.contains("confidence_gap_source=detector_score"));
        assert!(out.contains("fallback_note=nearest_candidate_non_retryable:smart_path"));
        assert!(out.contains("selected_capability="));
        assert!(out.contains("status:frozen"));
    }

    #[test]
    fn build_log_explain_recommendation_returns_fallback_for_empty_detections() {
        let detections: Vec<(String, u8, f32)> = Vec::new();
        let rec = build_log_explain_recommendation(&detections, 0.15);
        assert_eq!(rec.selected.0, "generic_text");
        assert_eq!(rec.fallback_decision, "fallback_selected");
        assert_eq!(rec.retry_plugin, "none");
        assert_eq!(rec.recommendation_action, "review_generic_fallback");
        assert_eq!(rec.recommendation_confidence, "low");
    }

    #[test]
    fn build_log_explain_recommendation_requests_retry_when_competitor_is_close() {
        let detections = vec![
            ("generic_text".to_string(), 255, 0.91),
            ("web_log".to_string(), 30, 0.82),
            ("smart_path".to_string(), 10, 0.80),
        ];
        let rec = build_log_explain_recommendation(&detections, 0.15);
        assert_eq!(rec.fallback_decision, "review_recommended");
        assert_eq!(rec.retry_plugin, "web_log");
        assert_eq!(rec.recommendation_action, "review_and_retry");
        assert_eq!(rec.recommendation_confidence, "medium");
        assert_eq!(rec.recommendation_alternative_1, "web_log");
        assert_eq!(rec.recommendation_alternative_2, "smart_path");
        assert!(rec.recommendation_reason.contains("close_competitor"));
    }

    #[test]
    fn explain_plugin_for_command_line_fallback_sample_recommends_review() {
        let sample_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("explain_plugin")
            .join("case_001_command_fallback.txt");
        let command_line = std::fs::read_to_string(&sample_path)
            .expect("read fallback command sample")
            .trim()
            .to_string();

        let args = CliArgs {
            mode: CliMode::ExplainPlugin,
            input: InputSource::Stdin,
            output: OutputTarget::Stdout,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            ai_export: false,
            ai_signal: false,
            output_format: OutputFormat::Text,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: DoctorOutputFormat::Text,
            doctor_strict: false,
            inject: false,
            config: None,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: Some(command_line.clone()),
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };

        let out = explain_plugin_for_command_line(&command_line, &args);
        assert!(out.contains("selected_plugin=generic_text"));
        assert!(out.contains("fallback_decision=fallback_selected"));
        assert!(out.contains("recommendation_action=review_and_retry"));
        assert!(out.contains("recommendation_confidence=low"));
    }

    #[test]
    fn handle_pre_pipeline_action_continue_returns_false() {
        let args = base_cli_args(CliMode::Compress);
        let handled = handle_pre_pipeline_action(&args).expect("continue should not fail");
        assert!(!handled);
    }

    #[test]
    fn explain_plugin_for_log_text_review_recommended_sample_has_retry_plugin() {
        let sample_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("artifact_summary_plugin")
            .join("case_002_junit_failures.xml");
        let log_text =
            std::fs::read_to_string(&sample_path).expect("read review-recommended sample");

        let out = explain_plugin_for_log_text(&log_text, 0.15);
        assert!(out.contains("selected_plugin=artifact_summary"));
        assert!(out.contains("fallback_decision=review_recommended"));
        assert!(out.contains("recommendation_action=review_and_retry"));
        assert!(out.contains("retry_plugin=nodejs"));
        assert!(out.contains("confidence_gap_source=detector_score"));
    }

    #[test]
    fn write_explain_replay_template_contains_recommendation_fields() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test_explain_replay_template.md");
        let report = "plugin_selection\nselected_plugin=ci_log\nrecommendation_primary=ci_log\nrecommendation_confidence=high\nrecommendation_action=accept\n";

        write_explain_replay_template(&path, "command", "az pipelines runs show", report)
            .expect("write replay template");

        let content = std::fs::read_to_string(&path).expect("read replay template");
        assert!(
            content.contains("recommendation_confidence: <copy_from_recommendation_confidence>")
        );
        assert!(content.contains("recommendation_action: <copy_from_recommendation_action>"));
        assert!(content.contains(
            "Inspect `recommendation_primary/recommendation_confidence/recommendation_action/recommendation_reason`",
        ));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn render_explain_report_json_exposes_structured_recommendation() {
        let args = CliArgs {
            mode: CliMode::ExplainPlugin,
            input: InputSource::Stdin,
            output: OutputTarget::Stdout,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            ai_export: false,
            ai_signal: false,
            output_format: OutputFormat::Json,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: DoctorOutputFormat::Text,
            doctor_strict: false,
            inject: false,
            config: None,
            run_command: Vec::new(),
            explain_route: false,
            explain_command: Some("az pipelines runs show".to_string()),
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };

        let raw = explain_plugin_for_command_line("az pipelines runs show", &args);
        let json_text = render_explain_report_json(&raw).expect("render explain json");
        let value: serde_json::Value =
            serde_json::from_str(&json_text).expect("parse explain json");

        assert_eq!(value["selected_plugin"], "ci_log");
        assert_eq!(value["recommendation"]["primary"], "ci_log");
        assert_eq!(value["recommendation"]["confidence"], "high");
        assert_eq!(value["recommendation"]["action"], "accept");
        assert_eq!(value["recommendation"]["alternative_1"], "vcs");
        assert_eq!(value["recommendation"]["alternative_2"], "none");
        assert_eq!(
            value["recommendation"]["confidence_gap_source"],
            "route_priority"
        );
        assert_eq!(value["contract_version"], "explain.v1");
        assert_eq!(value["contract_ok"], true);
        assert!(value["missing_required_fields"]
            .as_array()
            .expect("missing_required_fields should be array")
            .is_empty());
        assert_eq!(value["selected"]["plugin"], "ci_log");
    }

    #[test]
    fn render_explain_report_json_marks_contract_not_ok_when_required_fields_missing() {
        let report = "plugin_selection\ninput_kind=command\nselected_plugin=generic_text\n";
        let json_text = render_explain_report_json(report).expect("render explain json");
        let value: serde_json::Value =
            serde_json::from_str(&json_text).expect("parse explain json");

        assert_eq!(value["contract_version"], "explain.v1");
        assert_eq!(value["contract_ok"], false);
        assert_eq!(value["selected_plugin"], "generic_text");
        let missing = value["missing_required_fields"]
            .as_array()
            .expect("missing_required_fields should be array");
        assert!(missing
            .iter()
            .any(|item| item == &serde_json::Value::String("fallback_decision".to_string())));
        assert!(missing
            .iter()
            .any(|item| item == &serde_json::Value::String("recommendation_action".to_string())));
    }

    #[test]
    fn render_explain_report_json_skips_malformed_alternative_and_sorts_rank() {
        let report = "plugin_selection\ninput_kind=command\nselected_plugin=ci_log\nfallback_decision=stable_route\nretry_plugin=none\nrecommendation_primary=ci_log\nrecommendation_confidence=high\nrecommendation_action=accept\nrecommendation_reason=route_priority_high\nconfidence_gap=0.500\nconfidence_gap_source=route_priority\nalternatives=2\nalternative_x=bad|score=0.2|priority=1\nalternative_2=vcs|score=0.5|priority=95\nalternative_1=ci_log|score=1.0|priority=100\n";
        let json_text = render_explain_report_json(report).expect("render explain json");
        let value: serde_json::Value =
            serde_json::from_str(&json_text).expect("parse explain json");
        let alternatives = value["alternatives"]
            .as_array()
            .expect("alternatives should be array");

        assert_eq!(alternatives.len(), 2);
        assert_eq!(alternatives[0]["rank"], 1);
        assert_eq!(alternatives[0]["plugin"], "ci_log");
        assert_eq!(alternatives[1]["rank"], 2);
        assert_eq!(alternatives[1]["plugin"], "vcs");
    }

    #[test]
    fn render_explain_report_markdown_contains_recommendation_section() {
        let report = "plugin_selection\ninput_kind=log\nselected_plugin=web_log\nfallback_decision=stable_detector\nretry_plugin=none\nconfidence_gap=0.100\nconfidence_gap_source=detector_score\nrecommendation_primary=web_log\nrecommendation_confidence=medium\nrecommendation_action=accept\nrecommendation_alternative_1=smart_path\nrecommendation_alternative_2=generic_text\nrecommendation_reason=detector_stable\nselected_capability=description:web access log\nalternative_1=smart_path|score=0.9|priority=10\nalternative_1_capability=description:path helper\n";
        let md = render_explain_report_markdown(report);
        assert!(md.contains("# Plugin Selection"));
        assert!(md.contains("## Recommendation"));
        assert!(md.contains("primary: `web_log`"));
        assert!(md.contains("confidence: `medium`"));
        assert!(md.contains("confidence_gap: `0.100`"));
        assert!(md.contains("## Evidence"));
        assert!(md.contains("## Alternatives"));
    }

    #[test]
    fn plugins_for_run_command_excludes_vcs_for_cargo_build_commands() {
        let plugins = plugins_for_run_command("cargo", &["build".to_string()]);
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"generic_text"));
        assert!(names.contains(&"gcc_log"));
        assert!(names.contains(&"rust_go"));
        assert!(!names.contains(&"vcs"));
        assert!(!names.contains(&"git_diff"));
    }

    #[test]
    fn plugins_for_run_command_routes_new_build_and_data_tools() {
        let cases = [
            ("pytest", vec!["tests/".to_string()], "pytest"),
            (
                "go",
                vec!["test".to_string(), "-json".to_string()],
                "ndjson",
            ),
            ("gradle", vec!["build".to_string()], "android_gradle"),
            ("gradlew.bat", vec!["test".to_string()], "android_gradle"),
            (
                "cmake",
                vec!["--build".to_string(), "build".to_string()],
                "gcc_log",
            ),
            (
                "ninja",
                vec!["-C".to_string(), "build".to_string()],
                "gcc_log",
            ),
            (
                "docker",
                vec!["build".to_string(), ".".to_string()],
                "kubernetes_docker",
            ),
            (
                "docker-compose",
                vec!["up".to_string(), "--build".to_string()],
                "kubernetes_docker",
            ),
            (
                "kubectl",
                vec!["get".to_string(), "pods".to_string()],
                "kubernetes_docker",
            ),
            (
                "act",
                vec!["-j".to_string(), "build".to_string()],
                "generic_text",
            ),
            (
                "circleci",
                vec!["local".to_string(), "execute".to_string()],
                "generic_text",
            ),
            (
                "buildkite-agent",
                vec!["pipeline".to_string(), "upload".to_string()],
                "generic_text",
            ),
            (
                "az",
                vec![
                    "--subscription".to_string(),
                    "sub-001".to_string(),
                    "pipelines".to_string(),
                    "runs".to_string(),
                    "show".to_string(),
                ],
                "ci_log",
            ),
            (
                "psql",
                vec!["-c".to_string(), "select 1".to_string()],
                "db_log",
            ),
            (
                "mongosh",
                vec!["--eval".to_string(), "db.stats()".to_string()],
                "db_log",
            ),
            (
                "redis-cli",
                vec!["slowlog".to_string(), "get".to_string()],
                "db_log",
            ),
        ];

        for (prog, args, expected_plugin) in cases {
            assert_eq!(
                detect_run_plugin_route(prog, &args),
                RunPluginRoute::Build,
                "{prog} should use the non-VCS build/data route"
            );
            let plugins = plugins_for_run_command(prog, &args);
            let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
            assert!(
                names.contains(&expected_plugin),
                "{prog} should include {expected_plugin}, got {names:?}"
            );
            assert!(!names.contains(&"vcs"));
            assert!(!names.contains(&"git_diff"));
        }
    }

    #[test]
    fn plugins_for_run_command_keeps_az_repos_on_vcs_route() {
        assert_eq!(
            detect_run_plugin_route("az", &["repos".to_string(), "show".to_string()]),
            RunPluginRoute::Vcs
        );

        let plugins = plugins_for_run_command("az", &["repos".to_string(), "show".to_string()]);
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"vcs"));
    }

    #[test]
    fn plugins_for_run_command_excludes_vcs_for_node_commands() {
        let plugins = plugins_for_run_command("npm", &["-g".to_string()]);
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"nodejs"));
        assert!(names.contains(&"generic_text"));
        assert!(!names.contains(&"vcs"));
        assert!(!names.contains(&"git_diff"));
        assert_eq!(detect_vcs_run_intent("npm", &["-g".to_string()]), None);
    }

    #[test]
    fn plugins_for_run_command_uses_generic_chain_for_unknown_commands() {
        let plugins = plugins_for_run_command("foobar", &["hello".to_string()]);
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"generic_text"));
        assert!(names.contains(&"ansi_cleaner"));
        assert!(names.contains(&"noise_filter"));
        assert!(!names.contains(&"vcs"));
    }

    #[test]
    fn remove_vcs_plugins_strips_vcs_and_git_diff() {
        let plugins = remove_vcs_plugins(get_plugins());
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(!names.contains(&"vcs"));
        assert!(!names.contains(&"git_diff"));
        assert!(names.contains(&"generic_text"));
    }

    #[test]
    fn keep_generic_run_plugins_only_keeps_expected_chain() {
        let plugins = keep_generic_run_plugins(get_plugins());
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"generic_text"));
        assert!(names.contains(&"ansi_cleaner"));
        assert!(names.contains(&"noise_filter"));
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn detect_vcs_run_intent_defaults_to_other_when_vcs_route_has_no_subcommand() {
        let intent = detect_vcs_run_intent("git", &[]);
        assert!(matches!(intent, Some(VcsRunIntent::Other)));
    }

    #[test]
    fn optimize_paths_block_merges_and_filters_unprofitable_tokens() {
        let input = "changes:\nM $P1/issues.md\nM $P1/learnings.md\nM $P1/notes.md\nM $P1/plan.md\nM $P10/single.md\npaths: $P1=.sisyphus/notepads/REFACTORING_PLAN_V6.2\npaths: $P10=.tokenslim-context.md\n";

        let out = optimize_path_dictionary_blocks(input);
        println!("{}", out);

        assert_eq!(out.matches("paths:").count(), 1);
        assert!(out.contains("/issues.md"));
        assert!(out.contains("/learnings.md"));
        assert!(out.contains("/notes.md"));
        assert!(out.contains("/plan.md"));
        assert!(out.contains("single.md"));
        assert!(!out.contains("$P10/single.md"));
    }

    #[test]
    fn optimize_paths_keeps_two_use_medium_prefix_at_break_even() {
        let input = "changes:\nM $P1/a.md\nM $P1/b.md\npaths: $P1=docs/design\n";
        let out = optimize_path_dictionary_blocks(input);
        assert!(out.contains("$P1/a.md"));
        assert!(out.contains("$P1/b.md"));
        assert!(out.contains("paths: $P1=docs/design"));
    }

    #[test]
    fn count_paths_footer_lines_counts_only_footer_lines() {
        let input = "changes:\nM foo/bar.rs\npaths: $P1=src/core\nuntracked:\n?? docs/design/paths: note\npaths: $P2=docs/design\n";
        assert_eq!(count_paths_footer_lines(input), 2);
    }

    #[test]
    fn count_path_token_uses_respects_token_boundaries() {
        let input = "A $P1/file\nB $P10/file\nC $P1-more\nD $P1\n";
        assert_eq!(count_path_token_uses(input, "$P1"), 2);
        assert_eq!(count_path_token_uses(input, "$P10"), 1);
    }

    #[test]
    fn append_paths_footer_skips_single_use_token() {
        let mut output = crate::core::compression::CompressionOutput {
            tokens: Vec::new(),
            dictionary: crate::core::dictionary_engine::Dictionary::default(),
            metadata: crate::core::compression::CompressionMetadata::default(),
        };
        output
            .dictionary
            .paths
            .insert("$P1".to_string(), "C:\\git_work".to_string());
        let input = "$P1\\TokenSlim 的目录\n";
        let options = crate::core::path_optimizer::methods::PathDictionaryOptions::default();

        let out = append_paths_footer_from_output_dictionary(input, &output, &options);
        assert!(!out.contains("paths: "));
        assert_eq!(out, "C:\\git_work\\TokenSlim 的目录\n");
    }

    #[test]
    fn append_paths_footer_keeps_multi_use_token_and_appends_footer() {
        let mut output = crate::core::compression::CompressionOutput {
            tokens: Vec::new(),
            dictionary: crate::core::dictionary_engine::Dictionary::default(),
            metadata: crate::core::compression::CompressionMetadata::default(),
        };
        output
            .dictionary
            .paths
            .insert("$P1".to_string(), "src/core".to_string());
        let input = "M $P1/a.rs\nA $P1/b.rs\n";
        let options = crate::core::path_optimizer::methods::PathDictionaryOptions::default();

        let out = append_paths_footer_from_output_dictionary(input, &output, &options);
        assert!(out.contains("M $P1/a.rs"));
        assert!(out.contains("A $P1/b.rs"));
        assert!(out.contains("paths: $P1=src/core"));
    }

    #[test]
    fn append_paths_footer_keeps_existing_footer_unchanged() {
        let mut output = crate::core::compression::CompressionOutput {
            tokens: Vec::new(),
            dictionary: crate::core::dictionary_engine::Dictionary::default(),
            metadata: crate::core::compression::CompressionMetadata::default(),
        };
        output
            .dictionary
            .paths
            .insert("$P1".to_string(), "src/core".to_string());
        let input = "M $P1/a.rs\npaths: $P1=src/core\n";
        let options = crate::core::path_optimizer::methods::PathDictionaryOptions::default();

        let out = append_paths_footer_from_output_dictionary(input, &output, &options);
        assert_eq!(out, input);
    }

    #[test]
    fn is_alternative_rank_entry_key_skips_metadata_entries() {
        assert!(is_alternative_rank_entry_key("alternative_1"));
        assert!(!is_alternative_rank_entry_key("alternative_1_capability"));
        assert!(!is_alternative_rank_entry_key(
            "alternative_1_declared_patterns"
        ));
        assert!(!is_alternative_rank_entry_key("alternatives"));
        assert!(!is_alternative_rank_entry_key("selected_plugin"));
    }

    #[test]
    fn parse_plugin_capability_evidence_limits_detect_patterns_to_top_five() {
        let plugin = serde_json::json!({
            "name": "web_log",
            "description": "web access log",
            "capability_tags": "log,web",
            "route_group": "logs",
            "sample_cases": 48,
            "showcase_cases": 40,
            "audit_cases": 40,
            "frozen_cases": 40,
            "coverage_status": "ok",
            "detect_patterns": ["p1","p2","p3","p4","p5","p6"]
        });
        let evidence = parse_plugin_capability_evidence(&plugin);
        assert_eq!(evidence.description, "web access log");
        assert_eq!(evidence.detect_patterns.len(), 5);
        assert_eq!(evidence.detect_patterns[0], "p1");
        assert_eq!(evidence.detect_patterns[4], "p5");
    }

    #[test]
    fn compress_vcs_run_as_single_document_produces_single_slice_metadata() {
        let input = "git status\nM src/main.rs\n";
        let out = compress_vcs_run_as_single_document(input);
        assert_eq!(out.metadata.slice_count, 1);
        assert_eq!(out.metadata.original_size, input.len());
    }

    #[test]
    fn apply_run_mode_defaults_sets_text_and_ai_preset_when_not_explicit() {
        let parsed = CliArgs {
            mode: CliMode::Run,
            input: InputSource::Stdin,
            output: OutputTarget::Stdout,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            ai_export: false,
            ai_signal: false,
            output_format: OutputFormat::Json,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: DoctorOutputFormat::Text,
            doctor_strict: false,
            inject: false,
            config: None,
            run_command: vec![],
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: None,
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };

        let argv = vec!["tokenslim".to_string(), "--".to_string(), "git".to_string()];
        let out = apply_run_mode_defaults_from_argv(parsed, &argv);

        assert!(matches!(out.output_format, OutputFormat::Text));
        assert!(matches!(out.preset, Some(Preset::Ai)));
    }

    #[test]
    fn apply_run_mode_defaults_respects_explicit_format_and_preset() {
        let parsed = CliArgs {
            mode: CliMode::Run,
            input: InputSource::Stdin,
            output: OutputTarget::Stdout,
            verbose: false,
            calc_tokens: false,
            reorder: false,
            semantic: false,
            normalize: false,
            ai_export: false,
            ai_signal: false,
            output_format: OutputFormat::Json,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            init_hooks: false,
            uninstall_hooks: false,
            hook_shell: None,
            dry_run: false,
            init: false,
            no_hooks: false,
            force: false,
            gain: false,
            gain_daily: false,
            gain_by_filter: false,
            gain_json: false,
            gain_days: 7,
            doctor: None,
            doctor_format: DoctorOutputFormat::Text,
            doctor_strict: false,
            inject: false,
            config: None,
            run_command: vec![],
            explain_route: false,
            explain_command: None,
            explain_fallback_gap: 0.15,
            explain_replay_out: None,
            preset: Some(Preset::Balanced),
            fix: false,
            safety: false,
            rewrite: None,
            discover: Vec::new(),
            inplace: false,
            backup: false,
            include: Vec::new(),
            exclude: Vec::new(),
        };

        let argv = vec![
            "tokenslim".to_string(),
            "--preset=balanced".to_string(),
            "--format=json".to_string(),
            "--".to_string(),
            "git".to_string(),
        ];
        let out = apply_run_mode_defaults_from_argv(parsed, &argv);

        assert!(matches!(out.output_format, OutputFormat::Json));
        assert!(matches!(out.preset, Some(Preset::Balanced)));
    }

    #[test]
    fn should_enable_vcs_ai_compact_for_log_with_preset_text() {
        assert!(should_enable_vcs_ai_compact(
            Some(VcsRunIntent::Log),
            OutputFormat::Text,
            Some(Preset::Ai)
        ));
        assert!(!should_enable_vcs_ai_compact(
            Some(VcsRunIntent::Log),
            OutputFormat::Json,
            Some(Preset::Ai)
        ));
        assert!(!should_enable_vcs_ai_compact(
            Some(VcsRunIntent::Log),
            OutputFormat::Text,
            None
        ));
    }

    #[test]
    fn final_paths_optimizer_skips_single_status_footer_but_runs_for_generic() {
        let text = "changes:\nM $P1/a.rs\npaths: $P1=src/core\n";
        assert!(!should_apply_final_paths_optimizer(
            Some(VcsRunIntent::Status),
            OutputFormat::Text,
            text
        ));
        assert!(!should_apply_final_paths_optimizer(
            Some(VcsRunIntent::Other),
            OutputFormat::Text,
            text
        ));
        assert!(should_apply_final_paths_optimizer(
            None,
            OutputFormat::Text,
            text
        ));
    }

    #[test]
    fn final_paths_optimizer_runs_for_single_log_footer_and_multi_footer() {
        let single = "commit abc\n$P1/a.rs\npaths: $P1=src/core\n";
        assert!(should_apply_final_paths_optimizer(
            Some(VcsRunIntent::Log),
            OutputFormat::Text,
            single
        ));

        let multi = "changes:\nM $P1/a.rs\npaths: $P1=src/core\nuntracked:\n?? $P2/b.rs\npaths: $P2=docs/design\n";
        assert!(should_apply_final_paths_optimizer(
            Some(VcsRunIntent::Status),
            OutputFormat::Text,
            multi
        ));
    }

    #[test]
    fn replace_paths_with_dict_resolves_nested_aliases() {
        let entries = vec![
            ("$P1".to_string(), "$P2/subdir".to_string()),
            ("$P2".to_string(), "root/parent".to_string()),
        ];
        let text = "M root/parent/subdir/file.rs\nM root/parent/other.rs\n";
        let result = replace_paths_with_dict(text, &entries);
        assert!(
            result.contains("$P1/file.rs"),
            "nested reference should resolve: {}",
            result
        );
        assert!(
            result.contains("$P2/other.rs"),
            "parent should be replaced: {}",
            result
        );
    }

    #[test]
    fn parse_path_dictionary_blocks_keeps_first_token_definition() {
        let text = "[paths] $P1=src/old; $P2=src/core\n[paths] $P1=src/new\nM src/core/a.rs\n";
        let (entries, body) = parse_path_dictionary_blocks(text);
        assert_eq!(entries[0], ("$P1".to_string(), "src/old".to_string()));
        assert_eq!(entries[1], ("$P2".to_string(), "src/core".to_string()));
        assert_eq!(body.trim(), "M src/core/a.rs");
    }

    #[test]
    fn merge_path_dictionary_blocks_returns_original_when_no_dict() {
        let text = "git status\nM src/core/a.rs\n";
        let result = merge_path_dictionary_blocks(text);
        assert_eq!(result, text);
    }

    #[test]
    fn merge_path_dict_replaces_all_paths() {
        // 模拟：第一段落回退了原始路径，第二段落使用了 $P 令牌
        let text = "[paths] $P3=src/core\nM src/core/file.rs\nM src/plugins/vcs_bzr/mod.rs\n\n[paths] $P6=src/plugins/vcs_bzr; $P15=src/plugins\nM $P6/test.rs\nM $P15/other.rs\n";
        let result = merge_path_dictionary_blocks(text);
        // 字典应合并置顶
        assert!(
            result.starts_with("[paths]"),
            "dict should be at top: {}",
            result
        );
        // 第一段的原始路径应被替换
        assert!(
            !result.contains("src/core/file.rs"),
            "raw path should be replaced: {}",
            result
        );
        assert!(
            !result.contains("src/plugins/vcs_bzr/mod.rs"),
            "raw path should be replaced: {}",
            result
        );
        // 应出现嵌套别名
        assert!(result.contains("$P3/"), "should use $P3 token");
        assert!(result.contains("$P6/"), "should use $P6 token");
    }

    #[test]
    fn prepend_run_command_anchor_for_vcs_output_without_command_header() {
        let combined = "On branch master\nnothing to commit, working tree clean\n";
        let out = prepend_run_command_anchor_if_needed(combined, "git", &["status".to_string()]);
        assert!(out.starts_with("git status\n"), "out={}", out);
    }

    #[test]
    fn prepend_run_command_anchor_for_generic_output_without_command_header() {
        let combined = "npm <command>\nUsage:\n";
        let out = prepend_run_command_anchor_if_needed(combined, "npm", &["-g".to_string()]);
        assert!(out.starts_with("npm -g\n"), "out={}", out);
    }

    #[test]
    fn prepend_run_command_anchor_is_idempotent_when_header_exists() {
        let combined = "npm -g\nnpm <command>\nUsage:\n";
        let out = prepend_run_command_anchor_if_needed(combined, "npm", &["-g".to_string()]);
        assert_eq!(out, combined);
    }

    #[test]
    fn prepend_run_command_anchor_recognizes_equivalent_whitespace_header() {
        let combined = "svn   update\nUpdated to revision 9.\n";
        let out = prepend_run_command_anchor_if_needed(combined, "svn", &["update".to_string()]);
        assert_eq!(out, combined);
    }

    #[test]
    fn build_run_command_anchor_quotes_special_argv_tokens() {
        let out = build_run_command_anchor(
            "C:\\Program Files\\Git\\cmd\\git.exe",
            &[
                "-C".to_string(),
                "C:\\tmp\\my repo".to_string(),
                "log".to_string(),
                "--grep=a b".to_string(),
            ],
        );
        assert_eq!(
            out,
            "\"C:\\\\Program Files\\\\Git\\\\cmd\\\\git.exe\" -C \"C:\\\\tmp\\\\my repo\" log \"--grep=a b\""
        );
    }

    #[test]
    fn merge_path_dict_keeps_command_line_first() {
        let text = "git status\n[paths] $P1=src/core\nM src/core/a.rs\n";
        let result = merge_path_dictionary_blocks(text);
        let mut lines = result.lines();
        assert_eq!(lines.next().unwrap_or_default(), "git status");
        assert!(
            lines.next().unwrap_or_default().starts_with("[paths] "),
            "result={}",
            result
        );
    }

    #[test]
    fn merge_path_dict_keeps_cloud_vcs_command_line_first() {
        let text = "gh pr list\n[paths] $P1=src/core\nM src/core/a.rs\n";
        let result = merge_path_dictionary_blocks(text);
        let mut lines = result.lines();
        assert_eq!(lines.next().unwrap_or_default(), "gh pr list");
        assert!(
            lines.next().unwrap_or_default().starts_with("[paths] "),
            "result={}",
            result
        );
    }

    #[test]
    fn add_common_parent_entries_promotes_shared_prefix_after_three_children() {
        let mut entries = vec![
            ("$P1".to_string(), "src/core/a.rs".to_string()),
            ("$P2".to_string(), "src/core/b.rs".to_string()),
            ("$P3".to_string(), "src/core/c.rs".to_string()),
        ];
        add_common_parent_entries(&mut entries);
        assert!(
            entries.iter().any(|(_, path)| path == "src/core"),
            "entries={entries:?}"
        );
    }

    #[test]
    fn format_run_mode_tokens_text_keeps_flattened_output() {
        let output = crate::core::compression::CompressionOutput {
            tokens: vec![crate::core::compression::Token::Text(
                "hello\n".to_string().into(),
            )],
            dictionary: crate::core::dictionary_engine::Dictionary::default(),
            metadata: crate::core::compression::CompressionMetadata::default(),
        };
        let formatted = format_run_mode_tokens(OutputFormat::Text, &output)
            .expect("text format should succeed");
        assert_eq!(formatted, "hello\n");
    }

    #[test]
    fn apply_run_mode_text_postprocessors_leaves_non_vcs_text_unchanged_without_paths_footer() {
        let options = crate::core::path_optimizer::methods::PathDictionaryOptions::default();
        let input = "plain output\n".to_string();
        let output =
            apply_run_mode_text_postprocessors(input.clone(), None, OutputFormat::Text, &options);
        assert_eq!(output, input);
    }

    #[test]
    fn merge_path_dict_places_dictionary_top_without_command_anchor() {
        let text = "[paths] $P1=src/core\nM src/core/a.rs\n";
        let result = merge_path_dictionary_blocks(text);
        assert!(
            result
                .lines()
                .next()
                .unwrap_or_default()
                .starts_with("[paths] "),
            "result={}",
            result
        );
    }

    #[test]
    fn merge_path_dict_builds_repeated_subdir_alias() {
        let text =
            "[paths] $P1=src/plugins\nM src/plugins/web_log/a.rs\nM src/plugins/web_log/b.rs\n";
        let result = merge_path_dictionary_blocks(text);
        assert!(result.starts_with("[paths] "), "result={}", result);
        assert!(
            result.contains("src/plugins/web_log") || result.contains("$P1/web_log"),
            "should contain derived subdir alias in dictionary: {}",
            result
        );
        assert!(
            result.contains("$P2/a.rs") || result.contains("$P3/a.rs"),
            "should use promoted subdir alias token for rewritten path: {}",
            result
        );
    }

    #[test]
    fn remove_hook_block_is_idempotent() {
        let content = format!("line1\n{}\nhello\n{}\nline2\n", HOOK_BEGIN, HOOK_END);
        let cleaned = remove_hook_block(&content);
        assert!(cleaned.contains("line1"));
        assert!(cleaned.contains("line2"));
        assert!(!cleaned.contains(HOOK_BEGIN));
        assert!(!cleaned.contains(HOOK_END));
    }
}

pub fn get_plugins() -> Vec<Box<dyn Plugin>> {
    use crate::plugins::android_gradle_plugin::AndroidGradlePlugin;
    use crate::plugins::ansi_cleaner_plugin::AnsiCleanerPlugin;
    use crate::plugins::ansible_plugin::AnsiblePlugin;
    use crate::plugins::artifact_summary_plugin::ArtifactSummaryPlugin;
    use crate::plugins::bazel_plugin::BazelPlugin;
    use crate::plugins::ci_log_plugin::CiLogPlugin;
    use crate::plugins::cloud_log_plugin::CloudLogPlugin;
    use crate::plugins::cloudformation_plugin::CloudFormationPlugin;
    use crate::plugins::db_log_plugin::DbLogPlugin;
    use crate::plugins::dotnet_plugin::DotNetPlugin;
    use crate::plugins::gcc_log_plugin::GccLogPlugin;
    use crate::plugins::generic_text_plugin::GenericTextPlugin;
    use crate::plugins::git_diff_plugin::GitDiffPlugin;
    use crate::plugins::helm_plugin::HelmPlugin;
    use crate::plugins::java_stack_plugin::JavaStackPlugin;
    use crate::plugins::json_plugin::JsonPlugin;
    use crate::plugins::kubernetes_docker_plugin::KubernetesDockerPlugin;
    use crate::plugins::markdown_plugin::MarkdownPlugin;
    use crate::plugins::maven_plugin::MavenPlugin;
    use crate::plugins::ndjson_plugin::NdjsonPlugin;
    use crate::plugins::node_error_plugin::NodeErrorPlugin;
    use crate::plugins::nodejs_plugin::NodeJsPlugin;
    use crate::plugins::noise_filter_plugin::NoiseFilterPlugin;
    use crate::plugins::php_ruby_plugin::PhpRubyPlugin;
    use crate::plugins::protobuf_plugin::ProtobufPlugin;
    use crate::plugins::pulumi_plugin::PulumiPlugin;
    use crate::plugins::pytest_plugin::PytestPlugin;
    use crate::plugins::python_traceback_plugin::PythonTracebackPlugin;
    use crate::plugins::rust_go_plugin::RustGoPlugin;
    use crate::plugins::shell_session_plugin::methods::ShellSessionPlugin;
    use crate::plugins::smart_code_plugin::SmartCodePlugin;
    use crate::plugins::smart_path_plugin::SmartPathPlugin;
    use crate::plugins::spring_boot_plugin::SpringBootPlugin;
    use crate::plugins::sql_plugin::SqlPlugin;
    use crate::plugins::static_rule_plugin::{SimpleRulePlugin, StaticRuleConfig};
    use crate::plugins::syslog_plugin::SyslogPlugin;
    use crate::plugins::template_driven_plugin::types::{TemplateConfig, TemplateDrivenPlugin};
    use crate::plugins::terraform_plugin::TerraformPlugin;
    use crate::plugins::unity_unreal_plugin::UnityUnrealPlugin;
    use crate::plugins::vcs_plugin::VcsPlugin;
    use crate::plugins::web_log_plugin::WebLogPlugin;
    use crate::plugins::webpack_vite_plugin::WebpackVitePlugin;
    use crate::plugins::xcode_log_plugin::XcodeLogPlugin;
    use crate::plugins::xml_html_plugin::XmlHtmlPlugin;
    use crate::plugins::yaml_plugin::YamlPlugin;

    let mut plugins: Vec<Box<dyn Plugin>> = Vec::new();
    plugins.push(Box::new(AndroidGradlePlugin::new()));
    plugins.push(Box::new(AnsiblePlugin::new()));
    plugins.push(Box::new(AnsiCleanerPlugin::new()));
    plugins.push(Box::new(ArtifactSummaryPlugin::new()));
    plugins.push(Box::new(BazelPlugin::new()));
    plugins.push(Box::new(CloudLogPlugin::new()));
    plugins.push(Box::new(CloudFormationPlugin::new()));
    plugins.push(Box::new(CiLogPlugin::new()));
    plugins.push(Box::new(DbLogPlugin::new()));
    plugins.push(Box::new(DotNetPlugin::new()));
    plugins.push(Box::new(GccLogPlugin::new()));
    plugins.push(Box::new(HelmPlugin::new()));
    plugins.push(Box::new(JavaStackPlugin::new()));
    plugins.push(Box::new(JsonPlugin::new()));
    plugins.push(Box::new(KubernetesDockerPlugin::new()));
    plugins.push(Box::new(NdjsonPlugin::new()));
    plugins.push(Box::new(MarkdownPlugin::new()));
    plugins.push(Box::new(MavenPlugin::new()));
    plugins.push(Box::new(NodeErrorPlugin::new()));
    plugins.push(Box::new(NodeJsPlugin::new()));
    plugins.push(Box::new(NoiseFilterPlugin::new()));
    plugins.push(Box::new(GenericTextPlugin::new()));
    plugins.push(Box::new(PhpRubyPlugin::new()));
    plugins.push(Box::new(ProtobufPlugin::new()));
    plugins.push(Box::new(PulumiPlugin::new()));
    plugins.push(Box::new(PytestPlugin::new()));
    plugins.push(Box::new(PythonTracebackPlugin::new()));
    plugins.push(Box::new(RustGoPlugin::new()));
    plugins.push(Box::new(ShellSessionPlugin::default()));
    plugins.push(Box::new(SmartCodePlugin::new()));
    plugins.push(Box::new(SmartPathPlugin::new()));
    plugins.push(Box::new(SpringBootPlugin::new()));
    plugins.push(Box::new(SqlPlugin::new()));
    plugins.push(Box::new(SimpleRulePlugin::new(StaticRuleConfig::default())));
    plugins.push(Box::new(SyslogPlugin::new()));
    plugins.push(Box::new(TerraformPlugin::new()));
    plugins.push(Box::new(UnityUnrealPlugin::new()));
    plugins.push(Box::new(WebLogPlugin::new()));
    plugins.push(Box::new(XcodeLogPlugin::new()));
    plugins.push(Box::new(WebpackVitePlugin::new()));
    plugins.push(Box::new(XmlHtmlPlugin::new()));
    plugins.push(Box::new(VcsPlugin::new()));
    plugins.push(Box::new(GitDiffPlugin::new()));
    plugins.push(Box::new(TemplateDrivenPlugin::new(
        TemplateConfig::default(),
    )));
    plugins.push(Box::new(YamlPlugin::new()));

    plugins
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrePipelineAction {
    Continue,
    Inject,
    DoctorEncoding,
    DoctorWorkspace,
    DoctorRule,
    DoctorEnv,
    Rewrite,
    Discover,
    Gain,
    Init,
    Hooks,
    HooksStatus,
    VerifyRule,
    ExplainPlugin,
    Plugins,
    RepairFile,
}

fn select_pre_pipeline_action(args: &CliArgs) -> PrePipelineAction {
    if args.inject {
        return PrePipelineAction::Inject;
    }
    if let Some(crate::cli::types::DoctorKind::Encoding) = args.doctor {
        return PrePipelineAction::DoctorEncoding;
    }
    if let Some(crate::cli::types::DoctorKind::Workspace) = args.doctor {
        return PrePipelineAction::DoctorWorkspace;
    }
    if let Some(crate::cli::types::DoctorKind::Rule) = args.doctor {
        return PrePipelineAction::DoctorRule;
    }
    if let Some(crate::cli::types::DoctorKind::Env) = args.doctor {
        return PrePipelineAction::DoctorEnv;
    }
    if args.rewrite.is_some() {
        return PrePipelineAction::Rewrite;
    }
    if !args.discover.is_empty() {
        return PrePipelineAction::Discover;
    }
    if args.gain {
        return PrePipelineAction::Gain;
    }
    if matches!(args.mode, CliMode::Init) || args.init {
        return PrePipelineAction::Init;
    }
    if args.init_hooks || args.uninstall_hooks {
        return PrePipelineAction::Hooks;
    }
    if matches!(args.mode, CliMode::HooksStatus) {
        return PrePipelineAction::HooksStatus;
    }

    if args.verify_rule.is_some() && args.verify_fixture.is_some() && args.verify_expected.is_some()
    {
        return PrePipelineAction::VerifyRule;
    }
    if matches!(args.mode, CliMode::ExplainPlugin) {
        return PrePipelineAction::ExplainPlugin;
    }
    if matches!(args.mode, CliMode::Plugins) {
        return PrePipelineAction::Plugins;
    }
    if matches!(args.mode, CliMode::RepairFile) {
        return PrePipelineAction::RepairFile;
    }
    PrePipelineAction::Continue
}

fn should_show_compress_quick_usage(
    launched_without_args: bool,
    is_stdin_input: bool,
    input_text: &str,
) -> bool {
    launched_without_args && is_stdin_input && input_text.trim().is_empty()
}

fn handle_inject_action(args: &CliArgs) -> Result<bool, CliError> {
    match args.doctor {
        Some(crate::cli::types::DoctorKind::Encoding)
        | Some(crate::cli::types::DoctorKind::Rule)
        | Some(crate::cli::types::DoctorKind::Env) => {
            return Err(CliError::InvalidArgs(format_invalid_args_message(
                "E_CLI_INJECT_SCOPE",
                "`--inject` 仅支持与 `workspace` 诊断配合，或单独使用。",
                "`--inject` only supports `workspace` diagnostics or standalone usage.",
                Some("示例: tokenslim workspace --inject".to_string()),
                Some("Example: tokenslim workspace --inject".to_string()),
            )));
        }
        _ => {}
    }

    use crate::core::doctor_workspace::inject_context_file;
    let result = inject_context_file(args.dry_run).map_err(CliError::Config)?;
    println!("{}", result);
    Ok(true)
}

fn handle_doctor_encoding_action(args: &CliArgs) -> Result<bool, CliError> {
    use crate::core::doctor_encoding::{
        generate_fix_commands, run_encoding_doctor, DoctorReportFormat,
    };

    if args.fix {
        let fix_output = generate_fix_commands().map_err(|e| CliError::Config(e))?;
        println!("{}", fix_output);
        return Ok(true);
    }

    let format = match args.doctor_format {
        crate::cli::types::DoctorOutputFormat::Text => DoctorReportFormat::Text,
        crate::cli::types::DoctorOutputFormat::Json => DoctorReportFormat::Json,
        _ => {
            return Err(CliError::InvalidArgs(
                "doctor encoding supports only --doctor-format text|json".to_string(),
            ));
        }
    };

    let report = run_encoding_doctor(format).map_err(|e| CliError::Config(e))?;
    println!("{}", report);
    Ok(true)
}

fn handle_doctor_workspace_action(args: &CliArgs) -> Result<bool, CliError> {
    use crate::core::doctor_workspace::{run_workspace_doctor, WorkspaceReportFormat};

    let format = match args.doctor_format {
        crate::cli::types::DoctorOutputFormat::Text => WorkspaceReportFormat::Text,
        crate::cli::types::DoctorOutputFormat::Json => WorkspaceReportFormat::Json,
        crate::cli::types::DoctorOutputFormat::Llm => WorkspaceReportFormat::Llm,
        crate::cli::types::DoctorOutputFormat::JsonMin => WorkspaceReportFormat::JsonMin,
    };

    let report = run_workspace_doctor(format, args.doctor_strict).map_err(CliError::Config)?;
    println!("{}", report);
    Ok(true)
}

fn handle_doctor_rule_action(args: &CliArgs) -> Result<bool, CliError> {
    use crate::core::rule_diagnosis::{diagnose, render_diagnosis_text};
    let cwd =
        std::env::current_dir().map_err(|e| CliError::Config(format!("Cannot get cwd: {}", e)))?;
    let rule_files = ["config/plugins.toml", "rules.toml", ".tokenslim.toml"];
    let mut found = false;
    for rule_file in &rule_files {
        let path = cwd.join(rule_file);
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(config) =
                    toml::from_str::<crate::plugins::static_rule_plugin::StaticRuleConfig>(&content)
                {
                    let diagnosis = diagnose(&config, rule_file);
                    let output = match args.doctor_format {
                        crate::cli::types::DoctorOutputFormat::Json => {
                            serde_json::to_string_pretty(&diagnosis)
                                .map_err(|e| CliError::Config(format!("JSON error: {}", e)))?
                        }
                        _ => render_diagnosis_text(&diagnosis),
                    };
                    println!("{}", output);
                    found = true;
                }
            }
        }
    }
    if !found {
        println!(
            "No rule configuration found. Searched: {}",
            rule_files.join(", ")
        );
    }
    Ok(true)
}

fn handle_doctor_env_action(args: &CliArgs) -> Result<bool, CliError> {
    use crate::core::sys_env::get_environment_info;
    let env = get_environment_info();
    let output = match args.doctor_format {
        crate::cli::types::DoctorOutputFormat::Json => {
            let json = serde_json::json!({
                "os": env.os,
                "os_version": env.os_version,
                "locale": env.locale,
                "file_systems": env.file_systems,
            });
            serde_json::to_string_pretty(&json)
                .map_err(|e| CliError::Config(format!("JSON error: {}", e)))?
        }
        _ => {
            format!(
                "System Environment Report\n========================\n\nOS: {}\nVersion: {}\nLocale: {}\nFile Systems: {}\n",
                env.os, env.os_version, env.locale, env.file_systems.join(", ")
            )
        }
    };
    println!("{}", output);
    Ok(true)
}

fn handle_discover_action(args: &CliArgs) -> Result<bool, CliError> {
    let tracker =
        crate::core::tracking::Tracker::open_default().map_err(|e| CliError::Config(e))?;

    let result = crate::core::filter_discover::discover_filters(&args.discover, &tracker)
        .map_err(|e| CliError::Config(e))?;

    println!("{}", t("discover_result_header"));
    println!("{}", t1("discover_total_commands", result.total_commands));
    println!(
        "{}",
        t1(
            "discover_total_potential_savings",
            result.total_potential_savings
        )
    );

    if !result.filterable.is_empty() {
        println!(
            "{}",
            t1("discover_filterable_groups", result.filterable.len())
        );
        for group in &result.filterable {
            println!("{}", t2("discover_group_line", &group.key, group.count));
            println!(
                "{}",
                t1("discover_output_tokens", group.total_output_tokens)
            );
            if let Some(pct) = group.estimated_savings_pct {
                println!(
                    "{}",
                    t("discover_estimated_savings_pct").replace("{:.1}", &format!("{pct:.1}"))
                );
            }
            if let Some(saved) = group.estimated_tokens_saved {
                println!("{}", t1("discover_estimated_savings_tokens", saved));
            }
            println!();
        }
    }

    if !result.no_filter.is_empty() {
        println!(
            "{}",
            t1("discover_no_filter_groups", result.no_filter.len())
        );
        for group in &result.no_filter {
            println!("{}", t2("discover_group_line", &group.key, group.count));
            println!(
                "{}",
                t1("discover_output_tokens", group.total_output_tokens)
            );
            if let Some(saved) = group.estimated_tokens_saved {
                println!("{}", t1("discover_estimated_savings_tokens_default", saved));
            }
            println!();
        }
    }

    if !result.already_filtered.is_empty() {
        println!(
            "{}",
            t1(
                "discover_already_filtered_groups",
                result.already_filtered.len()
            )
        );
        for group in &result.already_filtered {
            println!("{}", t2("discover_group_line", &group.key, group.count));
        }
    }

    Ok(true)
}

fn handle_gain_action(args: &CliArgs) -> Result<bool, CliError> {
    if args.gain_json {
        let result = if args.gain_daily {
            crate::core::tracking::gain::render_gain_daily_json(args.gain_days)
        } else if args.gain_by_filter {
            crate::core::tracking::gain::render_gain_by_filter_json()
        } else {
            crate::core::tracking::gain::render_gain_json()
        };
        match result {
            Ok(json) => println!("{}", json),
            Err(err) => return Err(CliError::Config(err)),
        }
        return Ok(true);
    }
    let report = if args.gain_daily {
        crate::core::tracking::gain::render_gain_report_daily(args.gain_days)
    } else if args.gain_by_filter {
        crate::core::tracking::gain::render_gain_report_by_filter()
    } else {
        crate::core::tracking::gain::render_gain_report_summary()
    };
    println!("{}", report);
    Ok(true)
}

fn handle_verify_rule_action(args: &CliArgs) -> Result<bool, CliError> {
    if let (Some(rule), Some(fixture), Some(expected)) = (
        args.verify_rule.as_deref(),
        args.verify_fixture.as_deref(),
        args.verify_expected.as_deref(),
    ) {
        run_static_rule_verify(rule, fixture, expected, args.safety)?;
        return Ok(true);
    }
    Ok(false)
}

fn handle_explain_plugin_action(args: &CliArgs) -> Result<bool, CliError> {
    let (mut raw_report, input_kind, replay_input) =
        if let Some(command_line) = args.explain_command.as_deref() {
            (
                explain_plugin_for_command_line(command_line, args),
                "command".to_string(),
                command_line.to_string(),
            )
        } else {
            let input_text = read_explain_input_text(&args.input)?;
            (
                explain_plugin_for_log_text(&input_text, args.explain_fallback_gap),
                "log".to_string(),
                input_text,
            )
        };

    if let Some(path) = args.explain_replay_out.as_deref() {
        write_explain_replay_template(path, &input_kind, &replay_input, &raw_report)?;
        raw_report.push_str(&format!(
            "replay_case_template_path={}\n",
            path.to_string_lossy()
        ));
    }
    let report = render_explain_report_by_format(&raw_report, &args.output_format)?;

    match &args.output {
        OutputTarget::File(path) => std::fs::write(path, report).map_err(CliError::Io)?,
        OutputTarget::Stdout => println!("{}", report),
    }
    Ok(true)
}

fn handle_pre_pipeline_action(args: &CliArgs) -> Result<bool, CliError> {
    match select_pre_pipeline_action(args) {
        PrePipelineAction::Continue => Ok(false),
        PrePipelineAction::Inject => handle_inject_action(args),
        PrePipelineAction::DoctorEncoding => handle_doctor_encoding_action(args),
        PrePipelineAction::DoctorWorkspace => handle_doctor_workspace_action(args),
        PrePipelineAction::DoctorRule => handle_doctor_rule_action(args),
        PrePipelineAction::DoctorEnv => handle_doctor_env_action(args),
        PrePipelineAction::Rewrite => {
            if let Some(ref command) = args.rewrite {
                let config = crate::core::rewrite::load_user_config();
                let rewritten = crate::core::rewrite::rewrite_command(command, &config);
                println!("{}", rewritten);
                return Ok(true);
            }
            Ok(false)
        }
        PrePipelineAction::Discover => handle_discover_action(args),
        PrePipelineAction::Gain => handle_gain_action(args),
        PrePipelineAction::Init => {
            use crate::core::init_command::{print_init_summary, run_init, InitOptions};
            let options = InitOptions {
                install_hooks: !args.no_hooks,
                hook_shell: args.hook_shell.map(|s| s.as_str().to_string()),
                dry_run: args.dry_run,
                force: args.force,
            };
            let result = run_init(options).map_err(CliError::Config)?;
            print_init_summary(&result);
            Ok(true)
        }
        PrePipelineAction::Hooks => {
            let shell = args.hook_shell.unwrap_or_else(detect_shell);
            if args.init_hooks {
                install_hooks(shell, args.dry_run)?;
            } else {
                uninstall_hooks(shell, args.dry_run)?;
            }
            Ok(true)
        }
        PrePipelineAction::HooksStatus => {
            let shell = args.hook_shell.unwrap_or_else(detect_shell);
            check_hooks_status(shell)?;
            Ok(true)
        }

        PrePipelineAction::VerifyRule => handle_verify_rule_action(args),
        PrePipelineAction::ExplainPlugin => handle_explain_plugin_action(args),
        PrePipelineAction::Plugins => {
            crate::core::doctor_workspace::methods::run_plugins_mode();
            Ok(true)
        }
        PrePipelineAction::RepairFile => {
            run_repair_file_command(args)?;
            Ok(true)
        }
    }
}

fn resolve_plugins_for_args(args: &CliArgs) -> Vec<Box<dyn Plugin>> {
    if matches!(args.mode, CliMode::Run) {
        if let Some(prog) = args.run_command.first() {
            return plugins_for_run_command(prog, &args.run_command[1..]);
        }
    }
    get_plugins()
}

fn build_pipeline_config_for_args(args: &CliArgs) -> PipelineConfig {
    let mut pipeline_config = PipelineConfig::default();
    pipeline_config.dispatcher_config.fallback_plugin = "git_diff".to_string();

    // Apply preset configuration
    if let Some(preset) = args.preset {
        match preset {
            crate::cli::types::Preset::Fast => {
                // Speed priority: disable heavy features, reduce thresholds
                pipeline_config.reorder_config.enabled = false;
                pipeline_config.dispatcher_config.enable_semantic_fallback = false;
                pipeline_config.dictionary_threshold = 50; // Lower threshold for small files
                pipeline_config.dedup_config.pattern_threshold = 5; // Less aggressive pattern dedup
            }
            crate::cli::types::Preset::Balanced => {
                // Default behavior (already set by PipelineConfig::default())
                pipeline_config.reorder_config.enabled = args.reorder;
            }
            crate::cli::types::Preset::Ai => {
                // Signal priority: enable all semantic features, keep more context
                pipeline_config.reorder_config.enabled = true;
                pipeline_config.dispatcher_config.enable_semantic_fallback = true;
                pipeline_config.dictionary_threshold = 0; // Always use dictionary for max context
                pipeline_config.dedup_config.pattern_threshold = 2; // More aggressive dedup
            }
        }
    } else if args.reorder {
        pipeline_config.reorder_config.enabled = true;
    }

    // Run 模式以可读性为先：保留空行分隔，避免帮助文本段落粘连。
    if matches!(args.mode, CliMode::Run) {
        pipeline_config.slicer_config.skip_empty_lines = false;
    }

    pipeline_config
}

fn run_compress_mode(
    args: &CliArgs,
    pipeline: &mut CompressionPipeline,
    launched_without_args: bool,
    program: &str,
) -> Result<(), CliError> {
    let (input_text, is_stdin_input) = read_compress_input(&args.input)?;

    if should_show_compress_quick_usage(launched_without_args, is_stdin_input, &input_text) {
        println!("{}", render_global_usage(program));
        return Ok(());
    }

    let output = pipeline
        .compress_str(&input_text)
        .map_err(|e| CliError::Pipeline(e))?;

    // 统一到 tracking：compress 模式也进入同一统计账本
    record_tracking_event("tokenslim compress", Some("pipeline_compress"), &output, 0);

    let formatted = format_compress_output(&output)?;

    match &args.output {
        OutputTarget::File(path) => std::fs::write(path, formatted).map_err(|e| CliError::Io(e))?,
        OutputTarget::Stdout => println!("{}", formatted),
    }
    Ok(())
}

fn read_compress_input(input: &InputSource) -> Result<(String, bool), CliError> {
    match input {
        InputSource::File(path) => {
            let bytes = std::fs::read(path).map_err(CliError::Io)?;
            Ok((String::from_utf8_lossy(&bytes).into_owned(), false))
        }
        InputSource::Stdin => {
            let mut buffer = Vec::new();
            io::stdin().read_to_end(&mut buffer).map_err(CliError::Io)?;
            Ok((String::from_utf8_lossy(&buffer).into_owned(), true))
        }
    }
}

fn format_compress_output(
    output: &crate::core::compression::CompressionOutput,
) -> Result<String, CliError> {
    serde_json::to_string_pretty(output).map_err(CliError::Serialization)
}

fn run_decompress_mode(args: &CliArgs) -> Result<(), CliError> {
    let input_text = match &args.input {
        InputSource::File(path) => std::fs::read_to_string(path).map_err(|e| CliError::Io(e))?,
        InputSource::Stdin => {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|e| CliError::Io(e))?;
            buffer
        }
    };

    let output: crate::core::compression::CompressionOutput =
        serde_json::from_str(&input_text).map_err(|e| CliError::Serialization(e))?;

    let rehydrator = crate::core::rehydration_pipeline::RehydrationPipeline::new(
        output.dictionary.clone(),
        get_plugins(),
        crate::core::rehydration_pipeline::RehydrationConfig::default(),
    );

    let decompressed = if args.ai_signal {
        rehydrator
            .rehydrate_for_ai(&output)
            .map_err(|e| CliError::Decompression(e.to_string()))?
    } else if args.ai_export {
        let mut result = String::new();
        result.push_str("========== TokenSlim AI Export Context ==========\n");

        // Export Structural Directories for AI Context
        result.push_str("[Directories]\n");
        let mut dirs: Vec<_> = output.dictionary.directories.iter().collect();
        dirs.sort_by_key(|(k, _)| k[2..].parse::<usize>().unwrap_or(0));
        for (k, v) in dirs {
            result.push_str(&format!("{}: {}\n", k, v));
        }

        result.push_str("\n[Semantic Logs]\n");
        let rehydrated = rehydrator
            .rehydrate_for_ai(&output)
            .map_err(|e| CliError::Decompression(e.to_string()))?;
        result.push_str(&rehydrated);

        result
    } else {
        rehydrator
            .rehydrate(&output)
            .map_err(|e| CliError::Decompression(e.to_string()))?
    };

    match &args.output {
        OutputTarget::File(path) => {
            std::fs::write(path, decompressed).map_err(|e| CliError::Io(e))?
        }
        OutputTarget::Stdout => println!("{}", decompressed),
    }
    Ok(())
}

struct RunModeCompressionContext {
    vcs_intent: Option<VcsRunIntent>,
    path_options: crate::core::path_optimizer::methods::PathDictionaryOptions,
    enable_vcs_ai_compact: bool,
    profile: crate::plugins::vcs_plugin::methods::VcsAiProfile,
    run_input: String,
}

fn resolve_run_path_preset(
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

fn resolve_vcs_ai_profile(
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

fn build_run_mode_compression_context(
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

fn compress_run_mode_text(
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

fn build_run_command_string(prog: &str, cmd_args: &[String]) -> String {
    if cmd_args.is_empty() {
        prog.to_string()
    } else {
        format!("{} {}", prog, cmd_args.join(" "))
    }
}

fn resolve_run_filter_name(
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

fn render_run_mode_output(
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

fn format_run_mode_tokens(
    output_format: OutputFormat,
    output: &crate::core::compression::CompressionOutput,
) -> Result<String, CliError> {
    match output_format {
        OutputFormat::Json => serde_json::to_string_pretty(output).map_err(CliError::Serialization),
        OutputFormat::Markdown | OutputFormat::Text => Ok(flatten_tokens(&output.tokens)),
    }
}

fn apply_run_mode_text_postprocessors(
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

fn run_run_mode(
    args: &CliArgs,
    pipeline: &mut CompressionPipeline,
    program: &str,
) -> Result<(), CliError> {
    let (prog, cmd_args) = parse_run_target(program, &args.run_command)?;

    if args.explain_route {
        println!("{}", explain_run_route(prog, cmd_args, args));
        return Ok(());
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

    match &args.output {
        OutputTarget::File(path) => std::fs::write(path, formatted).map_err(|e| CliError::Io(e))?,
        OutputTarget::Stdout => println!("{}", formatted),
    }

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

pub fn run_cli() -> Result<(), CliError> {
    let argv: Vec<String> = std::env::args().collect();
    let launched_without_args = argv.len() <= 1;
    let program = argv
        .first()
        .map(|s| program_name_from_argv0(s))
        .unwrap_or_else(|| "tokenslim".to_string());

    if let Some(help_text) = intercept_help_request(&argv, &program) {
        println!("{}", help_text);
        return Ok(());
    }

    if should_show_quick_usage(&argv, io::stdin().is_terminal()) {
        // 空参数交互调用时，直接给出版本和用法，避免等待 stdin 造成“卡住”体感。
        println!("{}", render_global_usage(&program));
        return Ok(());
    }

    let args = CliArgs::parse_args()?;
    if handle_pre_pipeline_action(&args)? {
        return Ok(());
    }

    let plugins = resolve_plugins_for_args(&args);
    let pipeline_config = build_pipeline_config_for_args(&args);

    let mut pipeline = CompressionPipeline::new(
        pipeline_config,
        plugins,
        MetricsCollector::new(MetricsConfig::default()),
    );

    match args.mode {
        CliMode::Compress => {
            run_compress_mode(&args, &mut pipeline, launched_without_args, &program)?
        }
        CliMode::Decompress => run_decompress_mode(&args)?,
        CliMode::Run => run_run_mode(&args, &mut pipeline, &program)?,
        CliMode::Init => unreachable!("init mode should return before pipeline execution"),
        CliMode::HooksStatus => {
            unreachable!("hooks-status mode should return before pipeline execution")
        }
        CliMode::ExplainPlugin => {
            unreachable!("explain-plugin mode should return before pipeline execution")
        }
        CliMode::Plugins => {
            unreachable!("plugins mode should return before pipeline execution")
        }
        CliMode::RepairFile => {
            unreachable!("repair-file mode should return before pipeline execution")
        }
    }
    Ok(())
}
