//! cli benchmark 子命令

use crate::cli::common::*;
use crate::cli::types::*;
use crate::core::compression::{CompressionMetadata, CompressionOutput, Token};
use crate::core::compression_context::CompressionContext;
use crate::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
use crate::core::dedup_engine::{DedupConfig, DedupEngine};
use crate::core::dictionary_engine::DictionaryEngine;
use crate::core::metrics::{MetricsCollector, MetricsConfig};
use crate::core::path_optimizer::methods::{
    optimize_path_dictionary_blocks, optimize_path_dictionary_blocks_with_options,
    PathDictionaryOptions,
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


pub(crate) fn is_verify_fixture_file(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .as_deref(),
        Some("log") | Some("fixture") | Some("input")
    )
}


pub(crate) fn expected_file_for_fixture(
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


pub(crate) fn collect_verify_fixture_files(
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


pub(crate) fn verify_single_fixture_with_plugin(
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


pub(crate) fn load_verify_plugin(
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


pub(crate) fn run_static_rule_verify_file_mode(
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


pub(crate) fn run_static_rule_verify_directory_mode(
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


pub(crate) fn run_static_rule_verify(
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
    use crate::cli::app::*;
    use crate::cli::commands::{
        compress::*, config::*, decompress::*, doctor::*, export::*, repair::*, run::*,
    };
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
            json: false,
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
    fn should_show_quick_usage_for_verbose_flag_alone() {
        // `-v` / `--verbose` 单独使用且无 position args 时，
        // 不应进入 pipeline 阻塞 stdin，应直接输出 global usage
        assert!(should_show_quick_usage(
            &["tokenslim.exe".to_string(), "-v".to_string()],
            true
        ));
        assert!(should_show_quick_usage(
            &["tokenslim.exe".to_string(), "--verbose".to_string()],
            true
        ));
        // 有其他 args 时不应拦截
        assert!(!should_show_quick_usage(
            &[
                "tokenslim.exe".to_string(),
                "-v".to_string(),
                "git".to_string(),
                "status".to_string(),
            ],
            true
        ));
    }

    #[test]
    fn intercept_version_request_handles_dash_capital_v() {
        let argv = vec!["tokenslim.exe".to_string(), "-V".to_string()];
        let out = intercept_version_request(&argv);
        assert!(out.is_some(), "-V should produce version output");
        let text = out.unwrap();
        assert!(text.starts_with("tokenslim.exe "), "got: {text}");
        assert!(text.contains(env!("CARGO_PKG_VERSION")), "got: {text}");
    }

    #[test]
    fn intercept_version_request_handles_long_version_flag() {
        let argv = vec!["tokenslim.exe".to_string(), "--version".to_string()];
        let out = intercept_version_request(&argv);
        assert!(out.is_some(), "--version should produce version output");
    }

    #[test]
    fn intercept_version_request_handles_position_version_subcommand() {
        let argv = vec!["tokenslim.exe".to_string(), "version".to_string()];
        let out = intercept_version_request(&argv);
        assert!(out.is_some(), "`version` should produce version output");
    }

    #[test]
    fn intercept_version_request_ignores_non_version_input() {
        let argv = vec!["tokenslim.exe".to_string(), "git".to_string(), "status".to_string()];
        assert!(intercept_version_request(&argv).is_none());
    }

    #[test]
    fn intercept_version_request_skips_global_flags() {
        // `tokenslim -v version` 等价于 `tokenslim version`
        let argv = vec![
            "tokenslim.exe".to_string(),
            "-v".to_string(),
            "version".to_string(),
        ];
        assert!(intercept_version_request(&argv).is_some());
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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
            json: false,
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


pub(crate) fn handle_verify_rule_action(args: &CliArgs) -> Result<bool, CliError> {
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

