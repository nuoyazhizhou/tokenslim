//! cli doctor 子命令

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


pub(crate) fn handle_doctor_encoding_action(args: &CliArgs) -> Result<bool, CliError> {
    use crate::core::doctor_encoding::{
        generate_fix_commands, run_encoding_doctor, DoctorReportFormat,
    };

    if args.fix {
        let fix_output = generate_fix_commands().map_err(|e| CliError::Config(e))?;
        args.emit_text(&fix_output, None)?;
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
    args.emit_text(&report, None)?;
    Ok(true)
}


pub(crate) fn handle_doctor_workspace_action(args: &CliArgs) -> Result<bool, CliError> {
    use crate::core::doctor_workspace::{run_workspace_doctor, WorkspaceReportFormat};

    let format = match args.doctor_format {
        crate::cli::types::DoctorOutputFormat::Text => WorkspaceReportFormat::Text,
        crate::cli::types::DoctorOutputFormat::Json => WorkspaceReportFormat::Json,
        crate::cli::types::DoctorOutputFormat::Llm => WorkspaceReportFormat::Llm,
        crate::cli::types::DoctorOutputFormat::JsonMin => WorkspaceReportFormat::JsonMin,
    };

    let report = run_workspace_doctor(format, args.doctor_strict).map_err(CliError::Config)?;
    args.emit_text(&report, None)?;
    Ok(true)
}


pub(crate) fn handle_doctor_rule_action(args: &CliArgs) -> Result<bool, CliError> {
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
                    args.emit_text(&output, None)?;
                    found = true;
                }
            }
        }
    }
    if !found {
        args.emit_text(
            &format!(
                "No rule configuration found. Searched: {}",
                rule_files.join(", ")
            ),
            None,
        )?;
    }
    Ok(true)
}


pub(crate) fn handle_doctor_env_action(args: &CliArgs) -> Result<bool, CliError> {
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
    args.emit_text(&output, None)?;
    Ok(true)
}

