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

