//! cli config 子命令

use crate::cli::common::*;
use crate::cli::types::*;
use crate::core::compression::{CompressionMetadata, CompressionOutput, Token};
use crate::core::compression_context::CompressionContext;
use crate::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
use crate::core::config_manager::{ConfigManager, ConfigScope};
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
use std::collections::HashMap;
use std::io::{self, IsTerminal, Read};

pub(crate) const HOOK_BEGIN: &str = "# >>> tokenslim hook >>>";
pub(crate) const HOOK_END: &str = "# <<< tokenslim hook <<<";

/// 解析可选的 --hook-shell 参数：为 None 时返回 None；否则按 bash|zsh|fish 解析，不支持的值报 InvalidArgs。
pub(crate) fn parse_optional_hook_shell(
    shell: Option<&str>,
) -> Result<Option<HookShell>, CliError> {
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

/// 探测当前 shell：按 $SHELL(zsh/fish/bash) 判定，否则依据 PSModulePath 或 Windows 判定 PowerShell，兜底返回 Bash。
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

/// 解析用户主目录：优先取 $HOME，回退到 $USERPROFILE；两者皆缺失则报错 Config。
pub(crate) fn resolve_home_dir() -> Result<std::path::PathBuf, CliError> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .map_err(|_| CliError::Config("unable to resolve HOME/USERPROFILE".to_string()))
}

/// 返回指定 shell 的 rc 配置文件路径列表：PowerShell 探测并包含 $PROFILE；
/// 类 Unix 返回对应点文件(.bashrc/.zshrc/.config/fish/config.fish)。
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

/// 生成 hook 代码块：以 HOOK_BEGIN/HOOK_END 标记包裹 generate_hook_content 生成的 shell 钩子内容。
pub(crate) fn hook_block(shell: HookShell) -> String {
    let content = crate::core::init_command::generate_hook_content(shell.as_str());
    format!("{HOOK_BEGIN}\n{content}\n{HOOK_END}\n")
}

/// 从 rc 内容中移除 TokenSlim hook 块：定位 HOOK_BEGIN..HOOK_END 区间并删除(含标记行)，
/// 未找到标记则返回原内容不变。
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

/// 安装 hook 到各 rc 文件：dry-run 仅打印计划；否则先清除旧 hook 块，
/// 拼接新块写回文件，最后提示相应的重新加载命令。
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

/// 检查 hook 安装状态：遍历指定 shell 的各 rc 文件，判断是否存在 HOOK_BEGIN 标记，
/// 逐文件报告 installed 并汇总该 shell 是否整体已安装。
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

/// 卸载 hook：dry-run 仅打印计划；否则从各 rc 文件移除 TokenSlim hook 块，
/// 写回变更并提示重启终端使生效。
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

/// 处理 --inject 动作：若与 encoding/rule/env 诊断同时指定则报 InvalidArgs；
/// 否则调用 inject_context_file 注入工作区上下文文件并打印结果。
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

/// 处理 gain(压缩收益统计)动作：按 --gain-json / --gain-daily / --gain-by-filter 组合，
/// 渲染汇总/按日/按过滤器的压缩收益报告(纯文本或 JSON)并打印。
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
                    "set 命令需要指定键和值。例如: tokenslim config set general.preset fast"
                        .to_string(),
                ));
            }
            let key = sub_args[1].as_str();
            let value = sub_args[2].as_str();
            let global =
                sub_args.contains(&"--global".to_string()) || sub_args.contains(&"-g".to_string());
            let scope = if global {
                ConfigScope::Global
            } else {
                ConfigScope::Local
            };

            ConfigManager::set_value(scope, key, value)
                .map_err(|e| CliError::Config(format!("设置配置失败: {}", e)))?;
            println!(
                "✅ 成功将配置项 '{}' 设置为 '{}' ({})",
                key,
                value,
                if global { "全局" } else { "项目本地" }
            );
        }
        "get" => {
            if sub_args.len() < 2 {
                return Err(CliError::InvalidArgs(
                    "get 命令需要指定键。例如: tokenslim config get general.preset".to_string(),
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
                    "unset 命令需要指定键。例如: tokenslim config unset general.preset".to_string(),
                ));
            }
            let key = sub_args[1].as_str();
            let global =
                sub_args.contains(&"--global".to_string()) || sub_args.contains(&"-g".to_string());
            let scope = if global {
                ConfigScope::Global
            } else {
                ConfigScope::Local
            };

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
            let global =
                sub_args.contains(&"--global".to_string()) || sub_args.contains(&"-g".to_string());
            let scope = if global {
                ConfigScope::Global
            } else {
                ConfigScope::Local
            };

            ConfigManager::reset(scope)
                .map_err(|e| CliError::Config(format!("重置配置失败: {}", e)))?;
            println!(
                "✅ 成功清空并重置 {} 配置文件",
                if global { "全局" } else { "项目本地" }
            );
        }
        "wizard" => {
            let global =
                sub_args.contains(&"--global".to_string()) || sub_args.contains(&"-g".to_string());
            let scope = if global {
                ConfigScope::Global
            } else {
                ConfigScope::Local
            };

            run_config_wizard(scope)?;
        }
        "plugin" => {
            let global =
                sub_args.contains(&"--global".to_string()) || sub_args.contains(&"-g".to_string());
            let scope = if global {
                ConfigScope::Global
            } else {
                ConfigScope::Local
            };
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
    use std::io::{stdin, stdout, Write};

    println!("\x1b[1;36m====================================================\x1b[0m");
    println!("\x1b[1;36m    🚀 TokenSlim 交互式配置向导 (Wizard) 🚀\x1b[0m");
    println!("\x1b[1;36m====================================================\x1b[0m");
    println!(
        "\x1b[90m正在为 {} 配置进行设置...\x1b[0m\n",
        if scope == ConfigScope::Global {
            "全局"
        } else {
            "当前项目"
        }
    );

    let mut answers = HashMap::new();

    // 1. general.preset
    println!("\x1b[1m1. 选择压缩预设配置 (general.preset)\x1b[0m");
    println!("   压缩预设控制了 TokenSlim 的降噪策略，影响压缩速度与语义完整性。");
    println!(
        "   [\x1b[32m1\x1b[0m] balanced : \x1b[32m均衡模式\x1b[0m (默认，平衡压缩率与解析速度)"
    );
    println!("   [\x1b[32m2\x1b[0m] fast     : \x1b[33m速度优先\x1b[0m (关闭重排，追求极速)");
    println!("   [\x1b[32m3\x1b[0m] ai       : \x1b[36mAI 信号优先\x1b[0m (全力保留故障现场与最大上下文)");

    let default_preset =
        ConfigManager::get_value("general.preset").unwrap_or_else(|| "balanced".to_string());
    print!(
        "👉 请选择 (1-3) [\x1b[90m默认: {}\x1b[0m]: ",
        default_preset
    );
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

    let default_reorder =
        ConfigManager::get_value("compression.reorder").unwrap_or_else(|| "true".to_string());
    print!(
        "👉 是否启用 (true/false) [\x1b[90m默认: {}\x1b[0m]: ",
        default_reorder
    );
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
    println!(
        "   在 Windows CMD/PowerShell 环境中，日志可能使用 GBK。强制转为 UTF-8 能防止下游乱码。"
    );

    let default_utf8 =
        ConfigManager::get_value("encoding.force_utf8").unwrap_or_else(|| "true".to_string());
    print!(
        "👉 是否强制 (true/false) [\x1b[90m默认: {}\x1b[0m]: ",
        default_utf8
    );
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
                    "enable 命令需要指定插件名。例如: tokenslim config plugin enable gcc_log"
                        .to_string(),
                ));
            }
            plugin_enable(args[1], scope)?;
        }
        "disable" => {
            if args.len() < 2 {
                return Err(CliError::InvalidArgs(
                    "disable 命令需要指定插件名。例如: tokenslim config plugin disable gcc_log"
                        .to_string(),
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
        other => {
            return Err(CliError::InvalidArgs(format!(
                "未知的 plugin 子命令: '{}'。请使用 enable, disable, status, reset 之一。",
                other
            )));
        }
    }

    Ok(())
}

/// 打印 plugin 子命令用法
fn print_plugin_usage() {
    println!(
        "tokenslim config plugin

管理压缩插件的启用/禁用

用法:
  tokenslim config plugin <subcommand> [args...] [--global|-g]

子命令:
  enable <plugin-name>                 启用指定插件
  disable <plugin-name>                禁用指定插件
  status [<plugin-name>]               查看插件启用状态（不指定则列出全部）
  reset                                重置所有插件为默认启用状态

示例:
  tokenslim config plugin status
  tokenslim config plugin disable gcc_log
  tokenslim config plugin enable gcc_log"
    );
}

/// 返回所有插件配置清单（含禁用），按名称排序。
/// P3-179：统一以 plugin_config_loader 的 JSON 配置为权威源，
/// 取代自建 plugins.toml 解析；`enabled` 字段已叠加用户覆盖键 `plugins.{name}.enabled`。
fn load_all_plugin_configs() -> Vec<crate::core::plugin_config_loader::PluginConfigFile> {
    let loader = plugin_config_loader::PluginConfigLoader::new();
    let mut configs = loader.load_all_raw_configs();
    configs.sort_by(|a, b| a.name.cmp(&b.name));
    configs
}

/// 启用插件：写入 `plugins.{name}.enabled = true`（运行时 load_config 消费的覆盖键）。
fn plugin_enable(plugin_name: &str, scope: ConfigScope) -> Result<(), CliError> {
    let configs = load_all_plugin_configs();
    if !configs.iter().any(|c| c.name == plugin_name) {
        return Err(CliError::Config(format!(
            "未知插件: '{}'。使用 `tokenslim config plugin status` 查看可用插件列表。",
            plugin_name
        )));
    }

    ConfigManager::set_value(scope, &format!("plugins.{}.enabled", plugin_name), "true")
        .map_err(|e| CliError::Config(format!("设置配置失败: {}", e)))?;
    println!("✅ 已启用插件 '{}'", plugin_name);
    Ok(())
}

/// 禁用插件：写入 `plugins.{name}.enabled = false`（运行时 load_config 消费的覆盖键）。
fn plugin_disable(plugin_name: &str, scope: ConfigScope) -> Result<(), CliError> {
    let configs = load_all_plugin_configs();
    if !configs.iter().any(|c| c.name == plugin_name) {
        return Err(CliError::Config(format!(
            "未知插件: '{}'。使用 `tokenslim config plugin status` 查看可用插件列表。",
            plugin_name
        )));
    }

    ConfigManager::set_value(scope, &format!("plugins.{}.enabled", plugin_name), "false")
        .map_err(|e| CliError::Config(format!("设置配置失败: {}", e)))?;
    println!("✅ 已禁用插件 '{}'", plugin_name);
    Ok(())
}

/// 显示插件状态
/// P3-179：以 plugin_config_loader 的 JSON 配置为权威源，`enabled` 已叠加
/// 用户覆盖键 `plugins.{name}.enabled`（load_config 内合并），无需再读 TOML。
fn plugin_status(plugin_name: Option<&str>, _scope: ConfigScope) -> Result<(), CliError> {
    let configs = load_all_plugin_configs();

    if configs.is_empty() {
        return Err(CliError::Config(
            "无法读取插件配置（config/plugins 目录不存在或所有配置文件解析失败）".to_string(),
        ));
    }

    if let Some(name) = plugin_name {
        // 显示单个插件状态
        let Some(config) = configs.iter().find(|c| c.name == name) else {
            return Err(CliError::Config(format!(
                "未知插件: '{}'。使用 `tokenslim config plugin status` 查看可用插件列表。",
                name
            )));
        };
        let status_icon = if config.enabled { "✅" } else { "🚫" };
        let status_text = if config.enabled {
            "已启用"
        } else {
            "已禁用"
        };
        println!("{} {} — {}", status_icon, name, status_text);
    } else {
        // 显示全部插件状态
        let enabled_count = configs.iter().filter(|c| c.enabled).count();
        let disabled_count = configs.len() - enabled_count;
        println!(
            "=== 插件状态列表 ({} 已启用, {} 已禁用) ===\n",
            enabled_count, disabled_count
        );
        for config in &configs {
            let status_icon = if config.enabled { "✅" } else { "🚫" };
            println!("  {} {}", status_icon, config.name);
        }
    }

    Ok(())
}

/// 重置插件配置：移除用户对全部插件的启用/禁用覆盖键
/// `plugins.{name}.enabled`，恢复各插件 JSON 配置中的默认 enabled 状态。
/// P3-179：不再整表删除 `[plugins]`（避免误伤合法的 `plugins.{name}.enabled` 覆盖键），
/// 仅逐个 unset 覆盖键。
fn plugin_reset(scope: ConfigScope) -> Result<(), CliError> {
    let configs = load_all_plugin_configs();

    let mut removed = 0usize;
    for config in &configs {
        let key = format!("plugins.{}.enabled", config.name);
        if ConfigManager::unset_value(scope, &key)
            .map_err(|e| CliError::Config(format!("重置配置失败: {}", e)))?
        {
            removed += 1;
        }
    }

    if removed > 0 {
        println!("✅ 已重置 {} 个插件的启用状态为默认", removed);
    } else {
        println!("ℹ️ 没有用户插件覆盖需要重置");
    }

    Ok(())
}
