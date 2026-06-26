//! cli config 子命令

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
use bumpalo::Bump;
use serde::Serialize;
use std::borrow::Cow;
use std::io::{self, IsTerminal, Read};
use crate::core::config_manager::{ConfigManager, ConfigScope, global_config_path, local_config_path};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(crate) const HOOK_BEGIN: &str = "# >>> tokenslim hook >>>";
pub(crate) const HOOK_END: &str = "# <<< tokenslim hook <<<";


pub(crate) fn parse_optional_hook_shell(shell: Option<&str>) -> Result<Option<HookShell>, CliError> {
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


pub(crate) fn detect_shell() -> HookShell {
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


pub(crate) fn resolve_home_dir() -> Result<std::path::PathBuf, CliError> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .map_err(|_| CliError::Config("unable to resolve HOME/USERPROFILE".to_string()))
}


pub(crate) fn shell_rc_paths(shell: HookShell) -> Result<Vec<std::path::PathBuf>, CliError> {
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


pub(crate) fn hook_block(shell: HookShell) -> String {
    let content = crate::core::init_command::generate_hook_content(shell.as_str());
    format!("{HOOK_BEGIN}\n{content}\n{HOOK_END}\n")
}


pub(crate) fn remove_hook_block(content: &str) -> String {
    if let (Some(start), Some(end)) = (content.find(HOOK_BEGIN), content.find(HOOK_END)) {
        let end_with_marker = end + HOOK_END.len();
        let mut out = String::new();
        out.push_str(&content[..start]);
        out.push_str(content[end_with_marker..].trim_start_matches(['\r', '\n']));
        return out;
    }
    content.to_string()
}


pub(crate) fn install_hooks(shell: HookShell, dry_run: bool) -> Result<(), CliError> {
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


pub(crate) fn check_hooks_status(shell: HookShell) -> Result<(), CliError> {
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


pub(crate) fn uninstall_hooks(shell: HookShell, dry_run: bool) -> Result<(), CliError> {
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


pub(crate) fn handle_inject_action(args: &CliArgs) -> Result<bool, CliError> {
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


pub(crate) fn handle_gain_action(args: &CliArgs) -> Result<bool, CliError> {
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


/// 处理 `tokenslim config` 子命令分发
pub(crate) fn handle_config_command(args: &CliArgs) -> Result<(), CliError> {
    use crate::cli::app::render_config_usage;

    let sub_args = &args.config_args;
    if sub_args.is_empty() {
        println!("{}", render_config_usage("tokenslim"));
        return Ok(());
    }

    let sub_cmd = sub_args[0].as_str();
    match sub_cmd {
        "set" => {
            if sub_args.len() < 3 {
                return Err(CliError::InvalidArgs(
                    "set 命令需要指定键和值。例如: tokenslim config set general.preset fast".to_string()
                ));
            }
            let key = sub_args[1].as_str();
            let value = sub_args[2].as_str();
            let global = sub_args.contains(&"--global".to_string()) || sub_args.contains(&"-g".to_string());
            let scope = if global { ConfigScope::Global } else { ConfigScope::Local };

            ConfigManager::set_value(scope, key, value)
                .map_err(|e| CliError::Config(format!("设置配置失败: {}", e)))?;
            println!(
                "✅ 成功将配置项 '{}' 设置为 '{}' ({})",
                key, value, if global { "全局" } else { "项目本地" }
            );
        }
        "get" => {
            if sub_args.len() < 2 {
                return Err(CliError::InvalidArgs(
                    "get 命令需要指定键。例如: tokenslim config get general.preset".to_string()
                ));
            }
            let key = sub_args[1].as_str();
            match ConfigManager::get_value(key) {
                Some(val) => println!("{}", val),
                None => println!("(未设置)"),
            }
        }
        "list" => {
            let merged = ConfigManager::load_merged_config();
            let mut keys: Vec<&String> = merged.keys().collect();
            keys.sort();

            println!("=== TokenSlim 生效配置列表 ===");
            for k in keys {
                let v = &merged[k];
                println!("{} = {}", k, v);
            }
        }
        "unset" => {
            if sub_args.len() < 2 {
                return Err(CliError::InvalidArgs(
                    "unset 命令需要指定键。例如: tokenslim config unset general.preset".to_string()
                ));
            }
            let key = sub_args[1].as_str();
            let global = sub_args.contains(&"--global".to_string()) || sub_args.contains(&"-g".to_string());
            let scope = if global { ConfigScope::Global } else { ConfigScope::Local };

            match ConfigManager::unset_value(scope, key) {
                Ok(true) => println!(
                    "✅ 成功从 {} 配置中移除 '{}'",
                    if global { "全局" } else { "项目本地" },
                    key
                ),
                Ok(false) => println!(
                    "⚠️ {} 配置中未找到配置项 '{}'",
                    if global { "全局" } else { "项目本地" },
                    key
                ),
                Err(e) => return Err(CliError::Config(format!("删除配置失败: {}", e))),
            }
        }
        "reset" => {
            let global = sub_args.contains(&"--global".to_string()) || sub_args.contains(&"-g".to_string());
            let scope = if global { ConfigScope::Global } else { ConfigScope::Local };

            ConfigManager::reset(scope)
                .map_err(|e| CliError::Config(format!("重置配置失败: {}", e)))?;
            println!(
                "✅ 成功清空并重置 {} 配置文件",
                if global { "全局" } else { "项目本地" }
            );
        }
        "wizard" => {
            let global = sub_args.contains(&"--global".to_string()) || sub_args.contains(&"-g".to_string());
            let scope = if global { ConfigScope::Global } else { ConfigScope::Local };

            run_config_wizard(scope)?;
        }
        "plugin" => {
            let global = sub_args.contains(&"--global".to_string()) || sub_args.contains(&"-g".to_string());
            let scope = if global { ConfigScope::Global } else { ConfigScope::Local };
            // 提取 plugin 子命令的参数（去掉 "plugin" 本身和 --global/-g flag）
            let plugin_args: Vec<&str> = sub_args[1..]
                .iter()
                .filter(|a| *a != "--global" && *a != "-g")
                .map(|s| s.as_str())
                .collect();
            handle_plugin_command(&plugin_args, scope)?;
        }
        _ => {
            return Err(CliError::InvalidArgs(format!(
                "未知的 config 子命令: '{}'。请使用 set, get, list, unset, reset, wizard, plugin 之一。",
                sub_cmd
            )));
        }
    }

    Ok(())
}

/// 运行交互式配置向导
fn run_config_wizard(scope: ConfigScope) -> Result<(), CliError> {
    use std::io::{Write, stdin, stdout};

    println!("\x1b[1;36m====================================================\x1b[0m");
    println!("\x1b[1;36m    🚀 TokenSlim 交互式配置向导 (Wizard) 🚀\x1b[0m");
    println!("\x1b[1;36m====================================================\x1b[0m");
    println!(
        "\x1b[90m正在为 {} 配置进行设置...\x1b[0m\n",
        if scope == ConfigScope::Global { "全局" } else { "当前项目" }
    );

    let mut answers = HashMap::new();

    // 1. general.preset
    println!("\x1b[1m1. 选择压缩预设配置 (general.preset)\x1b[0m");
    println!("   压缩预设控制了 TokenSlim 的降噪策略，影响压缩速度与语义完整性。");
    println!("   [\x1b[32m1\x1b[0m] balanced : \x1b[32m均衡模式\x1b[0m (默认，平衡压缩率与解析速度)");
    println!("   [\x1b[32m2\x1b[0m] fast     : \x1b[33m速度优先\x1b[0m (关闭重排，追求极速)");
    println!("   [\x1b[32m3\x1b[0m] ai       : \x1b[36mAI 信号优先\x1b[0m (全力保留故障现场与最大上下文)");

    let default_preset = ConfigManager::get_value("general.preset").unwrap_or_else(|| "balanced".to_string());
    print!("👉 请选择 (1-3) [\x1b[90m默认: {}\x1b[0m]: ", default_preset);
    stdout().flush().unwrap();

    let mut input = String::new();
    stdin().read_line(&mut input).unwrap();
    let choice = input.trim();
    let preset_val = match choice {
        "1" => "balanced",
        "2" => "fast",
        "3" => "ai",
        "" => &default_preset,
        _ => {
            println!("⚠️ 输入无效，自动使用默认值: {}", default_preset);
            &default_preset
        }
    };
    answers.insert("general.preset", preset_val.to_string());
    println!("✨ \x1b[32m已选择: {}\x1b[0m\n", preset_val);

    // 2. compression.reorder
    println!("\x1b[1m2. 是否启用全局日志重排? (compression.reorder)\x1b[0m");
    println!("   在并发构建时（如 make -jN 或 parallel build），多线程交织会导致输出日志乱序。");
    println!("   启用重排可智能重组依赖关系日志以消除并发交织干扰。");

    let default_reorder = ConfigManager::get_value("compression.reorder").unwrap_or_else(|| "true".to_string());
    print!("👉 是否启用 (true/false) [\x1b[90m默认: {}\x1b[0m]: ", default_reorder);
    stdout().flush().unwrap();

    let mut input = String::new();
    stdin().read_line(&mut input).unwrap();
    let choice = input.trim().to_lowercase();
    let reorder_val = if choice.is_empty() {
        default_reorder
    } else if choice == "true" || choice == "t" || choice == "y" || choice == "1" {
        "true".to_string()
    } else {
        "false".to_string()
    };
    answers.insert("compression.reorder", reorder_val.clone());
    println!("✨ \x1b[32m已选择: {}\x1b[0m\n", reorder_val);

    // 3. encoding.force_utf8
    println!("\x1b[1m3. 是否强制 UTF-8 编码输出? (encoding.force_utf8)\x1b[0m");
    println!("   在 Windows CMD/PowerShell 环境中，日志可能使用 GBK。强制转为 UTF-8 能防止下游乱码。");

    let default_utf8 = ConfigManager::get_value("encoding.force_utf8").unwrap_or_else(|| "true".to_string());
    print!("👉 是否强制 (true/false) [\x1b[90m默认: {}\x1b[0m]: ", default_utf8);
    stdout().flush().unwrap();

    let mut input = String::new();
    stdin().read_line(&mut input).unwrap();
    let choice = input.trim().to_lowercase();
    let utf8_val = if choice.is_empty() {
        default_utf8
    } else if choice == "true" || choice == "t" || choice == "y" || choice == "1" {
        "true".to_string()
    } else {
        "false".to_string()
    };
    answers.insert("encoding.force_utf8", utf8_val.clone());
    println!("✨ \x1b[32m已选择: {}\x1b[0m\n", utf8_val);

    // 写入配置
    println!("💾 正在写入配置...");
    for (k, v) in answers {
        ConfigManager::set_value(scope, k, &v)
            .map_err(|e| CliError::Config(format!("写入键 '{}' 失败: {}", k, e)))?;
    }

    println!("\n\x1b[1;32m🎉 配置成功！TokenSlim 已全部设置完毕。 🎉\x1b[0m");
    println!("\x1b[90m您可以使用 `tokenslim config list` 随时查看最终生效的设置。\x1b[0m");
    println!("\x1b[1;36m====================================================\x1b[0m");
    Ok(())
}

// ============================================================================
// 插件配置管理子命令（Task 18）
// ============================================================================

/// 处理 `tokenslim config plugin` 子命令分发
fn handle_plugin_command(args: &[&str], scope: ConfigScope) -> Result<(), CliError> {
    if args.is_empty() {
        print_plugin_usage();
        return Ok(());
    }

    match args[0] {
        "enable" => {
            if args.len() < 2 {
                return Err(CliError::InvalidArgs(
                    "enable 命令需要指定插件名。例如: tokenslim config plugin enable gcc_log_plugin".to_string()
                ));
            }
            plugin_enable(args[1], scope)?;
        }
        "disable" => {
            if args.len() < 2 {
                return Err(CliError::InvalidArgs(
                    "disable 命令需要指定插件名。例如: tokenslim config plugin disable gcc_log_plugin".to_string()
                ));
            }
            plugin_disable(args[1], scope)?;
        }
        "status" => {
            let plugin_name = if args.len() >= 2 { Some(args[1]) } else { None };
            plugin_status(plugin_name, scope)?;
        }
        "reset" => {
            plugin_reset(scope)?;
        }
        "get" => {
            if args.len() < 3 {
                return Err(CliError::InvalidArgs(
                    "get 命令需要指定插件名和参数名。例如: tokenslim config plugin get gcc_log_plugin convert_timestamps".to_string()
                ));
            }
            plugin_get_param(args[1], args[2], scope)?;
        }
        "set" => {
            if args.len() < 4 {
                return Err(CliError::InvalidArgs(
                    "set 命令需要指定插件名、参数名和值。例如: tokenslim config plugin set gcc_log_plugin convert_timestamps false".to_string()
                ));
            }
            plugin_set_param(args[1], args[2], args[3], scope)?;
        }
        "list-params" | "list_params" => {
            if args.len() < 2 {
                return Err(CliError::InvalidArgs(
                    "list-params 命令需要指定插件名。例如: tokenslim config plugin list-params gcc_log_plugin".to_string()
                ));
            }
            plugin_list_params(args[1])?;
        }
        other => {
            return Err(CliError::InvalidArgs(format!(
                "未知的 plugin 子命令: '{}'。请使用 enable, disable, status, reset, get, set, list-params 之一。",
                other
            )));
        }
    }

    Ok(())
}

/// 打印 plugin 子命令用法
fn print_plugin_usage() {
    println!("tokenslim config plugin

管理压缩插件的启用/禁用和参数配置

用法:
  tokenslim config plugin <subcommand> [args...] [--global|-g]

子命令:
  enable <plugin-name>                 启用指定插件
  disable <plugin-name>                禁用指定插件
  status [<plugin-name>]               查看插件启用状态（不指定则列出全部）
  reset                                重置所有插件为默认启用状态
  get <plugin-name> <param>            获取插件参数值
  set <plugin-name> <param> <value>    设置插件参数值
  list-params <plugin-name>            列出插件所有可配置参数

示例:
  tokenslim config plugin status
  tokenslim config plugin disable gcc_log_plugin
  tokenslim config plugin set gcc_log_plugin convert_timestamps false
  tokenslim config plugin list-params gcc_log_plugin");
}

/// 获取仓库内置的 plugins.toml 路径
fn find_repo_plugins_toml() -> Option<PathBuf> {
    // 直接检查常见路径（开发目录、exe 目录）
    let candidates = [
        PathBuf::from("config/plugins.toml"),
        PathBuf::from("./config/plugins.toml"),
    ];
    for c in &candidates {
        if c.exists() {
            return Some(c.clone());
        }
    }
    // 尝试从 exe 目录推导
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent().and_then(|p| p.parent()) {
            let candidate = parent.join("config/plugins.toml");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 从 plugins.toml 读取默认启用的插件名列表
fn read_default_enabled_list() -> Vec<String> {
    let Some(path) = find_repo_plugins_toml() else { return Vec::new(); };
    let Ok(content) = std::fs::read_to_string(&path) else { return Vec::new(); };
    let Ok(doc) = content.parse::<toml_edit::DocumentMut>() else { return Vec::new(); };

    doc.get("plugins")
        .and_then(|p| p.get("enabled"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// 从用户配置文件读取被禁用的插件名列表
fn read_user_disabled_list(scope: ConfigScope) -> Vec<String> {
    let path = match scope {
        ConfigScope::Global => global_config_path(),
        ConfigScope::Local => local_config_path(),
    };
    let Some(path) = path else { return Vec::new(); };
    let Ok(content) = std::fs::read_to_string(&path) else { return Vec::new(); };
    let Ok(doc) = content.parse::<toml_edit::DocumentMut>() else { return Vec::new(); };

    doc.get("plugins")
        .and_then(|p| p.get("disabled"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// 获取用户配置文件路径，不存在则创建
fn ensure_user_config_path(scope: ConfigScope) -> Result<PathBuf, CliError> {
    let path = match scope {
        ConfigScope::Global => global_config_path()
            .ok_or_else(|| CliError::Config("无法获取全局配置路径".to_string()))?,
        ConfigScope::Local => local_config_path()
            .ok_or_else(|| CliError::Config("无法获取本地配置路径".to_string()))?,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CliError::Config(format!("创建配置目录失败: {}", e)))?;
    }
    Ok(path)
}

/// 启用插件：将其从用户 disabled 列表中移除
fn plugin_enable(plugin_name: &str, scope: ConfigScope) -> Result<(), CliError> {
    let defaults = read_default_enabled_list();
    if !defaults.iter().any(|p| p == plugin_name) {
        return Err(CliError::Config(format!("未知插件: '{}'。使用 `tokenslim config plugin status` 查看可用插件列表。", plugin_name)));
    }

    let path = ensure_user_config_path(scope)?;
    let mut doc = if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| CliError::Config(format!("读取配置失败: {}", e)))?;
        content.parse::<toml_edit::DocumentMut>()
            .map_err(|e| CliError::Config(format!("解析配置失败: {}", e)))?
    } else {
        toml_edit::DocumentMut::new()
    };

    // 从 disabled 数组中移除该插件
    let removed = if let Some(disabled) = doc.get_mut("plugins")
        .and_then(|p| p.get_mut("disabled"))
        .and_then(|v| v.as_array_mut())
    {
        let before = disabled.len();
        *disabled = disabled.iter()
            .filter(|v| v.as_str().map(|s| s != plugin_name).unwrap_or(true))
            .cloned()
            .collect::<toml_edit::Array>();
        before != disabled.len()
    } else {
        false
    };

    if removed {
        std::fs::write(&path, doc.to_string())
            .map_err(|e| CliError::Config(format!("写入配置失败: {}", e)))?;
        println!("✅ 已启用插件 '{}'", plugin_name);
    } else {
        println!("ℹ️ 插件 '{}' 已经是启用状态，无需操作", plugin_name);
    }

    Ok(())
}

/// 禁用插件：将其加入用户 disabled 列表
fn plugin_disable(plugin_name: &str, scope: ConfigScope) -> Result<(), CliError> {
    let defaults = read_default_enabled_list();
    if !defaults.iter().any(|p| p == plugin_name) {
        return Err(CliError::Config(format!("未知插件: '{}'。使用 `tokenslim config plugin status` 查看可用插件列表。", plugin_name)));
    }

    let path = ensure_user_config_path(scope)?;
    let mut doc = if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| CliError::Config(format!("读取配置失败: {}", e)))?;
        content.parse::<toml_edit::DocumentMut>()
            .map_err(|e| CliError::Config(format!("解析配置失败: {}", e)))?
    } else {
        toml_edit::DocumentMut::new()
    };

    // 确保 [plugins] 表存在
    if doc.get("plugins").is_none() {
        doc["plugins"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    // 检查是否已在 disabled 数组中
    let already_disabled = doc.get("plugins")
        .and_then(|p| p.get("disabled"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().any(|v| v.as_str() == Some(plugin_name)))
        .unwrap_or(false);

    if already_disabled {
        println!("ℹ️ 插件 '{}' 已经是禁用状态，无需操作", plugin_name);
        return Ok(());
    }

    // 追加到 disabled 数组
    if let Some(plugins_table) = doc["plugins"].as_table_mut() {
        let disabled_arr = if let Some(existing) = plugins_table.get_mut("disabled") {
            existing
        } else {
            plugins_table.insert("disabled", toml_edit::Item::Value(toml_edit::Value::Array(toml_edit::Array::new())));
            plugins_table.get_mut("disabled").unwrap()
        };
        if let Some(arr) = disabled_arr.as_array_mut() {
            arr.push(plugin_name);
        }
    }

    std::fs::write(&path, doc.to_string())
        .map_err(|e| CliError::Config(format!("写入配置失败: {}", e)))?;
    println!("✅ 已禁用插件 '{}'", plugin_name);

    Ok(())
}

/// 显示插件状态
fn plugin_status(plugin_name: Option<&str>, scope: ConfigScope) -> Result<(), CliError> {
    let defaults = read_default_enabled_list();
    let disabled = read_user_disabled_list(scope);

    if defaults.is_empty() {
        return Err(CliError::Config("无法读取内置插件列表（config/plugins.toml 不存在或解析失败）".to_string()));
    }

    if let Some(name) = plugin_name {
        // 显示单个插件状态
        if !defaults.iter().any(|p| p == name) {
            return Err(CliError::Config(format!("未知插件: '{}'", name)));
        }
        let is_enabled = !disabled.contains(&name.to_string());
        let status_icon = if is_enabled { "✅" } else { "🚫" };
        let status_text = if is_enabled { "已启用" } else { "已禁用" };
        println!("{} {} — {}", status_icon, name, status_text);
    } else {
        // 显示全部插件状态
        let enabled_count = defaults.iter().filter(|p| !disabled.contains(p)).count();
        let disabled_count = disabled.len();
        println!("=== 插件状态列表 ({} 已启用, {} 已禁用) ===\n", enabled_count, disabled_count);
        for plugin in &defaults {
            let is_enabled = !disabled.contains(plugin);
            let status_icon = if is_enabled { "✅" } else { "🚫" };
            println!("  {} {}", status_icon, plugin);
        }
    }

    Ok(())
}

/// 重置插件配置：移除用户的所有插件覆盖
fn plugin_reset(scope: ConfigScope) -> Result<(), CliError> {
    let path = ensure_user_config_path(scope)?;

    if !path.exists() {
        println!("ℹ️ 没有用户插件配置需要重置");
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| CliError::Config(format!("读取配置失败: {}", e)))?;
    let mut doc = content.parse::<toml_edit::DocumentMut>()
        .map_err(|e| CliError::Config(format!("解析配置失败: {}", e)))?;

    // 移除 [plugins] 表（包含 disabled 数组和插件参数覆盖）
    let had_plugins = doc.remove("plugins").is_some();

    if had_plugins {
        std::fs::write(&path, doc.to_string())
            .map_err(|e| CliError::Config(format!("写入配置失败: {}", e)))?;
        println!("✅ 已重置所有插件配置为默认状态");
    } else {
        println!("ℹ️ 没有用户插件配置需要重置");
    }

    Ok(())
}

/// 获取插件参数值
fn plugin_get_param(plugin_name: &str, param_name: &str, scope: ConfigScope) -> Result<(), CliError> {
    // 合并配置：默认值 → 用户覆盖
    let defaults = read_plugin_params_from_toml(plugin_name, find_repo_plugins_toml().as_deref());
    let user_params = read_plugin_params_from_scope(plugin_name, scope);

    if defaults.is_empty() && user_params.is_empty() {
        return Err(CliError::Config(format!("插件 '{}' 不存在或无可配置参数", plugin_name)));
    }

    // 用户覆盖优先
    if let Some(val) = user_params.get(param_name) {
        println!("{} = {} (用户覆盖)", param_name, val);
    } else if let Some(val) = defaults.get(param_name) {
        println!("{} = {} (默认值)", param_name, val);
    } else {
        println!("(参数 '{}' 未设置)", param_name);
    }

    Ok(())
}

/// 设置插件参数值
fn plugin_set_param(plugin_name: &str, param_name: &str, value: &str, scope: ConfigScope) -> Result<(), CliError> {
    // 验证插件存在
    let defaults = read_default_enabled_list();
    if !defaults.iter().any(|p| p == plugin_name) {
        return Err(CliError::Config(format!("未知插件: '{}'", plugin_name)));
    }

    let path = ensure_user_config_path(scope)?;
    let mut doc = if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| CliError::Config(format!("读取配置失败: {}", e)))?;
        content.parse::<toml_edit::DocumentMut>()
            .map_err(|e| CliError::Config(format!("解析配置失败: {}", e)))?
    } else {
        toml_edit::DocumentMut::new()
    };

    // 确保 [plugins.<plugin_name>] 表存在
    if doc.get("plugins").is_none() {
        doc["plugins"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    let plugins_table = doc["plugins"].as_table_mut().unwrap();
    if plugins_table.get(plugin_name).is_none() {
        plugins_table.insert(plugin_name, toml_edit::Item::Table(toml_edit::Table::new()));
    }

    // 根据值类型设置（布尔或字符串）
    if let Some(plugin_table) = plugins_table.get_mut(plugin_name).and_then(|t| t.as_table_mut()) {
        let value_item = if value == "true" || value == "false" {
            toml_edit::Item::Value(toml_edit::Value::Boolean(toml_edit::Formatted::new(value == "true")))
        } else if let Ok(n) = value.parse::<i64>() {
            toml_edit::Item::Value(toml_edit::Value::Integer(toml_edit::Formatted::new(n)))
        } else {
            toml_edit::Item::Value(toml_edit::Value::String(toml_edit::Formatted::new(value.to_string())))
        };
        plugin_table.insert(param_name, value_item);
    }

    std::fs::write(&path, doc.to_string())
        .map_err(|e| CliError::Config(format!("写入配置失败: {}", e)))?;
    println!("✅ 已设置 {}.{} = {}", plugin_name, param_name, value);

    Ok(())
}

/// 列出插件所有可配置参数（默认值 + 用户覆盖）
fn plugin_list_params(plugin_name: &str) -> Result<(), CliError> {
    let repo_path = find_repo_plugins_toml();
    let defaults = read_plugin_params_from_toml(plugin_name, repo_path.as_deref());

    if defaults.is_empty() {
        // 检查插件是否存在
        let all_plugins = read_default_enabled_list();
        if !all_plugins.iter().any(|p| p == plugin_name) {
            return Err(CliError::Config(format!("未知插件: '{}'", plugin_name)));
        }
        println!("插件 '{}' 没有可配置参数（使用默认值）", plugin_name);
        return Ok(());
    }

    println!("=== {} 可配置参数 ===\n", plugin_name);
    for (key, val) in &defaults {
        println!("  {} = {}", key, val);
    }

    Ok(())
}

/// 从指定 plugins.toml 文件中读取某插件的参数
fn read_plugin_params_from_toml(plugin_name: &str, path: Option<&Path>) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let Some(path) = path else { return result; };
    let Ok(content) = std::fs::read_to_string(path) else { return result; };
    let Ok(doc) = content.parse::<toml_edit::DocumentMut>() else { return result; };

    if let Some(plugin_section) = doc.get("plugins").and_then(|p| p.get(plugin_name)) {
        if let Some(table) = plugin_section.as_table() {
            for (k, v) in table.iter() {
                let str_val = if let Some(b) = v.as_bool() {
                    b.to_string()
                } else if let Some(i) = v.as_integer() {
                    i.to_string()
                } else if let Some(f) = v.as_float() {
                    f.to_string()
                } else if let Some(s) = v.as_str() {
                    s.to_string()
                } else {
                    continue;
                };
                result.insert(k.to_string(), str_val);
            }
        }
    }
    result
}

/// 从用户配置作用域读取插件参数覆盖
fn read_plugin_params_from_scope(plugin_name: &str, scope: ConfigScope) -> HashMap<String, String> {
    let path = match scope {
        ConfigScope::Global => global_config_path(),
        ConfigScope::Local => local_config_path(),
    };
    let Some(path) = path else { return HashMap::new(); };
    read_plugin_params_from_toml(plugin_name, Some(&path))
}

