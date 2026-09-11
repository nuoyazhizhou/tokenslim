//! cli 应用入口与参数解析

use crate::cli::commands::{
    benchmark::handle_verify_rule_action,
    compress::run_compress_mode,
    config::{
        check_hooks_status, detect_shell, handle_gain_action, handle_inject_action, install_hooks,
        parse_optional_hook_shell, uninstall_hooks,
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
use std::path::PathBuf;

/// 在 argv 中定位首个「位置参数（非选项）」的索引，作为子命令起始位置。
///
/// 从索引 1 开始向后扫描：遇到以 `-` 开头的选项时，仅放行 `--dry-run` / `-v` / `--verbose`
/// 并跳过；其余选项（如未知 flag）视为无有效子命令，返回 None；遇到首个非选项参数则返回其索引。
/// 若扫描到末尾仍未找到位置参数，返回 None。
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

/// 若 argv 以显式 `run` 子命令开头，提取其后的参数作为运行命令。
///
/// 复用 `find_cmd_index` 定位子命令位置；命中 `run` 时返回其后切片，否则返回 None。
pub(crate) fn maybe_parse_run_subcommand_from_argv(args: &[String]) -> Option<Vec<String>> {
    let cmd_index = find_cmd_index(args)?;

    if args[cmd_index].eq_ignore_ascii_case("run") {
        return Some(args[cmd_index + 1..].to_vec());
    }

    None
}

/// 判断给定字符串是否为 tokenslim 的内置子命令名。
///
/// 不区分大小写，覆盖 run/compress/decompress/init/workspace/encoding/rule/
/// env/gain/explain-plugin/plugins/repair-file/doctor/hooks/config/serve-static/feature 等。
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
            | "feature"
    )
}

/// 当 argv 未显式写 `run` 但首个位置参数不是内置命令时，将其整体当作隐式运行命令。
///
/// 复用 `find_cmd_index` 与 `is_tokenslim_builtin_command`；命中隐式运行命令时返回其后切片，否则 None。
pub(crate) fn maybe_parse_implicit_run_command_from_argv(args: &[String]) -> Option<Vec<String>> {
    let cmd_index = find_cmd_index(args)?;
    let cmd = &args[cmd_index];

    if is_tokenslim_builtin_command(cmd) {
        return None;
    }

    Some(args[cmd_index..].to_vec())
}

/// 从 argv[0] 中提取程序名称
///
/// 从可执行文件路径中提取文件名作为程序显示名称。
/// 如果提取失败则返回默认值 "tokenslim"。
///
/// # 参数
/// - `argv0` - 可执行文件的路径（argv[0]）
///
/// # 返回值
/// 程序名称字符串
pub(crate) fn program_name_from_argv0(argv0: &str) -> String {
    std::path::Path::new(argv0)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("tokenslim")
        .to_string()
}

/// 判断是否应该显示快速用法提示
///
/// 在以下情况直接显示用法而不进入流水线：
/// 1. 参数包含 --help 或 -h
/// 2. 参数仅包含 -v/--verbose 且无其他位置参数
/// 3. 无参数且 stdin 是终端（避免等待 stdin 造成卡住的体感）
///
/// # 参数
/// - `argv` - 命令行参数数组
/// - `stdin_is_terminal` - stdin 是否为终端
///
/// # 返回值
/// true 表示应该显示快速用法，false 表示继续正常处理
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

/// 生成 `config` 子命令的用法帮助文本。
///
/// 列出 set/get/list/unset/reset/wizard/plugin 等子命令及其示例，
/// 文案中的 `{program}` 占位由调用方传入的程序名填充。
pub(crate) fn render_config_usage(program: &str) -> String {
    format!(
        "{} config

{}

{}:
  {} config <subcommand> [args...]

{}:
  set <key> <value> [--global|-g]   {}
  get <key>                         {}
  list                              {}
  unset <key> [--global|-g]         {}
  reset [--global|-g]               {}
  wizard [--global|-g]              {}
  plugin <subcommand> [args...]     {}

{}:
  {} config set general.preset fast --global
  {} config get general.preset
  {} config list
  {} config wizard",
        program,
        t("cli_desc_config"),
        t("cli_help_usage"),
        program,
        t("cli_config_subcommands"),
        t("cli_config_sub_set"),
        t("cli_config_sub_get"),
        t("cli_config_sub_list"),
        t("cli_config_sub_unset"),
        t("cli_config_sub_reset"),
        t("cli_config_sub_wizard"),
        t("cli_config_sub_plugin"),
        t("cli_help_examples"),
        program,
        program,
        program,
        program
    )
}

/// 生成全局总览用法帮助文本（无子命令或未知命令时回退展示）。
///
/// 汇总核心命令、工作区诊断、工具类命令及常见示例，版本号取自 `CARGO_PKG_VERSION`。
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
        t("cli_desc_config"),
        "serve-static",
        t("cli_desc_serve_static"),
        t("cli_help_common_examples"),
        capability_summary
    )
}

/// 生成 `serve-static` 子命令的用法帮助文本。
///
/// 说明静态文件服务的目录参数与 `--port/--bind/--open` 选项及示例。
/// 全部用户可见文案均经 i18n 词表渲染，不随 locale 之外的输入变化。
pub(crate) fn render_serve_static_usage(program: &str) -> String {
    format!(
        "{} serve-static

{}

{}:
  {} serve-static [<DIR>] [{}]

{}:
  {:<34} {}

{}:
  {:<34} {}
  {:<34} {}
  {:<34} {}
  {:<34} {}

{}:
  {} serve-static                      # {}
  {} serve-static ./dist --port 9000   # {}
  {} serve-static --open               # {}",
        program,                                   // {0}
        t("cli_serve_static_desc"),                // {1}
        t("cli_help_usage"),                       // {2}
        program,                                   // {3}
        t("cli_help_options_placeholder"),         // {4}
        t("cli_serve_static_args"),                // {5}
        "<DIR>",                                   // {6}
        t("cli_serve_static_dir_arg"),             // {7}
        t("cli_help_options"),                     // {8}
        "--port <PORT>",                           // {9}
        t("cli_opt_port"),                         // {10}
        "--bind <IP>",                             // {11}
        t("cli_opt_bind"),                         // {12}
        "--open",                                  // {13}
        t("cli_opt_open"),                         // {14}
        "-h, --help",                              // {15}
        t("cli_opt_help"),                         // {16}
        t("cli_help_examples"),                    // {17}
        program,                                   // {18}
        t("cli_serve_static_example_1"),           // {19}
        program,                                   // {20}
        t("cli_serve_static_example_2"),           // {21}
        program,                                   // {22}
        t("cli_serve_static_example_3"),           // {23}
    )
}

/// 拦截版本请求
///
/// 检查命令行参数是否为版本查询请求（-V/--version/version）。
/// 会跳过 --dry-run、-v、--verbose 等全局开关后再判断。
///
/// # 参数
/// - `argv` - 命令行参数数组
///
/// # 返回值
/// - `Some(String)` - 版本信息字符串
/// - `None` - 不是版本请求
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

/// 拦截帮助请求
///
/// 检查命令行参数是否包含帮助标志（-h/--help），
/// 并根据子命令返回对应的帮助文本。
///
/// # 参数
/// - `argv` - 命令行参数数组
/// - `program` - 程序名称
///
/// # 返回值
/// - `Some(String)` - 帮助文本
/// - `None` - 不是帮助请求
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

/// 生成 `compress` 子命令的用法帮助文本。
///
/// 列出 `-i/-o/--format/--preset/--stream/--flush-interval/--merge` 等选项及示例。
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

/// 生成 `decompress` 子命令的用法帮助文本。
///
/// 列出 `-i/-o/--ai-export/--ai-signal/--source-encoding-write` 等选项及示例。
pub(crate) fn render_decompress_usage(program: &str) -> String {
    format!(
        "{} decompress

{}

{}:
  {} decompress [{}]

{}:
  {:<30} {}
  {:<30} {}
  {:<30} {}
  {:<30} {}
  {:<30} {}
  {:<30} {}

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
        "--source-encoding-write",
        t("cli_opt_source_encoding_write"),
        "-h, --help",
        t("cli_opt_help"),
        t("cli_help_examples"),
        program
    )
}

/// 生成 `repair-file` 子命令的用法帮助文本。
///
/// 说明 `<PATH>` 位置参数与 `--inplace/--backup/--include/--exclude` 选项及示例。
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

/// 生成 `run` 子命令的用法帮助文本。
///
/// 列出 `--stream/--flush-interval/--merge` 选项及 `run cargo test` 等示例。
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
        "--input <FILE>",
        t("cli_opt_input_file"),
        "--audit-jsonl <PATH>",
        t("cli_opt_audit_jsonl"),
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

/// 生成 `hooks` 子命令的用法帮助文本。
///
/// 说明 `install/status` 动作与 `--shell` 选项及示例。
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

/// 生成 `workspace` 子命令的用法帮助文本。
///
/// 列出 `--inject/--format` 选项及示例。
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

/// 生成 `encoding` 子命令的用法帮助文本。
///
/// 列出 `--fix/--format` 选项及示例。
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

/// 生成 `rule` 子命令的用法帮助文本。
///
/// 列出 `--format` 选项及示例。
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

/// 生成 `env` 子命令的用法帮助文本。
///
/// 列出 `--format` 选项及示例。
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

/// 生成 `gain` 子命令的用法帮助文本。
///
/// 列出 `--daily/--by-filter/--json/--days` 选项及示例。
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

/// 生成 `plugins` 子命令的简短用法帮助文本（用法 + 示例占位）。
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

/// 生成 `explain-plugin` 子命令的用法帮助文本。
///
/// 列出 `-i/--explain-command/--explain-replay-out` 选项及示例。
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

/// 生成 `init` 子命令的用法帮助文本。
///
/// 列出 `--force` 选项及示例。
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

/// 从运行命令中剥离并记录 `--explain-route` 标志。
///
/// 遍历 run_cmd：遇到 `--explain-route` 时置位标志并跳过紧跟的 `--`；
/// 遇到命令程序之后的 `--` 停止剥离并保留余下参数；返回 (是否解释路由, 余下参数)。
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

/// 生成运行模式缺少外部命令时的提示文本。
///
/// 文案来自 i18n 词表（`cli_run_requires_external_command`），其中 `{program}`
/// 占位符由调用方传入的程序名替换，便于在重命名/别名场景下保持示例可用。
pub(crate) fn render_run_command_hint(program: &str) -> String {
    t("cli_run_requires_external_command").replace("{program}", program)
}

/// 为 Run 模式补全来自 argv 的默认参数。
///
/// 仅当模式为 Run 时生效：未显式指定 `--format` 时默认 `Text` 输出，
/// 未显式指定 `--preset` 时默认 `Ai` 预设；其余模式原样返回。
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

/// 将旧式子命令写法改写为统一的 `--mode/--doctor/--gain/...` 标志式参数。
///
/// 内置子命令（如 compress/workspace/gain/hooks/serve-static 等）被映射为对应的
/// 长标志与子动作；非内置命令返回 None 表示无需改写；`repair-file` 缺少输入路径时报错。
pub(crate) fn rewrite_command_alias_to_flags(
    args: &[String],
) -> Result<Option<Vec<String>>, CliError> {
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
        "feature" => {
            // `feature --feature-lib <p> --learn <cat> --sample <f>` 原样透传给 --mode feature。
            rewritten.push("--mode".to_string());
            rewritten.push("feature".to_string());
            rewritten.extend(rest);
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

/// 将 doctor 相关短标志改写为 `--doctor-format`。
///
/// 仅把 `--format`/`-f` 替换为 `--doctor-format`，其余参数原样透传。
pub(crate) fn rewrite_doctor_flags(args: &[String]) -> Vec<String> {
    args.iter()
        .map(|arg| match arg.as_str() {
            "--format" | "-f" => "--doctor-format".to_string(),
            _ => arg.clone(),
        })
        .collect()
}

/// 将 gain 相关短标志改写为 `--gain-*` 长标志。
///
/// `--daily/--by-filter/--json` 映射为 `--gain-daily/--gain-by-filter/--gain-json`，
/// `--days [n]` 改写为 `--gain-days [n]`（含 `--days=n` 等号形式），其余透传。
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

/// 提取程序名、首个用户参数，并判断其是否“像外部命令”。
///
/// 像外部命令的条件：首个用户参数非空、不以 `-` 开头、且不是 tokenslim 内置命令。
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

/// 将 clap 解析错误转换为面向用户的 CliError，并附带友好提示与全局用法。
///
/// 若首个用户参数疑似外部命令，给出 `run <cmd>` 的提示；否则提示查看 `--help`；
/// 末尾追加全局用法与原始 clap 错误信息。
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

/// 拒绝已移除的旧式全局标志（`--mode/--doctor/--doctor-format`）。
///
/// 一旦检测到这些标志即返回 CliError，引导用户改用子命令式写法。
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

/// 根据运行命令与各项运行选项构造一个完整的 `CliArgs`（Run 模式）。
///
/// 将 stream/merge/flush_interval/run_plugin/passthrough/tee 等运行参数
/// 写入 `CliArgs`，其余字段使用 Run 模式的合理默认值（如 ai_export/ai_signal 开启）。
pub(crate) fn build_run_mode_args(
    run_command: Vec<String>,
    explain_route: bool,
    stream: bool,
    merge: bool,
    flush_interval: u64,
    run_plugin: Option<String>,
    passthrough: bool,
    tee: Option<std::path::PathBuf>,
    run_input_file: Option<std::path::PathBuf>,
    run_audit_jsonl: Option<std::path::PathBuf>,
) -> CliArgs {
    CliArgs {
        mode: CliMode::Run,
        input: match run_input_file {
            Some(path) => InputSource::File(path),
            None => InputSource::Stdin,
        },
        output: OutputTarget::Stdout,
        verbose: false,
        calc_tokens: false,
        reorder: false,
        semantic: false,
        normalize: false,
        ai_export: true,
        ai_signal: true,
        strict_rehydrate: false,
        source_encoding_write: false,
        output_format: OutputFormat::Text,
        verify_rule: None,
        verify_fixture: None,
        verify_expected: None,
        feature_learn: None,
        feature_sample: None,
        feature_lib: None,
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
        audit_jsonl: run_audit_jsonl,
    }
}

/// 将输出格式字符串解析为 `OutputFormat` 枚举。
///
/// 复用 `FromStr` 实现；解析失败时将错误信息包装为 CliError。
pub(crate) fn parse_output_format_arg(format: &str) -> Result<OutputFormat, CliError> {
    format.parse().map_err(|e: String| CliError::InvalidArgs(e))
}

/// 将 doctor 输出格式字符串解析为 `DoctorOutputFormat` 枚举。
///
/// 复用 `FromStr` 实现；解析失败时将错误包装为 CliError。
pub(crate) fn parse_doctor_output_format_arg(format: &str) -> Result<DoctorOutputFormat, CliError> {
    format
        .parse::<DoctorOutputFormat>()
        .map_err(CliError::InvalidArgs)
}

/// 将可选的预设字符串解析为 `Option<Preset>`。
///
/// 传入 None 时返回 None；传入 Some 时解析，失败则包装为 CliError；
/// 用 `transpose` 把 `Option<Result>` 翻转为 `Result<Option>`。
pub(crate) fn parse_preset_arg(preset: Option<&str>) -> Result<Option<Preset>, CliError> {
    preset
        .map(|p| p.parse::<Preset>().map_err(CliError::InvalidArgs))
        .transpose()
}

/// 将 clap 解析出的 `CliRawArgs` 与派生参数（模式/钩子 shell/doctor/格式/预设）组装为 `CliArgs`。
///
/// 负责把原始可选字段（input/output/hook_shell 等）映射为 `CliArgs` 的强类型字段，
/// 并补全 doctor_format/preset 等由上层解析得到的取值。
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
        strict_rehydrate: cli.strict_rehydrate,
        source_encoding_write: cli.source_encoding_write,
        output_format,
        json: cli.json,
        verify_rule: cli.verify_rule,
        verify_fixture: cli.verify_fixture,
        verify_expected: cli.verify_expected,
        feature_learn: cli.feature_learn,
        feature_sample: cli.feature_sample,
        feature_lib: cli.feature_lib,
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
        audit_jsonl: cli.audit_jsonl,
    }
}

impl CliArgs {
    /// 从进程真实命令行参数（`std::env::args`）解析 `CliArgs`。
    ///
    /// 仅作薄封装：收集 `env::args` 后转交 `parse_args_from_argv`。
    pub fn parse_args() -> Result<Self, CliError> {
        let argv: Vec<String> = std::env::args().collect();
        parse_args_from_argv(&argv)
    }

    /// 由 `CliRawArgs` 构建 `CliArgs`，串联模式解析、参数校验与派生字段解析。
    ///
    /// 依次：解析模式、校验 repair-file 作用域/互斥特性/verify 三元组、
    /// 解析 hook shell 与 doctor、解析输出格式与预设，最后委托 `build_cli_args_from_raw`。
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

/// 从 argv 解析完整 `CliArgs`，是参数解析的入口编排函数。
///
/// 优先尝试 Run 模式快捷解析；否则拒绝旧标志后，将子命令别名改写为标志式参数，
/// 再交给 clap 解析，最后应用 Run 模式默认值。
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

/// 从运行命令中解析 run 模式的包裹参数（stream/merge/flush/plugin/passthrough/tee）。
///
/// 逐个扫描 run_cmd：识别 `--stream/--merge/--flush-interval/--run-plugin/--passthrough/--tee`，
/// 区分命令程序前后的 `--` 语义（前者剥离、后者保留），其余归入 filtered 透传。
pub(crate) fn extract_run_wrapper_args_from_run_cmd(
    run_cmd: Vec<String>,
) -> (
    bool,
    bool,
    u64,
    Option<String>,
    bool,
    Option<std::path::PathBuf>,
    Option<std::path::PathBuf>,
    Option<std::path::PathBuf>,
    Vec<String>,
) {
    let mut stream = false;
    let mut merge = false;
    let mut flush_interval = 500;
    let mut run_plugin = None;
    let mut passthrough = false;
    let mut tee = None;
    let mut run_input_file = None;
    let mut run_audit_jsonl = None;
    let mut filtered = Vec::new();
    let mut i = 0;
    // 记录是否已遇到命令程序 (第一个非 wrapper flag 的 token)。
    // 用于区分 `--` 的两种语义: 程序之前的 `--` 是 tokenslim 与命令的分隔符 (应剥离),
    // 程序之后的 `--` 属于命令自身参数 (如 `cargo test -- --nocapture`, 必须保留)。
    let mut seen_program = false;
    while i < run_cmd.len() {
        let arg = &run_cmd[i];
        // Wrapper flags 只在命令程序出现之前生效：一旦已识别命令程序，后续同名 flag
        // 一律作为子命令自身参数透传，避免 `pytest --plugin x` / `cargo test --stream`
        // 等被 tokenslim 错误吞掉。`--` 分隔符的两种语义在下方单独处理。
        let is_wrapper_flag = arg == "--stream"
            || arg == "--merge"
            || arg == "--flush-interval"
            || arg.starts_with("--flush-interval=")
            || arg == "--run-plugin"
            || arg == "--plugin"
            || arg.starts_with("--run-plugin=")
            || arg.starts_with("--plugin=")
            || arg == "--passthrough"
            || arg == "--tee"
            || arg.starts_with("--tee=")
            || arg == "--input"
            || arg.starts_with("--input=")
            || arg == "--audit-jsonl"
            || arg.starts_with("--audit-jsonl=");
        if seen_program && is_wrapper_flag {
            filtered.push(arg.clone());
            i += 1;
            continue;
        }
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
        } else if arg == "--input" {
            if i + 1 < run_cmd.len() {
                run_input_file = Some(std::path::PathBuf::from(run_cmd[i + 1].clone()));
                i += 2;
            } else {
                i += 1;
            }
        } else if arg.starts_with("--input=") {
            run_input_file = arg.strip_prefix("--input=").map(std::path::PathBuf::from);
            i += 1;
        } else if arg == "--audit-jsonl" {
            if i + 1 < run_cmd.len() {
                run_audit_jsonl = Some(std::path::PathBuf::from(run_cmd[i + 1].clone()));
                i += 2;
            } else {
                i += 1;
            }
        } else if arg.starts_with("--audit-jsonl=") {
            run_audit_jsonl = arg
                .strip_prefix("--audit-jsonl=")
                .map(std::path::PathBuf::from);
            i += 1;
        } else if arg == "--" {
            if !seen_program {
                // `--` 出现在命令程序之前: 作为 tokenslim 与命令的分隔符, 剥离自身后透传
                filtered.extend(run_cmd[i + 1..].to_vec());
            } else {
                // `--` 出现在命令程序之后: 属于命令自身的参数 (如 cargo test -- --nocapture), 保留
                filtered.push(arg.clone());
                filtered.extend(run_cmd[i + 1..].to_vec());
            }
            break;
        } else {
            filtered.push(arg.clone());
            // 第一个非 wrapper flag 的 token 即命令程序
            seen_program = true;
            i += 1;
        }
    }
    (
        stream,
        merge,
        flush_interval,
        run_plugin,
        passthrough,
        tee,
        run_input_file,
        run_audit_jsonl,
        filtered,
    )
}

/// 从 argv 解析 Run 模式的 `CliArgs`（显式 `run` 或隐式外部命令两种入口）。
///
/// 先尝试显式 `run` 子命令，再尝试隐式外部命令；两者均先剥离 `--explain-route`，
/// 再提取包裹参数并委托 `build_run_mode_args` 构造最终结果。
pub(crate) fn parse_run_mode_args_from_argv(argv: &[String]) -> Option<CliArgs> {
    if let Some(run_cmd) = maybe_parse_run_subcommand_from_argv(argv) {
        let (explain_route, run_cmd) = split_run_explain_route_flag(run_cmd);
        let (
            stream,
            merge,
            flush_interval,
            run_plugin,
            passthrough,
            tee,
            run_input_file,
            run_audit_jsonl,
            run_cmd,
        ) = extract_run_wrapper_args_from_run_cmd(run_cmd);
        return Some(build_run_mode_args(
            run_cmd,
            explain_route,
            stream,
            merge,
            flush_interval,
            run_plugin,
            passthrough,
            tee,
            run_input_file,
            run_audit_jsonl,
        ));
    }
    if let Some(run_cmd) = maybe_parse_implicit_run_command_from_argv(argv) {
        let (
            stream,
            merge,
            flush_interval,
            run_plugin,
            passthrough,
            tee,
            run_input_file,
            run_audit_jsonl,
            run_cmd,
        ) = extract_run_wrapper_args_from_run_cmd(run_cmd);
        return Some(build_run_mode_args(
            run_cmd,
            false,
            stream,
            merge,
            flush_interval,
            run_plugin,
            passthrough,
            tee,
            run_input_file,
            run_audit_jsonl,
        ));
    }
    None
}

/// 用 clap 解析 argv 为 `CliRawArgs`，再转换为 `CliArgs`。
///
/// clap 解析失败时将 `clap::Error` 交给 `map_clap_error` 生成友好错误。
pub(crate) fn parse_cli_args_with_clap(argv: &[String]) -> Result<CliArgs, CliError> {
    let cli = <CliRawArgs as clap::Parser>::try_parse_from(argv.to_vec())
        .map_err(|e| map_clap_error(e, argv))?;
    CliArgs::from_raw(cli)
}

/// 根据 `CliRawArgs.mode` 解析出 `CliMode` 枚举。
///
/// 已知子命令直接映射；未指定 mode 时，若携带 run_command 则归为 Run，否则默认 Compress。
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
        Some("feature") => CliMode::Feature,
        _ => {
            if !cli.run_command.is_empty() {
                CliMode::Run
            } else {
                CliMode::Compress
            }
        }
    }
}

/// 校验仅 `repair-file` 模式可用的作用域参数。
///
/// `--inplace/--backup/--include/--exclude` 若出现在非 repair-file 模式下即报错。
pub(crate) fn validate_repair_file_scoped_args(
    cli: &CliRawArgs,
    mode: &CliMode,
) -> Result<(), CliError> {
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

/// 校验互斥特性标志不能同时使用。
///
/// `--ai-export` 与 `--ai-signal`、`--init-hooks` 与 `--uninstall-hooks` 两两互斥。
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

/// 校验 verify 三元组参数要么全提供、要么全不提供。
///
/// `--verify-rule/--verify-fixture/--verify-expected` 提供数量必须为 0 或 3，否则报错。
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

/// 将可选的 doctor 字符串解析为 `Option<DoctorKind>`。
///
/// 传入 None 返回 None；传入 Some 时按 `FromStr` 解析，失败包装为 CliError。
pub(crate) fn parse_optional_doctor(doctor: Option<&str>) -> Result<Option<DoctorKind>, CliError> {
    if let Some(d) = doctor {
        return d
            .parse::<DoctorKind>()
            .map(Some)
            .map_err(CliError::InvalidArgs);
    }
    Ok(None)
}

/// 构造并返回全部默认启用的压缩插件实例列表。
///
/// 集中 new 所有内置插件（VCS/语言/框架/通用等数十种），供非 Run 模式全量加载；
/// Run 模式则由 `plugins_for_run_command` 按需挑选。
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
    use crate::plugins::ls_listing_plugin::LsListingPlugin;
    use crate::plugins::markdown_plugin::MarkdownPlugin;
    use crate::plugins::maven_plugin::MavenPlugin;
    use crate::plugins::ndjson_plugin::NdjsonPlugin;
    use crate::plugins::node_error_plugin::NodeErrorPlugin;
    use crate::plugins::nodejs_plugin::NodeJsPlugin;
    use crate::plugins::noise_filter_plugin::NoiseFilterPlugin;
    use crate::plugins::php_ruby_plugin::PhpRubyPlugin;
    use crate::plugins::privacy_plugin::PrivacyPlugin;
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
    use crate::plugins::terraform_plugin::TerraformPlugin;
    use crate::plugins::toml_ini_plugin::TomlIniPlugin;
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
    plugins.push(Box::new(LsListingPlugin::new()));
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
    plugins.push(Box::new(PrivacyPlugin::new()));
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
    plugins.push(Box::new(TomlIniPlugin::new()));
    plugins.push(Box::new(UnityUnrealPlugin::new()));
    plugins.push(Box::new(WebLogPlugin::new()));
    plugins.push(Box::new(XcodeLogPlugin::new()));
    plugins.push(Box::new(WebpackVitePlugin::new()));
    plugins.push(Box::new(XmlHtmlPlugin::new()));
    plugins.push(Box::new(VcsPlugin::new()));
    plugins.push(Box::new(GitDiffPlugin::new()));
    plugins.push(Box::new(YamlPlugin::new()));

    plugins
}

/// 流水线执行前的预操作枚举。
///
/// 由 `select_pre_pipeline_action` 根据 `CliArgs` 选出，描述在创建压缩流水线之前
/// 需要先处理的动作（注入、各类 doctor 诊断、rewrite、discover、gain、init、hooks 等）。
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
    Feature,
}

/// 根据 `CliArgs` 选出本次应执行的预操作（`PrePipelineAction`）。
///
/// 按优先级依次判断 inject / doctor 系列 / rewrite / discover / gain / init /
/// hooks / verify-rule / explain-plugin / plugins / repair-file / config / serve-static，
/// 均不匹配时回退为 `Continue`（进入常规压缩流水线）。
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
    if matches!(args.mode, CliMode::Feature) {
        return PrePipelineAction::Feature;
    }
    PrePipelineAction::Continue
}

/// 判断是否对 compress 模式展示“空输入”快速用法。
///
/// 仅当“无参数启动 + 输入来自终端 + 标准输入文本为空”三者同时满足时返回 true，
/// 用于避免空输入时阻塞等待 stdin 的体感。
pub(crate) fn should_show_compress_quick_usage(
    launched_without_args: bool,
    is_stdin_input: bool,
    input_text: &str,
) -> bool {
    launched_without_args && is_stdin_input && input_text.trim().is_empty()
}

/// 处理流水线前的预操作
///
/// 在创建压缩流水线之前，先判断是否需要执行一些预操作。
/// 这些操作不需要完整的流水线，包括：
/// - inject: 注入命令包装
/// - doctor 系列: 编码/工作区/规则/环境诊断
/// - rewrite: 命令重写
/// - discover: 插件发现
/// - gain: 增益统计
/// - init: 初始化项目
/// - hooks: Git hooks 管理
/// - verify-rule: 规则验证
/// - explain-plugin: 插件说明
/// - plugins: 插件列表
/// - repair-file: 文件修复
/// - config: 配置管理
/// - serve-static: 静态文件服务
///
/// # 参数
/// - `args` - CLI 参数
///
/// # 返回值
/// - `Ok(true)` - 已处理预操作，程序应直接退出
/// - `Ok(false)` - 未处理预操作，继续进入流水线
/// - `Err(CliError)` - 处理过程中发生错误
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
        PrePipelineAction::Feature => {
            crate::cli::commands::feature::handle_feature_command(args)?;
            Ok(true)
        }
    }
}

/// 根据 CLI 参数解析需要加载的插件列表
///
/// - Run 模式：根据运行的命令选择对应的插件（如 cargo 命令选择 rust_go 插件）
/// - 其他模式：加载所有默认插件
///
/// # 参数
/// - `args` - CLI 参数
///
/// # 返回值
/// 插件实例列表
pub(crate) fn resolve_plugins_for_args(args: &CliArgs) -> Vec<Box<dyn Plugin>> {
    if matches!(args.mode, CliMode::Run) {
        if let Some(prog) = args.run_command.first() {
            return plugins_for_run_command(
                prog,
                &args.run_command[1..],
                args.run_plugin.as_deref(),
            );
        }
    }
    get_plugins()
}

/// 获取当前项目工作区的调试审计路径。
///
/// 该功能只读取最近 `.tokenslim.toml` 的本地配置，避免用户的全局配置、环境变量
/// 或 AGENTS.md 注入流程在不知情的情况下启用审计。
fn workspace_debug_audit_path() -> Option<PathBuf> {
    if crate::core::config_manager::ConfigManager::get_local_bool("debug.audit.enabled")
        != Some(true)
    {
        return None;
    }
    let config_path = crate::core::config_manager::ConfigManager::local_project_config_path()?;
    let workspace_root = config_path.parent()?;
    let configured_path =
        crate::core::config_manager::ConfigManager::get_local_value("debug.audit.path")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| ".tokenslim/audit/compression.jsonl".to_string());
    let audit_path = PathBuf::from(configured_path);
    Some(if audit_path.is_absolute() {
        audit_path
    } else {
        workspace_root.join(audit_path)
    })
}
/// 根据 CLI 参数构建流水线配置。
///
/// 配置优先级从高到低：命令行参数 > 用户配置文件 > 默认值。
///
/// 主要配置项包括：
/// - 预设模式（fast/balanced/ai）
/// - 重排序功能
/// - 字典阈值
/// - 去重配置
/// - 切片器配置
///
/// # 参数
/// - `args` - CLI 参数
///
/// # 返回值
/// 流水线配置对象
pub(crate) fn build_pipeline_config_for_args(args: &CliArgs) -> PipelineConfig {
    use crate::core::config_manager::ConfigManager;

    let mut pipeline_config = PipelineConfig::default();
    pipeline_config.debug_audit_jsonl =
        args.audit_jsonl.clone().or_else(workspace_debug_audit_path);

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

/// CLI 主入口函数
///
/// 负责 TokenSlim 命令行工具的整体流程控制：
/// 1. 解析命令行参数，提取程序名称
/// 2. 拦截版本请求（-V/--version/version）
/// 3. 拦截帮助请求（-h/--help），根据子命令返回对应帮助
/// 4. 处理无参数交互调用场景，直接显示用法
/// 5. 使用 clap 解析完整参数
/// 6. 处理流水线前的预操作（init、doctor、config 等）
/// 7. 根据参数解析插件列表和流水线配置
/// 8. 创建压缩流水线并根据模式执行对应操作
/// 9. 处理 JSON 输出模式下的错误格式化
///
/// # 返回值
/// - Ok(()) - 执行成功
/// - Err(CliError) - 执行失败，包含错误类型和信息
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
            CliMode::Feature => {
                unreachable!("feature mode should return before pipeline execution")
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

    /// 将负载写入输出目标（文件或标准输出）。
    ///
    /// 文件目标用 `std::fs::write`，标准输出用 `println!`；IO 错误包装为 `CliError::Io`。
    fn write_output(&self, payload: &str) -> Result<(), CliError> {
        match &self.output {
            OutputTarget::File(path) => std::fs::write(path, payload).map_err(CliError::Io),
            OutputTarget::Stdout => {
                println!("{}", payload);
                Ok(())
            }
        }
    }

    /// P1-08：字节写出通道——按源编码回写时产出的是原始字节而非 UTF-8 文本，
    /// 必须经此通道写出；stdout 场景直接 `write_all`，不做文本化包装。
    pub fn write_output_bytes(&self, bytes: &[u8]) -> Result<(), CliError> {
        match &self.output {
            OutputTarget::File(path) => std::fs::write(path, bytes).map_err(CliError::Io),
            OutputTarget::Stdout => {
                use std::io::Write;
                io::stdout().write_all(bytes).map_err(CliError::Io)
            }
        }
    }

    /// 以 `--json` 错误格式打印 `CliError` 到标准输出。
    ///
    /// 根据错误变体归类为 `invalid_args/io/compression/...` 等 code，输出
    /// `{status:error, error, code}` 的 JSON；序列化失败则静默跳过。
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

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(run_cmd: Vec<&str>) -> Vec<String> {
        let run_cmd: Vec<String> = run_cmd.into_iter().map(String::from).collect();
        let (_, _, _, _, _, _, _, _, filtered) = extract_run_wrapper_args_from_run_cmd(run_cmd);
        filtered
    }

    // 回归: `cargo test ... -- --nocapture` 中的 `--` 是命令自身参数分隔符,
    // 转发时必须保留, 否则 Cargo 在执行测试前就以 exit 1 拒绝。
    #[test]
    fn preserves_double_dash_after_command_program() {
        let filtered = extract(vec![
            "cargo",
            "test",
            "--manifest-path",
            "rust_ext/Cargo.toml",
            "--lib",
            "http_server",
            "--",
            "--nocapture",
        ]);
        assert_eq!(
            filtered,
            vec![
                "cargo",
                "test",
                "--manifest-path",
                "rust_ext/Cargo.toml",
                "--lib",
                "http_server",
                "--",
                "--nocapture",
            ]
        );
    }

    // `--` 出现在命令程序之前时仍是 tokenslim 与命令的分隔符, 应剥离
    #[test]
    fn strips_double_dash_before_command_program() {
        let filtered = extract(vec!["--stream", "--", "cargo", "test"]);
        assert_eq!(filtered, vec!["cargo", "test"]);
    }

    // 显式 `--` 分隔符 + 命令自身 `--` 分隔符 同时存在时, 前者剥离、后者保留
    #[test]
    fn preserves_command_double_dash_when_separator_precedes() {
        let filtered = extract(vec!["--", "cargo", "test", "--", "--nocapture"]);
        assert_eq!(filtered, vec!["cargo", "test", "--", "--nocapture"]);
    }

    // 无 `--` 的普通命令不受影响
    #[test]
    fn ordinary_command_untouched() {
        let filtered = extract(vec!["git", "status"]);
        assert_eq!(filtered, vec!["git", "status"]);
    }

    // 命令程序之后出现的 wrapper flags（如 `--plugin`/`--stream`）必须原样透传给子命令，
    // 否则 `pytest --plugin x` / `cargo test --stream` 等命令自身的参数会被错误吞掉。
    #[test]
    fn passes_through_same_named_flags_after_command_program() {
        let filtered = extract(vec![
            "pytest",
            "--plugin",
            "pytest_rerunfailures",
            "--stream",
            "--merge",
            "tests/test_a.py",
        ]);
        assert_eq!(
            filtered,
            vec![
                "pytest",
                "--plugin",
                "pytest_rerunfailures",
                "--stream",
                "--merge",
                "tests/test_a.py",
            ]
        );
    }

    // 命令程序之前的 wrapper flags 仍按 tokenslim 语义生效（剥离），不被透传给子命令
    #[test]
    fn still_strips_wrapper_flags_before_command_program() {
        let filtered = extract(vec!["--stream", "--plugin", "smart_path", "pytest", "run"]);
        // 注意: `--plugin smart_path` 与 `--stream` 均被剥离
        assert_eq!(filtered, vec!["pytest", "run"]);
    }
}
