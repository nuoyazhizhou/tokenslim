//! cli 应用入口与参数解析

use crate::cli::commands::{
    benchmark::handle_verify_rule_action,
    compress::run_compress_mode,
    config::{
        check_hooks_status, detect_shell, handle_gain_action, handle_inject_action,
        install_hooks, parse_optional_hook_shell, uninstall_hooks,
    },
    decompress::run_decompress_mode,
    doctor::{
        handle_doctor_encoding_action, handle_doctor_env_action, handle_doctor_rule_action,
        handle_doctor_workspace_action,
    },
    export::{handle_discover_action, handle_explain_plugin_action},
    repair::{default_repair_output_path_from_input_arg, run_repair_file_command},
    run::{plugins_for_run_command, run_run_mode},
};
use crate::cli::common::*;
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
use clap::Parser;
use serde::Serialize;
use serde_json::{json, Value};
use std::borrow::Cow;
use std::io::{self, IsTerminal, Read};


pub(crate) fn find_cmd_index(args: &[String]) -> Option<usize> {
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


pub(crate) fn maybe_parse_run_subcommand_from_argv(args: &[String]) -> Option<Vec<String>> {
    let cmd_index = find_cmd_index(args)?;

    if args[cmd_index].eq_ignore_ascii_case("run") {
        return Some(args[cmd_index + 1..].to_vec());
    }

    None
}


pub(crate) fn is_tokenslim_builtin_command(cmd: &str) -> bool {
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
            | "config"
            | "serve-static"
            | "serve_static"
    )
}


pub(crate) fn maybe_parse_implicit_run_command_from_argv(args: &[String]) -> Option<Vec<String>> {
    let cmd_index = find_cmd_index(args)?;
    let cmd = &args[cmd_index];

    if is_tokenslim_builtin_command(cmd) {
        return None;
    }

    Some(args[cmd_index..].to_vec())
}


pub(crate) fn program_name_from_argv0(argv0: &str) -> String {
    std::path::Path::new(argv0)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("tokenslim")
        .to_string()
}


pub(crate) fn should_show_quick_usage(argv: &[String], stdin_is_terminal: bool) -> bool {
    if argv.len() == 2 && (argv[1] == "--help" || argv[1] == "-h") {
        return true;
    }
    // `-v` / `--verbose` 单独使用且无 position args 时，不应进入 pipeline 阻塞 stdin
    if argv.len() == 2 && (argv[1] == "-v" || argv[1] == "--verbose") {
        return true;
    }
    argv.len() <= 1 && stdin_is_terminal
}


pub(crate) fn render_config_usage(program: &str) -> String {
    format!(
        "{} config

管理全局与项目级配置项

{}:
  {} config <subcommand> [args...]

子命令:
  set <key> <value> [--global|-g]   设置配置项的值
  get <key>                         获取合并后的配置项值
  list                              列出所有合并生效的配置项
  unset <key> [--global|-g]         移除配置项
  reset [--global|-g]               重置/清空配置文件
  wizard [--global|-g]              进入交互式配置向导
  plugin <subcommand> [args...]     管理插件启用/禁用和参数配置

{}:
  {} config set general.preset fast --global
  {} config get general.preset
  {} config list
  {} config wizard",
        program,
        t("cli_help_usage"),
        program,
        t("cli_help_examples"),
        program,
        program,
        program,
        program
    )
}


pub(crate) fn render_global_usage(program: &str) -> String {
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
        "config",
        "管理全局与项目级配置项 / Manage configurations",
        "serve-static",
        "启动静态文件服务器 / Start a static file server",
        t("cli_help_common_examples"),
        capability_summary
    )
}


pub(crate) fn render_serve_static_usage(program: &str) -> String {
    format!(
        "{} serve-static

启动静态文件服务以托管指定目录下的文件。

用法:
  {} serve-static [目录路径] [选项]

参数:
  [目录路径]                          静态服务根目录（默认：当前目录 \".\"）

选项:
  --port <PORT>                      绑定的端口（默认：8080）
  --bind <IP>                        绑定的 IP 地址（默认：127.0.0.1）
  --open                             是否在启动后自动打开浏览器
  -h, --help                         显示此帮助信息

示例:
  {} serve-static                      # 托管当前目录，绑定到 127.0.0.1:8080
  {} serve-static ./dist --port 9000   # 托管 ./dist 目录，绑定到端口 9000
  {} serve-static --open               # 托管当前目录并自动打开浏览器",
        program,
        program,
        program,
        program,
        program
    )
}


pub(crate) fn intercept_version_request(argv: &[String]) -> Option<String> {
    // 跳过 argv[0] 和全局开关，只看 position args
    const GLOBAL_FLAGS: &[&str] = &["--dry-run", "-v", "--verbose"];
    let position_args: Vec<&String> = argv
        .iter()
        .skip(1)
        .filter(|a| !GLOBAL_FLAGS.contains(&a.as_str()))
        .collect();
    if position_args.len() != 1 {
        return None;
    }
    let first = position_args[0];
    if first == "-V" || first == "--version" || first.eq_ignore_ascii_case("version") {
        let program = argv
            .first()
            .map(|s| program_name_from_argv0(s))
            .unwrap_or_else(|| "tokenslim".to_string());
        return Some(format!("{} {}", program, env!("CARGO_PKG_VERSION")));
    }
    None
}


pub(crate) fn intercept_help_request(argv: &[String], program: &str) -> Option<String> {
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
        Some("config") => render_config_usage(program),
        Some("serve-static") | Some("serve_static") => render_serve_static_usage(program),
        _ => render_global_usage(program),
    };

    Some(help_text)
}


pub(crate) fn render_compress_usage(program: &str) -> String {
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
  {:<23} {}
  {:<23} {}
  {:<23} {}

{}:
  {} compress -i input.log -o output.json --format json
  {} compress --stream",
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
        "--stream",
        t("cli_opt_stream"),
        "--flush-interval <MS>",
        t("cli_opt_flush_interval"),
        "--merge",
        t("cli_opt_merge"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program,
        program
    )
}


pub(crate) fn render_decompress_usage(program: &str) -> String {
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


pub(crate) fn render_repair_file_usage(program: &str) -> String {
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


pub(crate) fn render_run_usage(program: &str) -> String {
    format!(
        "{} run

{}

{}:
  {} run {}

{}:
  {:<23} {}
  {:<23} {}
  {:<23} {}
  {:<23} {}

{}:
  {} run cargo test
  {} run git status
  {} run --stream -- cargo test",
        program,
        t("cli_desc_run"),
        t("cli_help_usage"),
        program,
        t("cli_run_external_command"),
        t("cli_help_options"),
        "--stream",
        t("cli_opt_stream"),
        "--flush-interval <MS>",
        t("cli_opt_flush_interval"),
        "--merge",
        t("cli_opt_merge"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program,
        program,
        program
    )
}


pub(crate) fn render_hooks_usage(program: &str) -> String {
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


pub(crate) fn render_workspace_usage(program: &str) -> String {
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


pub(crate) fn render_encoding_usage(program: &str) -> String {
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


pub(crate) fn render_rule_usage(program: &str) -> String {
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


pub(crate) fn render_env_usage(program: &str) -> String {
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


pub(crate) fn render_gain_usage(program: &str) -> String {
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


pub(crate) fn render_plugins_usage(program: &str) -> String {
    format!(
        "{}:\n  {} plugins\n\n{}:\n  {} plugins [{}...]\n",
        t("cli_help_usage"),
        program,
        t("cli_help_examples"),
        program,
        t("cli_help_options_placeholder")
    )
}


pub(crate) fn render_explain_plugin_usage(program: &str) -> String {
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


pub(crate) fn render_init_usage(program: &str) -> String {
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


pub(crate) fn split_run_explain_route_flag(run_cmd: Vec<String>) -> (bool, Vec<String>) {
    let mut explain_route = false;
    let mut remain = Vec::new();
    let mut i = 0;
    while i < run_cmd.len() {
        let arg = &run_cmd[i];
        if arg == "--explain-route" {
            explain_route = true;
            i += 1;
            // 如果紧跟在其后的是 "--"，也一并剥离
            if i < run_cmd.len() && run_cmd[i] == "--" {
                i += 1;
            }
        } else if arg == "--" {
            // 遇到 "--" 标志，说明后面的都是实际命令参数，不再剥离
            remain.extend(run_cmd[i..].to_vec());
            break;
        } else {
            remain.push(arg.clone());
            i += 1;
        }
    }
    (explain_route, remain)
}


pub(crate) fn render_run_command_hint(program: &str) -> String {
    format!(
        "运行模式需要外部命令。\n示例:\n  {program} run git status\n  {program} git status\n\nRun mode expects an external command.\nExamples:\n  {program} run git status\n  {program} git status"
    )
}


pub(crate) fn apply_run_mode_defaults_from_argv(mut parsed: CliArgs, argv: &[String]) -> CliArgs {
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


pub(crate) fn rewrite_command_alias_to_flags(args: &[String]) -> Result<Option<Vec<String>>, CliError> {
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
        "config" => {
            rewritten.push("--mode".to_string());
            rewritten.push("config".to_string());
            for arg in rest {
                rewritten.push("--config-args".to_string());
                rewritten.push(arg);
            }
            Ok(Some(rewritten))
        }
        "serve-static" | "serve_static" => {
            rewritten.push("--mode".to_string());
            rewritten.push("serve-static".to_string());
            let mut i = 0;
            let mut has_dir = false;
            while i < rest.len() {
                let arg = &rest[i];
                if arg == "--port" {
                    rewritten.push("--serve-port".to_string());
                    if i + 1 < rest.len() {
                        rewritten.push(rest[i + 1].clone());
                        i += 2;
                    } else {
                        i += 1;
                    }
                } else if arg.starts_with("--port=") {
                    let val = arg.strip_prefix("--port=").unwrap();
                    rewritten.push(format!("--serve-port={}", val));
                    i += 1;
                } else if arg == "--bind" {
                    rewritten.push("--serve-bind".to_string());
                    if i + 1 < rest.len() {
                        rewritten.push(rest[i + 1].clone());
                        i += 2;
                    } else {
                        i += 1;
                    }
                } else if arg.starts_with("--bind=") {
                    let val = arg.strip_prefix("--bind=").unwrap();
                    rewritten.push(format!("--serve-bind={}", val));
                    i += 1;
                } else if arg == "--open" {
                    rewritten.push("--serve-open".to_string());
                    i += 1;
                } else if !arg.starts_with('-') && !has_dir {
                    // 第一个不以 '-' 开头的参数认为是静态根目录
                    rewritten.push("--serve-static".to_string());
                    rewritten.push(arg.clone());
                    has_dir = true;
                    i += 1;
                } else {
                    rewritten.push(arg.clone());
                    i += 1;
                }
            }
            Ok(Some(rewritten))
        }
        "doctor" => Err(CliError::InvalidArgs(
            "`doctor` command has been removed. Use one of: `workspace`, `encoding`, `rule`, `env`"
                .to_string(),
        )),
        _ => Ok(None),
    }
}


pub(crate) fn rewrite_doctor_flags(args: &[String]) -> Vec<String> {
    args.iter()
        .map(|arg| match arg.as_str() {
            "--format" | "-f" => "--doctor-format".to_string(),
            _ => arg.clone(),
        })
        .collect()
}


pub(crate) fn rewrite_gain_flags(args: &[String]) -> Vec<String> {
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


pub(crate) fn detect_external_like_first_arg(argv: &[String]) -> (String, String, bool) {
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


pub(crate) fn map_clap_error(err: clap::Error, argv: &[String]) -> CliError {
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


pub(crate) fn reject_legacy_flags(args: &[String]) -> Result<(), CliError> {
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


pub(crate) fn build_run_mode_args(
    run_command: Vec<String>,
    explain_route: bool,
    stream: bool,
    merge: bool,
    flush_interval: u64,
    run_plugin: Option<String>,
    passthrough: bool,
    tee: Option<std::path::PathBuf>,
) -> CliArgs {
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
        json: false,
        stream,
        flush_interval,
        merge,
        config_args: Vec::new(),
        serve_static: None,
        serve_port: None,
        serve_bind: None,
        serve_open: false,
        run_plugin,
        passthrough,
        tee,
    }
}


pub(crate) fn parse_output_format_arg(format: &str) -> Result<OutputFormat, CliError> {
    format.parse().map_err(|e: String| CliError::InvalidArgs(e))
}


pub(crate) fn parse_doctor_output_format_arg(format: &str) -> Result<DoctorOutputFormat, CliError> {
    format
        .parse::<DoctorOutputFormat>()
        .map_err(CliError::InvalidArgs)
}


pub(crate) fn parse_preset_arg(preset: Option<&str>) -> Result<Option<Preset>, CliError> {
    preset
        .map(|p| p.parse::<Preset>().map_err(CliError::InvalidArgs))
        .transpose()
}


pub(crate) fn build_cli_args_from_raw(
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
        json: cli.json,
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
        stream: cli.stream,
        flush_interval: cli.flush_interval,
        merge: cli.merge,
        config_args: cli.config_args,
        serve_static: cli.serve_static,
        serve_port: cli.serve_port,
        serve_bind: cli.serve_bind,
        serve_open: cli.serve_open,
        run_plugin: cli.run_plugin,
        passthrough: cli.passthrough,
        tee: cli.tee,
    }
}


impl CliArgs {
    pub fn parse_args() -> Result<Self, CliError> {
        let argv: Vec<String> = std::env::args().collect();
        parse_args_from_argv(&argv)
    }

    pub(crate) fn from_raw(cli: CliRawArgs) -> Result<Self, CliError> {
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


pub(crate) fn parse_args_from_argv(argv: &[String]) -> Result<CliArgs, CliError> {
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


pub(crate) fn extract_run_wrapper_args_from_run_cmd(
    run_cmd: Vec<String>,
) -> (bool, bool, u64, Option<String>, bool, Option<std::path::PathBuf>, Vec<String>) {
    let mut stream = false;
    let mut merge = false;
    let mut flush_interval = 500;
    let mut run_plugin = None;
    let mut passthrough = false;
    let mut tee = None;
    let mut filtered = Vec::new();
    let mut i = 0;
    while i < run_cmd.len() {
        let arg = &run_cmd[i];
        if arg == "--stream" {
            stream = true;
            i += 1;
        } else if arg == "--merge" {
            merge = true;
            i += 1;
        } else if arg == "--flush-interval" {
            if i + 1 < run_cmd.len() {
                if let Ok(val) = run_cmd[i + 1].parse::<u64>() {
                    flush_interval = val;
                }
                i += 2;
            } else {
                i += 1;
            }
        } else if arg.starts_with("--flush-interval=") {
            if let Some(val_str) = arg.strip_prefix("--flush-interval=") {
                if let Ok(val) = val_str.parse::<u64>() {
                    flush_interval = val;
                }
            }
            i += 1;
        } else if arg == "--run-plugin" || arg == "--plugin" {
            if i + 1 < run_cmd.len() {
                run_plugin = Some(run_cmd[i + 1].clone());
                i += 2;
            } else {
                i += 1;
            }
        } else if arg.starts_with("--run-plugin=") {
            run_plugin = arg.strip_prefix("--run-plugin=").map(String::from);
            i += 1;
        } else if arg.starts_with("--plugin=") {
            run_plugin = arg.strip_prefix("--plugin=").map(String::from);
            i += 1;
        } else if arg == "--passthrough" {
            passthrough = true;
            i += 1;
        } else if arg == "--tee" {
            if i + 1 < run_cmd.len() {
                tee = Some(std::path::PathBuf::from(run_cmd[i + 1].clone()));
                i += 2;
            } else {
                i += 1;
            }
        } else if arg.starts_with("--tee=") {
            tee = arg.strip_prefix("--tee=").map(std::path::PathBuf::from);
            i += 1;
        } else if arg == "--" {
            filtered.extend(run_cmd[i + 1..].to_vec());
            break;
        } else {
            filtered.push(arg.clone());
            i += 1;
        }
    }
    (stream, merge, flush_interval, run_plugin, passthrough, tee, filtered)
}

pub(crate) fn parse_run_mode_args_from_argv(argv: &[String]) -> Option<CliArgs> {
    if let Some(run_cmd) = maybe_parse_run_subcommand_from_argv(argv) {
        let (explain_route, run_cmd) = split_run_explain_route_flag(run_cmd);
        let (stream, merge, flush_interval, run_plugin, passthrough, tee, run_cmd) =
            extract_run_wrapper_args_from_run_cmd(run_cmd);
        return Some(build_run_mode_args(
            run_cmd,
            explain_route,
            stream,
            merge,
            flush_interval,
            run_plugin,
            passthrough,
            tee,
        ));
    }
    if let Some(run_cmd) = maybe_parse_implicit_run_command_from_argv(argv) {
        let (stream, merge, flush_interval, run_plugin, passthrough, tee, run_cmd) =
            extract_run_wrapper_args_from_run_cmd(run_cmd);
        return Some(build_run_mode_args(
            run_cmd,
            false,
            stream,
            merge,
            flush_interval,
            run_plugin,
            passthrough,
            tee,
        ));
    }
    None
}


pub(crate) fn parse_cli_args_with_clap(argv: &[String]) -> Result<CliArgs, CliError> {
    let cli = <CliRawArgs as clap::Parser>::try_parse_from(argv.to_vec())
        .map_err(|e| map_clap_error(e, argv))?;
    CliArgs::from_raw(cli)
}


pub(crate) fn resolve_cli_mode(cli: &CliRawArgs) -> CliMode {
    match cli.mode.as_deref() {
        Some("compress") => CliMode::Compress,
        Some("decompress") => CliMode::Decompress,
        Some("init") => CliMode::Init,
        Some("config") => CliMode::Config,
        Some("serve-static") | Some("serve_static") => CliMode::ServeStatic,

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


pub(crate) fn validate_repair_file_scoped_args(cli: &CliRawArgs, mode: &CliMode) -> Result<(), CliError> {
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


pub(crate) fn validate_exclusive_feature_flags(cli: &CliRawArgs) -> Result<(), CliError> {
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


pub(crate) fn validate_verify_triplet(cli: &CliRawArgs) -> Result<(), CliError> {
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


pub(crate) fn parse_optional_doctor(doctor: Option<&str>) -> Result<Option<DoctorKind>, CliError> {
    if let Some(d) = doctor {
        return d
            .parse::<DoctorKind>()
            .map(Some)
            .map_err(CliError::InvalidArgs);
    }
    Ok(None)
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
pub(crate) enum PrePipelineAction {
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
    Config,
    ServeStatic,
}


pub(crate) fn select_pre_pipeline_action(args: &CliArgs) -> PrePipelineAction {
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
    if matches!(args.mode, CliMode::Config) {
        return PrePipelineAction::Config;
    }
    if matches!(args.mode, CliMode::ServeStatic) {
        return PrePipelineAction::ServeStatic;
    }
    PrePipelineAction::Continue
}


pub(crate) fn should_show_compress_quick_usage(
    launched_without_args: bool,
    is_stdin_input: bool,
    input_text: &str,
) -> bool {
    launched_without_args && is_stdin_input && input_text.trim().is_empty()
}


pub(crate) fn handle_pre_pipeline_action(args: &CliArgs) -> Result<bool, CliError> {
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
        PrePipelineAction::Config => {
            crate::cli::commands::config::handle_config_command(args)?;
            Ok(true)
        }
        PrePipelineAction::ServeStatic => {
            crate::cli::commands::serve_static::handle_serve_static_command(args)?;
            Ok(true)
        }
    }
}


pub(crate) fn resolve_plugins_for_args(args: &CliArgs) -> Vec<Box<dyn Plugin>> {
    if matches!(args.mode, CliMode::Run) {
        if let Some(prog) = args.run_command.first() {
            return plugins_for_run_command(prog, &args.run_command[1..], args.run_plugin.as_deref());
        }
    }
    get_plugins()
}


pub(crate) fn build_pipeline_config_for_args(args: &CliArgs) -> PipelineConfig {
    use crate::core::config_manager::ConfigManager;

    let mut pipeline_config = PipelineConfig::default();
    pipeline_config.dispatcher_config.fallback_plugin = "git_diff".to_string();

    let active_preset = args.preset.or_else(|| {
        ConfigManager::get_value("compression.preset")
            .or_else(|| ConfigManager::get_value("general.preset"))
            .and_then(|p| p.parse::<Preset>().ok())
    });

    // Apply preset configuration
    if let Some(preset) = active_preset {
        match preset {
            crate::cli::types::Preset::Fast => {
                // Speed priority: disable heavy features, reduce thresholds
                pipeline_config.reorder_config.enabled = false;
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
                pipeline_config.dictionary_threshold = 0; // Always use dictionary for max context
                pipeline_config.dedup_config.pattern_threshold = 2; // More aggressive dedup
            }
        }
    } else if args.reorder {
        pipeline_config.reorder_config.enabled = true;
    }

    if args.preset.is_none() {
        if let Some(reorder) = ConfigManager::get_bool("compression.reorder") {
            pipeline_config.reorder_config.enabled = reorder;
        }
    }

    // Run 模式以可读性为先：保留空行分隔，避免帮助文本段落粘连。
    if matches!(args.mode, CliMode::Run) {
        pipeline_config.slicer_config.skip_empty_lines = false;
    }

    pipeline_config
}


pub fn run_cli() -> Result<(), CliError> {
    let argv: Vec<String> = std::env::args().collect();
    let launched_without_args = argv.len() <= 1;
    let program = argv
        .first()
        .map(|s| program_name_from_argv0(s))
        .unwrap_or_else(|| "tokenslim".to_string());

    if let Some(version_text) = intercept_version_request(&argv) {
        println!("{}", version_text);
        return Ok(());
    }

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
    let result: Result<(), CliError> = (|| {
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
            CliMode::Config => {
                unreachable!("config mode should return before pipeline execution")
            }
            CliMode::ServeStatic => {
                unreachable!("serve-static mode should return before pipeline execution")
            }
        }
        Ok(())
    })();

    if let Err(ref err) = result {
        if args.json {
            args.emit_error(err);
            let code = match err {
                CliError::InvalidArgs(_) => 2,
                _ => 1,
            };
            std::process::exit(code);
        }
    }
    result
}

impl CliArgs {
    /// 向输出目标写入纯文本；若启用 --json，则包装为结构化 JSON。
    pub fn emit_text(&self, text: &str, stats: Option<Value>) -> Result<(), CliError> {
        if self.json {
            let mut obj = serde_json::Map::new();
            obj.insert("status".to_string(), "success".into());
            obj.insert("data".to_string(), json!({ "text": text }));
            if let Some(stats) = stats {
                obj.insert("stats".to_string(), stats);
            }
            self.write_output(&serde_json::to_string(&obj)?)
        } else {
            self.write_output(text)
        }
    }

    /// 向输出目标写入可序列化数据；若启用 --json，则包装为结构化 JSON。
    pub fn emit_serializable<T: Serialize>(
        &self,
        data: &T,
        stats: Option<Value>,
    ) -> Result<(), CliError> {
        if self.json {
            let mut obj = serde_json::Map::new();
            obj.insert("status".to_string(), "success".into());
            obj.insert("data".to_string(), serde_json::to_value(data)?);
            if let Some(stats) = stats {
                obj.insert("stats".to_string(), stats);
            }
            self.write_output(&serde_json::to_string(&obj)?)
        } else {
            self.write_output(&serde_json::to_string_pretty(data)?)
        }
    }

    fn write_output(&self, payload: &str) -> Result<(), CliError> {
        match &self.output {
            OutputTarget::File(path) => std::fs::write(path, payload).map_err(CliError::Io),
            OutputTarget::Stdout => {
                println!("{}", payload);
                Ok(())
            }
        }
    }

    fn emit_error(&self, err: &CliError) {
        let code = match err {
            CliError::InvalidArgs(_) => "invalid_args",
            CliError::Io(_) => "io",
            CliError::Compression(_) => "compression",
            CliError::Decompression(_) => "decompression",
            CliError::Config(_) => "config",
            CliError::Pipeline(_) => "pipeline",
            CliError::Serialization(_) => "serialization",
        };
        let obj = json!({
            "status": "error",
            "error": err.to_string(),
            "code": code,
        });
        if let Ok(text) = serde_json::to_string(&obj) {
            println!("{}", text);
        }
    }
}
