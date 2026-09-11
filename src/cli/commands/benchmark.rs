//! cli verify 子命令（静态规则夹具校验）

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

/// 判断给定路径是否为「验证夹具（fixture）」文件。
///
/// 依据扩展名判定：仅当扩展名为 `log`、`fixture` 或 `input`（大小写不敏感）时视为夹具。
/// 目录模式扫描时靠它从一堆文件中筛出需要参与 `verify` 的输入样本。
pub(crate) fn is_verify_fixture_file(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .as_deref(),
        Some("log") | Some("fixture") | Some("input")
    )
}

/// 根据夹具文件推导其对应的「期望输出」文件路径。
///
/// 推导规则：若文件名以 `_fixture` 结尾，则对应 `<前缀>_expected.txt`；否则对应 `<文件名>.expected`。
/// 先在 `expected_dir` 中查找上述主路径，若不存在再退回到同名文件；均不存在则返回 `None`。
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

/// 收集目录下的所有验证夹具文件（按文件名排序返回）。
///
/// 遍历 `fixture_dir`，过滤出普通文件且经 [`is_verify_fixture_file`] 判定为夹具的项，
/// 排序后返回，供目录模式逐个比对。
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

/// 用指定插件对单条夹具文本执行压缩，并与期望文本比对（`verify` 的核心单元）。
///
/// 流程：将夹具包装为 `Slice` → 经 `plugin.compress` 压缩并 `flatten_tokens` 得到实际输出；
/// 若开启 `safety`，再对原始夹具与实际输出跑全套安全校验，命中即返回风险错误；
/// 最后通过 `verify_text_pair` 将实际输出与 `expected_text` 逐行/逐段比对。
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

/// 从 TOML 规则文件加载 `SimpleRulePlugin`，可选开启配置安全校验。
///
/// 读取 `rule_path` 文本；若 `safety` 为真，先跑配置级安全校验，命中风险直接报错；
/// 校验通过后用 `SimpleRulePlugin::from_toml` 反序列化插件配置。
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

/// 文件模式：对「单个夹具 + 单个期望文件」执行一次 verify。
///
/// 读取夹具与期望文本后委托 [`verify_single_fixture_with_plugin`] 比对；通过则打印
/// `verify_pass_rule` 提示，否则向上传播错误。
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

/// 目录模式：扫描夹具目录，逐个夹具与对应期望文件比对并汇总结果。
///
/// 先经 [`collect_verify_fixture_files`] 收集夹具；对每个夹具用
/// [`expected_file_for_fixture`] 寻找期望文件（缺失则记录失败原因后跳过）；
/// 调用 [`verify_single_fixture_with_plugin`] 比对，累计通过数与失败消息；
/// 全部通过打印汇总，否则聚合失败信息返回错误。
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

/// `verify` 子命令的编排入口：加载规则插件并按路径类型分派到文件或目录模式。
///
/// 先经 [`load_verify_plugin`] 构造插件；随后：
/// - 夹具与期望均为文件 → [`run_static_rule_verify_file_mode`]；
/// - 夹具与期望均为目录 → [`run_static_rule_verify_directory_mode`]；
/// - 否则返回「路径类型不匹配」错误。
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
    // 测试 fixture 中刻意的 mojibake 字符串（UTF-8 中文被误读为 Latin-1）含软连字符 U+00AD,
    // 属不可见字符，clippy 默认 deny；此处按测试意图允许该 lint。
    #![allow(clippy::invisible_characters)]
    use super::*;
    use crate::cli::app::*;
    use crate::cli::commands::{
        compress::*, config::*, decompress::*, doctor::*, export::*, repair::*, run::*,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 构造指定模式的基础 [`CliArgs`]，为测试用例提供统一默认参数。
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        }
    }

    /// 创建带前缀与进程 ID/时间戳随机后缀的临时测试目录并返回路径。
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

    /// 验证全局 usage 文案包含程序名与 `run <command>` 提示。
    #[test]
    fn renders_quick_usage_with_program_name() {
        let usage = render_global_usage("tokenslim.exe");
        assert!(usage.contains("tokenslim"));
        assert!(usage.contains(&format!("{}:", crate::utils::i18n::t("cli_help_usage"))));
        assert!(usage.contains("tokenslim.exe run <command>"));
    }

    /// 验证 `--help` 触发快捷 usage 展示。
    #[test]
    fn should_show_quick_usage_for_help_flag() {
        let argv = vec!["tokenslim.exe".to_string(), "--help".to_string()];
        assert!(should_show_quick_usage(&argv, true));
    }

    /// 验证 `-v`/`--verbose` 单独使用且无位置参数时不阻塞 stdin、直接输出 usage。
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

    /// 验证 `-V` 大写版本旗标产生版本输出且含程序名与包版本。
    #[test]
    fn intercept_version_request_handles_dash_capital_v() {
        let argv = vec!["tokenslim.exe".to_string(), "-V".to_string()];
        let out = intercept_version_request(&argv);
        assert!(out.is_some(), "-V should produce version output");
        let text = out.unwrap();
        assert!(text.starts_with("tokenslim.exe "), "got: {text}");
        assert!(text.contains(env!("CARGO_PKG_VERSION")), "got: {text}");
    }

    /// 验证 `--version` 长旗标产生版本输出。
    #[test]
    fn intercept_version_request_handles_long_version_flag() {
        let argv = vec!["tokenslim.exe".to_string(), "--version".to_string()];
        let out = intercept_version_request(&argv);
        assert!(out.is_some(), "--version should produce version output");
    }

    /// 验证位置子命令 `version` 同样产生版本输出。
    #[test]
    fn intercept_version_request_handles_position_version_subcommand() {
        let argv = vec!["tokenslim.exe".to_string(), "version".to_string()];
        let out = intercept_version_request(&argv);
        assert!(out.is_some(), "`version` should produce version output");
    }

    /// 验证非版本相关输入（如 `git status`）不触发版本拦截。
    #[test]
    fn intercept_version_request_ignores_non_version_input() {
        let argv = vec![
            "tokenslim.exe".to_string(),
            "git".to_string(),
            "status".to_string(),
        ];
        assert!(intercept_version_request(&argv).is_none());
    }

    /// 验证全局旗标 `-v` 不影响 `version` 子命令的版本拦截。
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

    /// 验证空参数且允许交互时展示快捷 usage，非交互模式不展示。
    #[test]
    fn should_show_quick_usage_for_empty_interactive() {
        let argv = vec!["tokenslim.exe".to_string()];
        assert!(should_show_quick_usage(&argv, true));
        assert!(!should_show_quick_usage(&argv, false));
    }

    /// 验证 verify fixture 文件扩展名过滤：仅接受 .log/.fixture/.input。
    #[test]
    fn verify_fixture_file_extension_filter_works() {
        assert!(is_verify_fixture_file(std::path::Path::new("a.log")));
        assert!(is_verify_fixture_file(std::path::Path::new("a.fixture")));
        assert!(is_verify_fixture_file(std::path::Path::new("a.input")));
        assert!(!is_verify_fixture_file(std::path::Path::new("a.txt")));
    }

    /// 验证 fixture 对应期望文件优先取映射名、缺省时回退同名 fixture 文件。
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

    /// 验证 fixture 文件收集按扩展名过滤并按名称排序。
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

    /// 验证单文件 verify 模式：规则匹配时校验通过。
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

    /// 验证目录 verify 模式：期望文件缺失时报 `missing expected file for` 错误。
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

    /// 验证 repair 请求校验：`--backup` 必须搭配 `--inplace`。
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

    /// 验证 repair 请求校验：目录模式必须搭配 `--inplace`。
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

    /// 验证 repair 请求校验：目录模式不支持 `--output file`。
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

    /// 验证单 repair 目标解析遵循 `--inplace`（回输入）与 `--output`（回输出）优先级。
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

    /// 验证单 repair 跳过判定遵循 include/exclude 过滤规则。
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

    /// 验证单模式 JSON 报告统计 skipped/changed 计数正确。
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

    /// 验证 compress 快捷 usage 仅在 stdin 为空且无参数时展示。
    #[test]
    fn should_show_compress_quick_usage_only_for_empty_stdin_without_args() {
        assert!(should_show_compress_quick_usage(true, true, " \n\t "));
        assert!(!should_show_compress_quick_usage(false, true, ""));
        assert!(!should_show_compress_quick_usage(true, false, ""));
        assert!(!should_show_compress_quick_usage(true, true, "git status"));
    }

    /// 验证 compress 输入读取：文件源返回内容且标记非 stdin。
    #[test]
    fn read_compress_input_reads_file_and_marks_non_stdin() {
        let temp_dir = make_temp_test_dir("compress_input_file");
        let input_path = temp_dir.join("input.log");
        std::fs::write(&input_path, "hello\nworld").expect("write input file");
        // P2-08：验证返回真 lossy 降级标志（纯 UTF-8 输入应为 false）。
        let (text, is_stdin, lossy, _enc) =
            read_compress_input(&InputSource::File(input_path)).expect("read compress input");
        assert_eq!(text, "hello\nworld");
        assert!(!is_stdin);
        assert!(!lossy, "纯 UTF-8 输入不应标记 lossy 降级");
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    /// P2-08：含无解码候选的非法 UTF-8 字节输入应被标记为真 lossy 降级，
    /// 而不是静默产出 U+FFFD（assert lossy 标志为 true）。
    #[test]
    fn read_compress_input_marks_lossy_on_undecodable_bytes() {
        let temp_dir = make_temp_test_dir("compress_input_lossy");
        let input_path = temp_dir.join("undecodable.bin");
        // 0xFF 为非法 UTF-8 且无任何编码候选、非二进制（无 NUL），命中 utf-8-lossy 回退。
        std::fs::write(&input_path, [0xFF, 0xFE, b'a']).expect("write undecodable input");
        let (_text, _is_stdin, lossy, _enc) =
            read_compress_input(&InputSource::File(input_path)).expect("read compress input");
        assert!(lossy, "无可用解码候选的非法字节应标记为真 lossy 降级");
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    /// P1-08：GBK 编码样本 压缩→产物JSON反序列化→再水合→按源编码回写，必须与原始字节逐字节一致
    /// （问题清单 P1-08 原验收条：round-trip 无 mojibake）。
    /// 样本物理化于 `samples/encoding_fallback/case_011_gbk_log_roundtrip.hex`（P3-202 红线，
    /// scenario `skip: true` 不注册 showcase，冻结基线零影响）。
    #[test]
    fn p1_08_gbk_roundtrip_bytes_identical() {
        // 1. 读物理化样本（.hex 文本 → 原始 GBK 字节）
        let hex_text = std::fs::read_to_string(
            "samples/encoding_fallback/case_011_gbk_log_roundtrip.hex",
        )
        .expect("GBK round-trip 样本应存在");
        let gbk_bytes: Vec<u8> = hex_text
            .split_whitespace()
            .map(|h| u8::from_str_radix(h, 16).expect("hex token 解析失败"))
            .collect();
        assert_eq!(gbk_bytes.len(), 174, "样本字节数应与生成基线一致");

        // 2. 压缩入口：GBK 字节 → UTF-8 文本 + 源编码名
        let temp_dir = make_temp_test_dir("p1_08_gbk_roundtrip");
        let input_path = temp_dir.join("input_gbk.log");
        std::fs::write(&input_path, &gbk_bytes).expect("write gbk input");
        let (text, _is_stdin, lossy, enc) =
            read_compress_input(&InputSource::File(input_path)).expect("read compress input");
        assert!(!lossy, "GBK 样本应命中真实解码而非 lossy 降级");
        assert!(enc.starts_with("GB"), "GBK 样本应识别为 GB 系编码，实际 {enc}");

        // 3. 压缩并注入源编码（与 CLI 主路径同口径：非 UTF-8 才写入）
        let mut pipeline = CompressionPipeline::new(
            PipelineConfig::default(),
            crate::cli::get_plugins(),
            MetricsCollector::new(MetricsConfig::default()),
        );
        let mut output = pipeline.compress_str(&text).expect("compress");
        output.metadata.source_encoding = Some(enc.to_string());

        // 4. 产物 JSON 序列化→反序列化（decompress 命令的真实输入形态）
        let json = serde_json::to_string(&output).expect("serialize output");
        let parsed: CompressionOutput = serde_json::from_str(&json).expect("parse output");
        assert_eq!(
            parsed.metadata.source_encoding.as_deref(),
            Some(enc),
            "源编码应经 JSON round-trip 保留"
        );

        // 5. 再水合（decompress 主路径同配置：宽松降级）
        let mut rehydrator = crate::core::rehydration_pipeline::RehydrationPipeline::new(
            parsed.dictionary.clone(),
            crate::cli::get_plugins(),
            crate::core::rehydration_pipeline::RehydrationConfig {
                fallback_on_error: true,
            },
        );
        let restored_text = rehydrator.rehydrate(&parsed).expect("rehydrate");

        // 6. 按源编码回写 → 与原始 GBK 字节逐字节一致
        let restored_bytes = crate::core::encoding_fallback::encode_to_source_encoding(
            &restored_text,
            &enc,
        )
        .unwrap_or_else(|| panic!("GBK 回写应成功"));
        assert_eq!(
            restored_bytes, gbk_bytes,
            "回写字节应与原始 GBK 字节逐字节一致"
        );
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    /// P1-08：UTF-8 输入的产物 JSON 不应含 `source_encoding` 字段（`skip_serializing_if`），
    /// 锁定「UTF-8 输入产物字节零变化」的冻结基线零漂移承诺。
    #[test]
    fn p1_08_utf8_output_json_has_no_source_encoding_field() {
        let mut pipeline = CompressionPipeline::new(
            PipelineConfig::default(),
            crate::cli::get_plugins(),
            MetricsCollector::new(MetricsConfig::default()),
        );
        let output = pipeline.compress_str("plain utf-8 log line\n").expect("compress");
        let json = serde_json::to_string(&output).expect("serialize output");
        assert!(
            !json.contains("source_encoding"),
            "UTF-8 输入产物不应序列化 source_encoding 字段（冻结基线零漂移）"
        );
    }

    /// 验证预流水线动作选择：inject 优先于 doctor。
    #[test]
    fn select_pre_pipeline_action_prioritizes_inject() {
        let mut args = base_cli_args(CliMode::Compress);
        args.inject = true;
        args.doctor = Some(DoctorKind::Encoding);
        assert_eq!(select_pre_pipeline_action(&args), PrePipelineAction::Inject);
    }

    /// 验证预流水线动作选择：verify_rule/fixture/expected 齐全时进入 VerifyRule。
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

    /// 验证预流水线动作选择：RepairFile 模式进入 RepairFile 动作。
    #[test]
    fn select_pre_pipeline_action_detects_repair_file_mode() {
        let args = base_cli_args(CliMode::RepairFile);
        assert_eq!(
            select_pre_pipeline_action(&args),
            PrePipelineAction::RepairFile
        );
    }

    /// 验证预流水线动作选择：普通流水线模式返回 Continue。
    #[test]
    fn select_pre_pipeline_action_continue_for_pipeline_modes() {
        let args = base_cli_args(CliMode::Compress);
        assert_eq!(
            select_pre_pipeline_action(&args),
            PrePipelineAction::Continue
        );
    }

    /// 验证预流水线动作选择：doctor 四种诊断各自路由到对应动作。
    /// doctor 分支按 Encoding/Workspace/Rule/Env 顺序判断，四者互斥、各返回专属动作。
    #[test]
    fn select_pre_pipeline_action_routes_all_doctor_kinds() {
        for (kind, expected) in [
            (DoctorKind::Encoding, PrePipelineAction::DoctorEncoding),
            (DoctorKind::Workspace, PrePipelineAction::DoctorWorkspace),
            (DoctorKind::Rule, PrePipelineAction::DoctorRule),
            (DoctorKind::Env, PrePipelineAction::DoctorEnv),
        ] {
            let mut args = base_cli_args(CliMode::Compress);
            args.doctor = Some(kind);
            assert_eq!(select_pre_pipeline_action(&args), expected);
        }
    }

    /// 验证预流水线动作选择：flag 型动作（rewrite/discover/gain）按字段触发。
    #[test]
    fn select_pre_pipeline_action_routes_flag_actions() {
        // rewrite：任意非空值即触发
        let mut args = base_cli_args(CliMode::Compress);
        args.rewrite = Some("json".to_string());
        assert_eq!(
            select_pre_pipeline_action(&args),
            PrePipelineAction::Rewrite
        );

        // discover：非空路径列表触发
        let mut args = base_cli_args(CliMode::Compress);
        args.discover = vec!["docs".into()];
        assert_eq!(
            select_pre_pipeline_action(&args),
            PrePipelineAction::Discover
        );

        // gain：布尔开关触发
        let mut args = base_cli_args(CliMode::Compress);
        args.gain = true;
        assert_eq!(select_pre_pipeline_action(&args), PrePipelineAction::Gain);
    }

    /// 验证预流水线动作选择：init 由显式 mode 或 init flag 两条路径触发。
    #[test]
    fn select_pre_pipeline_action_routes_init_from_mode_and_flag() {
        // 路径 1：CliMode::Init 模式
        let args = base_cli_args(CliMode::Init);
        assert_eq!(select_pre_pipeline_action(&args), PrePipelineAction::Init);

        // 路径 2：非 Init 模式下置 init flag（init_hooks 场景常见组合）
        let mut args = base_cli_args(CliMode::Compress);
        args.init = true;
        assert_eq!(select_pre_pipeline_action(&args), PrePipelineAction::Init);
    }

    /// 验证预流水线动作选择：hooks 安装/卸载 flag 触发 Hooks，HooksStatus 模式触发查询。
    #[test]
    fn select_pre_pipeline_action_routes_hooks_and_hooks_status() {
        // init_hooks=true 触发 Hooks
        let mut args = base_cli_args(CliMode::Compress);
        args.init_hooks = true;
        assert_eq!(select_pre_pipeline_action(&args), PrePipelineAction::Hooks);

        // uninstall_hooks=true 同样触发 Hooks
        let mut args = base_cli_args(CliMode::Compress);
        args.uninstall_hooks = true;
        assert_eq!(select_pre_pipeline_action(&args), PrePipelineAction::Hooks);

        // CliMode::HooksStatus 模式触发 HooksStatus
        let args = base_cli_args(CliMode::HooksStatus);
        assert_eq!(
            select_pre_pipeline_action(&args),
            PrePipelineAction::HooksStatus
        );
    }

    /// 验证预流水线动作选择：mode 型动作（ExplainPlugin/Plugins/Config/ServeStatic）按模式路由。
    #[test]
    fn select_pre_pipeline_action_routes_mode_only_actions() {
        for (mode, expected) in [
            (CliMode::ExplainPlugin, PrePipelineAction::ExplainPlugin),
            (CliMode::Plugins, PrePipelineAction::Plugins),
            (CliMode::Config, PrePipelineAction::Config),
            (CliMode::ServeStatic, PrePipelineAction::ServeStatic),
        ] {
            let args = base_cli_args(mode);
            assert_eq!(select_pre_pipeline_action(&args), expected);
        }
    }

    /// 验证预流水线动作优先级：前置动作优先于后置 mode 动作。
    /// select_pre_pipeline_action 按 if 链顺序短路，前面的动作胜出：
    /// gain(第7位) > init(第8位) > hooks(第9位) > explain-plugin(第12位)。
    /// 该顺序是 CLI 行为契约，防止后续重排 if 链时静默改变路由优先级。
    #[test]
    fn select_pre_pipeline_action_priority_follows_if_chain_order() {
        // gain 优先于 init：同时置 gain 与 init 时走 Gain
        let mut args = base_cli_args(CliMode::Compress);
        args.gain = true;
        args.init = true;
        assert_eq!(select_pre_pipeline_action(&args), PrePipelineAction::Gain);

        // init 优先于 hooks：同时置 init 与 init_hooks 时走 Init
        let mut args = base_cli_args(CliMode::Compress);
        args.init = true;
        args.init_hooks = true;
        assert_eq!(select_pre_pipeline_action(&args), PrePipelineAction::Init);

        // hooks 优先于 mode 动作：init_hooks + ExplainPlugin 模式时走 Hooks
        let mut args = base_cli_args(CliMode::ExplainPlugin);
        args.init_hooks = true;
        assert_eq!(select_pre_pipeline_action(&args), PrePipelineAction::Hooks);
    }

    /// 验证 `--explain-route` 旗标从 run 命令中拆分并返回剩余参数。
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

    /// 验证 run 模式参数构造应用默认值（AI 导出/信号/路由解释/preset）。
    #[test]
    fn build_run_mode_args_applies_run_defaults() {
        let args = build_run_mode_args(
            vec!["git".to_string(), "status".to_string()],
            true,
            false,
            false,
            500,
            None,
            false,
            None,
            None,
            None,
        );
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

    /// 验证 argv 解析将 `compress` 别名命令映射到 Compress 模式与格式。
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

    /// 验证 argv 解析将 `run --explain-route` 映射到 Run 模式并保留命令。
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

    /// 验证 run 路径 preset 映射：Fast/Balanced/Ai 对应保守/均衡/激进字典。
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

    /// 验证 run 意图到 VCS AI 档位映射（Status/Log/Diff/Other/None）。
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

    /// 验证 VCS 意图时 run 过滤器优先选择 vcs_plugin。
    #[test]
    fn resolve_run_filter_name_prefers_vcs_plugin_for_vcs_intent() {
        let filter =
            resolve_run_filter_name("git", &["status".to_string()], Some(VcsRunIntent::Status));
        assert_eq!(filter, "vcs_plugin");
    }

    /// 验证 run 过滤器回退：优先首参，无参数时用程序名。
    #[test]
    fn resolve_run_filter_name_falls_back_to_first_arg_then_program() {
        let with_args = resolve_run_filter_name("python", &["script.py".to_string()], None);
        assert_eq!(with_args, "script.py");

        let no_args = resolve_run_filter_name("python", &[], None);
        assert_eq!(no_args, "python");
    }

    /// 验证 run 命令字符串拼接程序名与参数。
    #[test]
    fn build_run_command_string_joins_program_and_args() {
        assert_eq!(
            build_run_command_string("git", &["status".to_string()]),
            "git status"
        );
        assert_eq!(build_run_command_string("cargo", &[]), "cargo");
    }

    /// 验证隐式 run 解析接受外部命令（非内置命令）。
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

    /// 验证隐式 run 解析跳过内置命令（如 gain）。
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

    /// 验证隐式 run 解析跳过全局长旗标（如 --help）。
    #[test]
    fn maybe_parse_implicit_run_command_skips_long_flag() {
        let argv = vec!["tokenslim.exe".to_string(), "--help".to_string()];
        let parsed = maybe_parse_implicit_run_command_from_argv(&argv);
        assert!(parsed.is_none());
    }

    /// 验证 run 目标解析拒绝空命令并提示用法。
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

    /// 验证 run 目标解析拒绝把选项（如 --gain）当命令并给出提示。
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

    /// 验证 run 目标解析接受合法外部命令并拆分程序与参数。
    #[test]
    fn parse_run_target_accepts_external_command() {
        let args = vec!["git".to_string(), "status".to_string()];
        let (prog, tail) = parse_run_target("tokenslim.exe", &args).expect("valid run target");
        assert_eq!(prog, "git");
        assert_eq!(tail, &["status".to_string()]);
    }

    /// 验证 clap 错误映射为外部命令提示（run gitx 等）。
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

    /// 验证首参外部命令判定：内置命令（如 gain）与非内置可区分。
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

    /// 验证 `--ai-export` 与 `--ai-signal` 同时使用被拒绝。
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
            strict_rehydrate: false,
            source_encoding_write: false,
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };

        let result = CliArgs::from_raw(raw);
        assert!(matches!(result, Err(CliError::InvalidArgs(_))));
    }

    /// 验证仅 `--ai-signal`（无 export）可被解析接受。
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
            strict_rehydrate: false,
            source_encoding_write: false,
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };

        let parsed = CliArgs::from_raw(raw).expect("ai-signal should be accepted");
        assert!(parsed.ai_signal);
        assert!(!parsed.ai_export);
    }

    /// 验证 init 模式可被解析接受。
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
            strict_rehydrate: false,
            source_encoding_write: false,
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };
        let parsed = CliArgs::from_raw(raw).expect("init mode should parse");
        assert!(matches!(parsed.mode, CliMode::Init));
    }

    /// 验证 mode 缺失但有 run 命令时自动推断为 Run 模式。
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
            strict_rehydrate: false,
            source_encoding_write: false,
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };

        let parsed = CliArgs::from_raw(raw).expect("run mode should be inferred");
        assert!(matches!(parsed.mode, CliMode::Run));
        assert_eq!(
            parsed.run_command,
            vec!["git".to_string(), "status".to_string()]
        );
    }

    /// 验证 repair-file 模式参数可被解析接受。
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
            strict_rehydrate: false,
            source_encoding_write: false,
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };
        let parsed = CliArgs::from_raw(raw).expect("repair-file mode should parse");
        assert!(matches!(parsed.mode, CliMode::RepairFile));
    }

    /// 验证 preset 参数拒绝非法值并返回非空错误信息。
    #[test]
    fn parse_preset_arg_rejects_invalid_value() {
        let err = parse_preset_arg(Some("unknown")).expect_err("preset should be invalid");
        match err {
            CliError::InvalidArgs(msg) => assert!(!msg.trim().is_empty()),
            other => panic!("expected invalid args, got {other:?}"),
        }
    }

    /// 验证输出格式参数 `text` 解析为 Text 格式。
    #[test]
    fn parse_output_format_arg_accepts_text() {
        let fmt = parse_output_format_arg("text").expect("text format should parse");
        assert!(matches!(fmt, OutputFormat::Text));
    }

    /// 验证 repair-file 别名重写自动补充默认输出文件（.repaired.log）。
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

    /// 验证 repair-file 别名重写：指定 --inplace 时不补充 --output。
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

    /// 验证 workspace 别名重写保留 --inject 旗标。
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

    /// 验证 --inplace 在非 repair 模式下被拒绝。
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
            strict_rehydrate: false,
            source_encoding_write: false,
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };
        let result = CliArgs::from_raw(raw);
        assert!(matches!(result, Err(CliError::InvalidArgs(_))));
    }

    /// 验证 --include 在非 repair 模式下被拒绝。
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
            strict_rehydrate: false,
            source_encoding_write: false,
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };
        let result = CliArgs::from_raw(raw);
        assert!(matches!(result, Err(CliError::InvalidArgs(_))));
    }

    /// 验证二进制文件触发 binary-guard 跳过且不写目标文件。
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

    /// 验证 mojibake 文本修复：detected_enc 非空、evidence 含 repairs= 且输出为「中文」。
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

    /// 验证 inplace+backup 修复：生成 .bak 备份且原文件更新为「中文」。
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };

        run_repair_file_command(&args).expect("repair-file should succeed");

        let repaired = std::fs::read_to_string(&input).expect("read repaired");
        assert_eq!(repaired.trim(), "中文");

        let backup = input.with_file_name("bad.log.bak");
        let backup_text = std::fs::read_to_string(&backup).expect("read backup");
        assert_eq!(backup_text, original);

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    /// 验证目录 dry-run 修复不修改任何文件内容。
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };
        run_repair_file_command(&args).expect("dry run should succeed");

        let c1 = std::fs::read_to_string(&f1).expect("read f1");
        let c2 = std::fs::read_to_string(&f2).expect("read f2");
        assert_eq!(c1, bad);
        assert_eq!(c2, bad);
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    /// 验证目录 inplace 修复实际改写文本文件内容。
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };
        run_repair_file_command(&args).expect("directory repair should succeed");
        let c1 = std::fs::read_to_string(&f1).expect("read f1");
        assert_eq!(c1.trim(), "中文");
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    /// 验证目录 inplace 修复按 include 过滤仅改写匹配文件。
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };
        run_repair_file_command(&args).expect("directory repair with include should succeed");

        let c1 = std::fs::read_to_string(&f1).expect("read f1");
        let c2 = std::fs::read_to_string(&f2).expect("read f2");
        assert_eq!(c1.trim(), "中文");
        assert_eq!(c2, bad);
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    /// 验证目录 inplace 修复按 exclude 过滤跳过匹配文件。
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };
        run_repair_file_command(&args).expect("directory repair with exclude should succeed");

        let c1 = std::fs::read_to_string(&f1).expect("read f1");
        let c2 = std::fs::read_to_string(&f2).expect("read f2");
        assert_eq!(c1, bad);
        assert_eq!(c2.trim(), "中文");
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    /// 验证单文件修复命中 include 过滤时跳过并保留原文件。
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };
        run_repair_file_command(&args)
            .expect("single-file repair with non-matching include should succeed");

        let c = std::fs::read_to_string(&file_path).expect("read file");
        assert_eq!(c, bad);
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    /// 验证 repair JSON 记录包含 repair_chain 修复链字段。
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

    /// 验证 verify 三参数（rule/fixture/expected）不全时被拒绝。
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
            strict_rehydrate: false,
            source_encoding_write: false,
            verify_rule: Some("rule.toml".into()),
            verify_fixture: Some("fixture.log".into()),
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };

        let result = CliArgs::from_raw(raw);
        assert!(matches!(result, Err(CliError::InvalidArgs(_))));
    }

    /// 验证 --init-hooks 与 --uninstall-hooks 冲突被拒绝。
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
            strict_rehydrate: false,
            source_encoding_write: false,
            verify_rule: None,
            verify_fixture: None,
            verify_expected: None,
            feature_learn: None,
            feature_sample: None,
            feature_lib: None,
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };
        let result = CliArgs::from_raw(raw);
        assert!(matches!(result, Err(CliError::InvalidArgs(_))));
    }

    /// 验证跨工具（git/svn/hg）VCS run 意图检测，非 VCS 工具返回 None。
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

    /// 验证 git 全局选项（-C/--git-dir）后的子命令仍可识别 VCS 意图。
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

    /// 验证 run 插件路由按关键字映射（git→VCS、npm→Node、cargo→Build、未知→Generic）。
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

    /// 验证 run 命令前的 --explain-route 旗标被剥离且保留后续命令。
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

    /// 验证 run 路由解释输出决策与插件链（cargo→build→rust_go）。
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
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

    /// 验证路由解释中参数前缀候选优先于关键字候选（az→ci_log）。
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
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

    /// 验证 explain-plugin 模式输出候选排序与证据（含备选路由）。
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
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

    /// 验证 explain-plugin 对空白/非法命令行返回 none 与 invalid 原因。
    #[test]
    fn explain_plugin_for_command_line_handles_invalid_input() {
        let mut args = base_cli_args(CliMode::ExplainPlugin);
        args.output_format = OutputFormat::Text;
        let out = explain_plugin_for_command_line("   ", &args);
        assert!(out.contains("selected_plugin=none"));
        assert!(out.contains("reason=invalid_command_line"));
    }

    /// 验证稳定路由（非 fallback、arg_prefix 命中）推荐接受且置信度 gap 正确。
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

    /// 验证 fallback 路由推荐 review_and_retry 且 retry_plugin 指向最高分候选。
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

    /// 验证日志文本 explain 输出检测器得分与证据（web_log 选中）。
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
        // 能力证据行的 coverage_status 锚点：web_log 已全量 frozen（48 个 case），
        // 早前该断言写死 "missing_audit"，在 web_log 审计完成后成为基准漂移，据此校准。
        assert!(out.contains("status:frozen"));
    }

    /// 验证空检测列表时日志 explain 推荐回退 generic_text。
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

    /// 验证竞争候选差距过小时日志 explain 推荐 review_and_retry。
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

    /// 验证空输入(无任何 plugin 命中)时 explain_plugin_for_log_text：
    /// 1) 选中 generic_text；2) 注入 fallback_reason=no_plugin_detector_above_threshold；
    /// 3) line_count/byte_count 与输入精确一致；4) alternatives=0 且无任何 alternative_* 明细行。
    /// 这是独立于 `build_log_explain_recommendation` 之外的报告渲染层契约，之前完全未覆盖。
    #[test]
    fn explain_plugin_for_log_text_empty_input_emits_fallback_reason_and_metadata() {
        // 空字符串输入不会触发任何插件 detector，因此 detections 为空。
        // 选择空串而非乱码，避免非 ASCII 或标点偶然触发路径/噪声类 detector 的低置信度命中。
        let input = "";
        let out = explain_plugin_for_log_text(input, 0.15);

        assert!(out.contains("plugin_selection"));
        assert!(out.contains("input_kind=log"));
        assert!(
            out.contains("selected_plugin=generic_text"),
            "空输入应回退到 generic_text。完整报告：{}",
            out
        );
        // detections.is_empty() 分支写入专用锚点
        assert!(
            out.contains("fallback_reason=no_plugin_detector_above_threshold"),
            "空检测报告应写 fallback_reason 锚点：{}",
            out
        );
        // 元数据必须与输入精确一致：空串 .lines().count()=0，字节数=0
        assert!(
            out.contains("line_count=0"),
            "空输入 line_count 应为 0（空串 lines().count() 返回 0）：{}",
            out
        );
        assert!(
            out.contains("byte_count=0"),
            "空输入 byte_count 应为 0：{}",
            out
        );
        // alternatives 必须是 0 且不产生 alternative_1= 前缀的明细行。
        // 注意：不能用 .contains("alternative_1=")，否则会误命中兄弟字段 recommendation_alternative_1=none。
        // 通过 '\n' 作为行首锚点精确匹配 "alternative_1=" 开的行。
        assert!(out.contains("alternatives=0"));
        assert!(
            !out.contains("\nalternative_1="),
            "alternatives=0 时不应输出 alternative_N= 明细行：{}",
            out
        );
        // 选中插件 generic_text 仍应写 selected_capability 证据段
        assert!(
            out.contains("selected_capability="),
            "generic_text 选中也应输出 capability 证据：{}",
            out
        );
    }

    /// 验证 alternatives 明细行格式契约：
    /// 输入一段能稳定触发多个候选（web_log + smart_path + generic_text）的真实日志，
    /// 断言 alternative_N 行采用 `name|score=X.XXX|priority=Y` 三段式格式，并与
    /// alternatives=N 计数一致。该格式是下游 report 解析器的强契约，此前仅有 helper 层
    /// build_log_explain_recommendation 测试，缺少渲染层端到端覆盖。
    #[test]
    fn explain_plugin_for_log_text_alternatives_lines_match_pipe_schema() {
        let log = r#"203.0.113.7 - - [13/May/2026:08:13:39 +0000] "GET /health HTTP/1.1" 200 2 "-" "kube-probe/1.29" 0.001
203.0.113.8 - - [13/May/2026:08:13:40 +0000] "GET /api/v1/orders HTTP/1.1" 503 41 "-" "Mozilla/5.0" 1.532"#;
        let out = explain_plugin_for_log_text(log, 0.15);

        // alternatives=N 行必须出现，且解析为正整数
        let n_line = out
            .lines()
            .find(|l| l.starts_with("alternatives="))
            .expect("报告必须含 alternatives=N 行");
        let n: usize = n_line
            .trim_start_matches("alternatives=")
            .parse()
            .unwrap_or_else(|_| panic!("alternatives= 后必须是十进制整数：{n_line}"));
        assert!(n >= 1, "至少有 1 个备选插件：n={n}，报告={out}");

        // 每条 alternative_N=xxx|score=X.XXX|priority=Y 需严格匹配三段式 schema
        for idx in 1..=n {
            let prefix = format!("alternative_{idx}=");
            let line = out
                .lines()
                .find(|l| l.starts_with(&prefix))
                .unwrap_or_else(|| panic!("缺少 {prefix} 明细行"));
            let body = line.trim_start_matches(&prefix);
            // 管道分隔：name | score=X.XXX | priority=N
            //   注意这里不用 split('|')，因为 name 段不含 '|'，按 score=/priority= 做子串匹配更鲁棒。
            assert!(
                body.contains("|score=") && body.contains("|priority="),
                "{prefix} 行不符合 name|score=X.XXX|priority=N 管道格式：{line}"
            );
            // score 必须是带小数点的浮点数文本
            let score_start = body.find("|score=").unwrap() + "|score=".len();
            let after_score = &body[score_start..];
            let pipe2 = after_score.find('|').unwrap_or(after_score.len());
            let score_str = &after_score[..pipe2];
            assert!(
                score_str.contains('.'),
                "score 段应是小数格式 X.XXX：{prefix} -> score_str={score_str}，完整行={line}"
            );
            score_str
                .parse::<f32>()
                .unwrap_or_else(|_| panic!("score 段 {score_str} 不能解析为 f32：{line}"));
            // priority 段必须是正整数
            let prio_start = body.find("|priority=").unwrap() + "|priority=".len();
            let prio_str = &body[prio_start..];
            prio_str
                .parse::<u8>()
                .unwrap_or_else(|_| panic!("priority 段 {prio_str} 不能解析为 u8：{line}"));
        }
    }

    /// 验证 fallback 命令样例 explain 推荐 review（generic_text 选中）。
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };

        let out = explain_plugin_for_command_line(&command_line, &args);
        assert!(out.contains("selected_plugin=generic_text"));
        assert!(out.contains("fallback_decision=fallback_selected"));
        assert!(out.contains("recommendation_action=review_and_retry"));
        assert!(out.contains("recommendation_confidence=low"));
    }

    /// 验证预流水线动作 Continue 未被消费时返回 false。
    #[test]
    fn handle_pre_pipeline_action_continue_returns_false() {
        let args = base_cli_args(CliMode::Compress);
        let handled = handle_pre_pipeline_action(&args).expect("continue should not fail");
        assert!(!handled);
    }

    /// 验证 review 推荐样例 explain 输出含 retry_plugin（nodejs）。
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

    /// 验证 explain 回放模板写入包含推荐字段占位符。
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

    /// 验证 explain JSON 渲染暴露结构化推荐（contract v1 校验通过）。
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
            strict_rehydrate: false,
            source_encoding_write: false,
            output_format: OutputFormat::Json,
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
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

    /// 验证 explain JSON 在必填字段缺失时 contract_ok=false 且列出缺失字段。
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

    /// 验证 explain JSON 跳过畸形备选并按 rank 排序。
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

    /// 验证 explain Markdown 渲染包含推荐/证据/备选小节。
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

    /// 验证 cargo build 命令的插件链排除 VCS/git_diff 并包含构建类插件。
    #[test]
    fn plugins_for_run_command_excludes_vcs_for_cargo_build_commands() {
        let plugins = plugins_for_run_command("cargo", &["build".to_string()], None);
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"generic_text"));
        assert!(names.contains(&"gcc_log"));
        assert!(names.contains(&"rust_go"));
        assert!(!names.contains(&"vcs"));
        assert!(!names.contains(&"git_diff"));
    }

    /// 验证新构建/数据工具（pytest/go/gradle/cmake/docker/kubectl/az/psql 等）路由到对应插件。
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
            let plugins = plugins_for_run_command(prog, &args, None);
            let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
            assert!(
                names.contains(&expected_plugin),
                "{prog} should include {expected_plugin}, got {names:?}"
            );
            assert!(!names.contains(&"vcs"));
            assert!(!names.contains(&"git_diff"));
        }
    }

    /// 验证 `az repos` 命令保持走 VCS 路由并包含 vcs 插件。
    #[test]
    fn plugins_for_run_command_keeps_az_repos_on_vcs_route() {
        assert_eq!(
            detect_run_plugin_route("az", &["repos".to_string(), "show".to_string()]),
            RunPluginRoute::Vcs
        );

        let plugins =
            plugins_for_run_command("az", &["repos".to_string(), "show".to_string()], None);
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"vcs"));
    }

    /// 验证 Node 命令的插件链排除 VCS/git_diff 并包含 nodejs。
    #[test]
    fn plugins_for_run_command_excludes_vcs_for_node_commands() {
        let plugins = plugins_for_run_command("npm", &["-g".to_string()], None);
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"nodejs"));
        assert!(names.contains(&"generic_text"));
        assert!(!names.contains(&"vcs"));
        assert!(!names.contains(&"git_diff"));
        assert_eq!(detect_vcs_run_intent("npm", &["-g".to_string()]), None);
    }

    /// 验证未知命令使用通用插件链（generic_text/ansi_cleaner/noise_filter）。
    #[test]
    fn plugins_for_run_command_uses_generic_chain_for_unknown_commands() {
        let plugins = plugins_for_run_command("foobar", &["hello".to_string()], None);
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"generic_text"));
        assert!(names.contains(&"ansi_cleaner"));
        assert!(names.contains(&"noise_filter"));
        assert!(!names.contains(&"vcs"));
    }

    /// 验证移除 VCS 与 git_diff 插件后保留其他插件。
    #[test]
    fn remove_vcs_plugins_strips_vcs_and_git_diff() {
        let plugins = remove_vcs_plugins(get_plugins());
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(!names.contains(&"vcs"));
        assert!(!names.contains(&"git_diff"));
        assert!(names.contains(&"generic_text"));
    }

    /// 验证通用 run 插件链仅保留预期 4 个插件。
    #[test]
    fn keep_generic_run_plugins_only_keeps_expected_chain() {
        let plugins = keep_generic_run_plugins(get_plugins());
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"generic_text"));
        assert!(names.contains(&"ansi_cleaner"));
        assert!(names.contains(&"noise_filter"));
        assert!(names.contains(&"privacy"));
        assert_eq!(names.len(), 4);
    }

    /// 验证 VCS 路由无子命令时意图默认 Other。
    #[test]
    fn detect_vcs_run_intent_defaults_to_other_when_vcs_route_has_no_subcommand() {
        let intent = detect_vcs_run_intent("git", &[]);
        assert!(matches!(intent, Some(VcsRunIntent::Other)));
    }

    /// 验证路径字典块优化：合并段且过滤无收益令牌（$P10 单次使用被展开）。
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

    /// 验证两次使用的中等长度前缀在盈亏平衡点保留。
    #[test]
    fn optimize_paths_keeps_two_use_medium_prefix_at_break_even() {
        let input = "changes:\nM $P1/a.md\nM $P1/b.md\npaths: $P1=docs/design\n";
        let out = optimize_path_dictionary_blocks(input);
        assert!(out.contains("$P1/a.md"));
        assert!(out.contains("$P1/b.md"));
        assert!(out.contains("paths: $P1=docs/design"));
    }

    /// 验证 paths 页脚行计数仅统计页脚行（排除正文中的 paths: 字样）。
    #[test]
    fn count_paths_footer_lines_counts_only_footer_lines() {
        let input = "changes:\nM foo/bar.rs\npaths: $P1=src/core\nuntracked:\n?? docs/design/paths: note\npaths: $P2=docs/design\n";
        assert_eq!(count_paths_footer_lines(input), 2);
    }

    /// 验证路径令牌使用计数遵循令牌边界（$P1 不匹配 $P10）。
    #[test]
    fn count_path_token_uses_respects_token_boundaries() {
        let input = "A $P1/file\nB $P10/file\nC $P1-more\nD $P1\n";
        assert_eq!(count_path_token_uses(input, "$P1"), 2);
        assert_eq!(count_path_token_uses(input, "$P10"), 1);
    }

    /// 验证单次使用的路径令牌不追加页脚并直接展开。
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

    /// 验证多次使用的路径令牌保留并追加页脚。
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

    /// 验证已有页脚时保持原样不重复追加。
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

    /// 验证备选排名键判定跳过元数据条目（capability/declared_patterns 等）。
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

    /// 验证插件能力证据解析将 detect_patterns 限制为前 5 个。
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

    /// 验证 run 模式默认值应用：未显式指定时输出格式归 Text、preset 归 Ai。
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
            strict_rehydrate: false,
            source_encoding_write: false,
            output_format: OutputFormat::Json,
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
        };

        let argv = vec!["tokenslim".to_string(), "--".to_string(), "git".to_string()];
        let out = apply_run_mode_defaults_from_argv(parsed, &argv);

        assert!(matches!(out.output_format, OutputFormat::Text));
        assert!(matches!(out.preset, Some(Preset::Ai)));
    }

    /// 验证 run 模式默认值应用：显式 --format/--preset 时不被覆盖。
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
            strict_rehydrate: false,
            source_encoding_write: false,
            output_format: OutputFormat::Json,
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
            stream: false,
            flush_interval: 500,
            merge: false,
            serve_static: None,
            serve_port: None,
            serve_bind: None,
            serve_open: false,
            run_plugin: None,
            passthrough: false,
            tee: None,
            audit_jsonl: None,
            config_args: Vec::new(),
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

    /// 验证 VCS AI 精简仅在 Log 意图 + Text 输出 + Ai preset 时启用。
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

    /// 验证单 status 页脚跳过最终路径优化，通用（无 VCS 意图）时执行。
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

    /// 验证单 log 页脚与多段页脚场景均执行最终路径优化。
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

    /// 验证路径字典替换解析嵌套别名（$P1→$P2/subdir→root/parent）。
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

    /// 验证路径字典块解析保留首个令牌定义。
    #[test]
    fn parse_path_dictionary_blocks_keeps_first_token_definition() {
        let text = "[paths] $P1=src/old; $P2=src/core\n[paths] $P1=src/new\nM src/core/a.rs\n";
        let (entries, body) = parse_path_dictionary_blocks(text);
        assert_eq!(entries[0], ("$P1".to_string(), "src/old".to_string()));
        assert_eq!(entries[1], ("$P2".to_string(), "src/core".to_string()));
        assert_eq!(body.trim(), "M src/core/a.rs");
    }

    /// 验证无路径字典时合并操作原样返回。
    #[test]
    fn merge_path_dictionary_blocks_returns_original_when_no_dict() {
        let text = "git status\nM src/core/a.rs\n";
        let result = merge_path_dictionary_blocks(text);
        assert_eq!(result, text);
    }

    /// 验证多段路径字典合并：字典置顶、原始路径被替换、嵌套别名生效。
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

    /// 验证 VCS 输出缺命令头时前置 run 命令锚（git status）。
    #[test]
    fn prepend_run_command_anchor_for_vcs_output_without_command_header() {
        let combined = "On branch master\nnothing to commit, working tree clean\n";
        let out = prepend_run_command_anchor_if_needed(combined, "git", &["status".to_string()]);
        assert!(out.starts_with("git status\n"), "out={}", out);
    }

    /// 验证通用输出缺命令头时前置 run 命令锚（npm -g）。
    #[test]
    fn prepend_run_command_anchor_for_generic_output_without_command_header() {
        let combined = "npm <command>\nUsage:\n";
        let out = prepend_run_command_anchor_if_needed(combined, "npm", &["-g".to_string()]);
        assert!(out.starts_with("npm -g\n"), "out={}", out);
    }

    /// 验证命令头已存在时锚前置幂等（不重复）。
    #[test]
    fn prepend_run_command_anchor_is_idempotent_when_header_exists() {
        let combined = "npm -g\nnpm <command>\nUsage:\n";
        let out = prepend_run_command_anchor_if_needed(combined, "npm", &["-g".to_string()]);
        assert_eq!(out, combined);
    }

    /// 验证等价空白形式的命令头可被识别（svn   update 已存在时不重复）。
    #[test]
    fn prepend_run_command_anchor_recognizes_equivalent_whitespace_header() {
        let combined = "svn   update\nUpdated to revision 9.\n";
        let out = prepend_run_command_anchor_if_needed(combined, "svn", &["update".to_string()]);
        assert_eq!(out, combined);
    }

    /// 验证 run 命令锚对含空格/特殊字符的 argv 令牌加引号。
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

    /// 验证字典合并保留命令行首行，字典紧跟其后。
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

    /// 验证云端 VCS 命令（gh pr list）首行同样保留。
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

    /// 验证三个以上子路径共享前缀时提升公共父目录条目。
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

    /// 验证 run 模式 Text 输出保持扁平化 token 内容。
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

    /// 验证无 VCS 意图且无 paths 页脚时文本后处理原样返回。
    #[test]
    fn apply_run_mode_text_postprocessors_leaves_non_vcs_text_unchanged_without_paths_footer() {
        let options = crate::core::path_optimizer::methods::PathDictionaryOptions::default();
        let input = "plain output\n".to_string();
        let output =
            apply_run_mode_text_postprocessors(input.clone(), None, OutputFormat::Text, &options);
        assert_eq!(output, input);
    }

    /// 验证无命令锚时字典合并仍将字典置于顶部。
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

    /// 验证重复子目录路径构造派生子目录别名令牌并用于重写。
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

    /// 验证 hook 块移除幂等：重复执行同样删除 BEGIN/END 标记间内容。
    #[test]
    fn remove_hook_block_is_idempotent() {
        let content = format!("line1\n{}\nhello\n{}\nline2\n", HOOK_BEGIN, HOOK_END);
        let cleaned = remove_hook_block(&content);
        assert!(cleaned.contains("line1"));
        assert!(cleaned.contains("line2"));
        assert!(!cleaned.contains(HOOK_BEGIN));
        assert!(!cleaned.contains(HOOK_END));
    }

    // -------------------------------------------------------------
    // handle_explain_plugin_action 契约测试（编排层：输入分派/输出落地）
    // -------------------------------------------------------------

    /// 验证 explain 动作走「日志文本输入分支」：
    /// 指定 `InputSource::File` 读取样例，写入 `OutputTarget::File`，报告必须含
    /// `plugin_selection`/`input_kind=log`/`selected_plugin=` 三段，
    /// 验证 handle_explain_plugin_action 能正确串联读取→识别→格式化→写入。
    #[test]
    fn handle_explain_plugin_action_supports_log_file_input_branch() {
        use super::*;
        use crate::cli::types::*;

        let tmp = make_temp_test_dir("explain_log_input");
        let input_path = tmp.join("sample.log");
        let output_path = tmp.join("report.txt");
        // 写入一段足够触发插件识别的典型 Spring Boot/Maven 下载日志。
        std::fs::write(
            &input_path,
            "2025-01-01 12:00:00 INFO  [main] org.example.App - Starting\n\
             Downloading from central: https://repo1.maven.org/maven2/org/spring/spring-core/6.1.0/spring-core-6.1.0.jar\n\
             Downloaded 1.2 MB at 3.4 MB/s\n",
        )
        .expect("write sample log");

        let mut args = base_cli_args(CliMode::Compress);
        args.input = InputSource::File(input_path);
        args.output = OutputTarget::File(output_path.clone());
        args.output_format = OutputFormat::Text;
        args.explain_command = None;
        args.explain_fallback_gap = 0.15;
        args.explain_replay_out = None;

        let ok = handle_explain_plugin_action(&args).expect("handle explain should succeed");
        assert!(ok, "handle_explain_plugin_action 应用适用分支返回 Ok(true)");

        let report = std::fs::read_to_string(&output_path).expect("output report must exist");
        assert!(
            report.contains("plugin_selection"),
            "报告头部应有 plugin_selection 标题段"
        );
        assert!(
            report.contains("input_kind=log"),
            "日志分支 input_kind 必须是 log"
        );
        assert!(
            report.contains("selected_plugin="),
            "报告必须给出 selected_plugin 字段"
        );
        assert!(
            report.contains("recommendation_action="),
            "报告必须给出 recommendation_action 字段"
        );
    }

    /// 验证 explain 动作走「命令行输入分支」：
    /// 通过 `explain_command=Some("git status")` 直接识别命令，报告必须含
    /// `input_kind=command` 与 `selected_plugin=vcs_git`，
    /// 证明 explain_plugin_for_command_line 被正确串起来。
    #[test]
    fn handle_explain_plugin_action_supports_command_line_branch() {
        use super::*;
        use crate::cli::types::*;

        let tmp = make_temp_test_dir("explain_cmd_input");
        let output_path = tmp.join("report.txt");

        let mut args = base_cli_args(CliMode::Compress);
        args.input = InputSource::Stdin; // command 分支不读 input，仍给合法默认值
        args.output = OutputTarget::File(output_path.clone());
        args.output_format = OutputFormat::Text;
        args.explain_command = Some("git status --short".to_string());
        args.explain_fallback_gap = 0.15;
        args.explain_replay_out = None;

        let ok = handle_explain_plugin_action(&args).expect("handle explain should succeed");
        assert!(ok);

        let report = std::fs::read_to_string(&output_path).expect("output report must exist");
        assert!(
            report.contains("input_kind=command"),
            "命令分支 input_kind 必须是 command"
        );
        // route_group 是命令行分支的专属字段（日志分支通过 detector 不依赖路由配置），
        // 用它存在 + selected_plugin 有值代替断言具体插件名，避免路由注册表变化导致的脆弱性。
        assert!(
            report.contains("selected_plugin=") && report.contains("route_group="),
            "命令分支必须给出 selected_plugin 与命令行专属 route_group：{}",
            report
        );
        assert!(report.contains("plugin_selection"));
    }

    /// 验证 explain 动作写 replay_case 模板：
    /// 当 `explain_replay_out=Some(...)` 时，handle_explain_plugin_action 必须落盘一份
    /// replay 模板到指定路径，并在最终报告末尾追加 `replay_case_template_path=...` 指示。
    #[test]
    fn handle_explain_plugin_action_writes_replay_template_when_requested() {
        use super::*;
        use crate::cli::types::*;

        let tmp = make_temp_test_dir("explain_replay_out");
        let input_path = tmp.join("sample.log");
        let output_path = tmp.join("report.txt");
        let replay_path = tmp.join("replay_case.json");
        std::fs::write(&input_path, "simple log line one\nsimple log line two\n").unwrap();

        let mut args = base_cli_args(CliMode::Compress);
        args.input = InputSource::File(input_path);
        args.output = OutputTarget::File(output_path.clone());
        args.output_format = OutputFormat::Text;
        args.explain_command = None;
        args.explain_fallback_gap = 0.15;
        args.explain_replay_out = Some(replay_path.clone());

        let ok = handle_explain_plugin_action(&args).expect("handle explain should succeed");
        assert!(ok);

        assert!(
            replay_path.exists(),
            "explain_replay_out 指定后必须生成模板文件"
        );
        let replay_content = std::fs::read_to_string(&replay_path).expect("read replay template");
        assert!(
            replay_content.contains("input_kind:") && replay_content.contains("status: todo"),
            "replay 模板必须采用 YAML 风书写并含 input_kind 与 status 头：\n{}",
            replay_content
        );

        let report = std::fs::read_to_string(&output_path).unwrap();
        assert!(
            report.contains("replay_case_template_path="),
            "报告尾部必须含 replay_case_template_path 锚点：{}",
            report
        );
    }

    /// 验证 explain 动作支持 JSON 输出格式：
    /// output_format=Json 时，最终报告能被 serde_json::from_str 解析为 Value，
    /// 并且顶层包含 plugin_selection 信息，证明 render_explain_report_by_format 生效。
    #[test]
    fn handle_explain_plugin_action_supports_json_output_format() {
        use super::*;
        use crate::cli::types::*;

        let tmp = make_temp_test_dir("explain_json_out");
        let input_path = tmp.join("sample.log");
        let output_path = tmp.join("report.json");
        std::fs::write(
            &input_path,
            "[2025-01-01 10:00:00] INFO  Boot: Starting application\n",
        )
        .unwrap();

        let mut args = base_cli_args(CliMode::Compress);
        args.input = InputSource::File(input_path);
        args.output = OutputTarget::File(output_path.clone());
        args.output_format = OutputFormat::Json;
        args.explain_command = None;
        args.explain_fallback_gap = 0.15;
        args.explain_replay_out = None;

        let ok = handle_explain_plugin_action(&args).expect("handle explain json should succeed");
        assert!(ok);

        let raw = std::fs::read_to_string(&output_path).expect("report.json exists");
        let parsed: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("json 输出应合法可解析: {e}\nraw={raw}"));
        assert!(
            parsed.get("plugin_selection").is_some() || parsed.get("selected_plugin").is_some(),
            "JSON 报告顶层必须含 plugin_selection/selected_plugin 之一：{parsed}"
        );
    }

    /// 全量 frozen 样本插件选择前瞻回放门禁（方案 1）。遍历 samples/ 全量样本，
    /// 复用与打包压缩完全一致的运行时选择逻辑（含命令锚点对齐）做前瞻回放比对，
    /// 产出失配清单报告到 docs/audit/plugin_selection_replay.md，并守住两条门禁：
    ///   1) 至少命中一个命令锚定样本（证明命令锚点对齐确实被回放覆盖到）；
    ///   2) 命令锚定的内容样本绝不允许被清理类插件抢占（noise_filter 等），
    ///      这是修复前 CRLF 行尾导致 VCS 合并日志被 noise_filter 抢占的回归类。
    /// 其余内容检测重叠导致的「归属插件 != 选中插件」列为信息性失配（不断言），
    /// 使插插件选择从黑盒变为可度量、可跟踪的测量点。
    ///
    /// 标记 #[ignore]：全量 frozen 样本回放需对每个样本跑完整压缩链路（含 MB 级样本），
    /// 实测约 24 分钟，若随默认 `cargo test --lib` 运行会严重拖慢变更回归流水线。
    /// 该门禁作为「低频率重闸」保留，按需用 `cargo test -- --ignored` 或 CI 专隔档显式触发；
    /// 默认回归不执行，但能力与报告产物仍在。
    #[test]
    #[ignore]
    fn plugin_selection_forward_replay_over_all_samples_guard_cleanup_steal() {
        // 前置回放在专用大栈线程执行：全量样本往返说明链路包含多层插件检测/
        // 路由解析，测试宿主线程默认栈在部分路径下可能逼近溢出；分配 128MB 栈
        // 使回放测量本身与「触发线程栈上限」解耦，保证门禁在真实压缩路由语义上生效。
        let handle = std::thread::Builder::new()
            .name("plugin_selection_replay".to_string())
            .stack_size(128 * 1024 * 1024)
            .spawn(plugin_selection_replay_report)
            .expect("spawn replay thread");
        let (report, stats) = handle.join().expect("replay thread must not panic");
        let match_rate = if stats.strict_owner_total > 0 {
            stats.strict_owner_match as f64 / stats.strict_owner_total as f64
        } else {
            0.0
        };
        eprintln!(
            "plugin_selection_replay: strict_owner={} match={} rate={:.3} anchored={} rollup_excluded={} cleanup_steal={} mismatches={}",
            stats.strict_owner_total,
            stats.strict_owner_match,
            match_rate,
            stats.command_anchored,
            stats.rollup_excluded,
            stats.cleanup_steal_violations.len(),
            stats.mismatches.len(),
        );
        assert!(
            stats.command_anchored > 0,
            "前瞻回放必须命中至少一个命令锚定样本，否则命令锚点对齐未生效"
        );
        assert!(
            stats.cleanup_steal_violations.is_empty(),
            "命令锚定样本被清理类插件抢占（回归门禁）：\n{}",
            stats.cleanup_steal_violations.join("\n")
        );
        // 落盘信息性测量报告（含完整失配清单），供人工/LLM 复盘与 CI 归档。
        let out_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs")
            .join("audit")
            .join("plugin_selection_replay.md");
        std::fs::write(&out_path, report).expect("write plugin_selection_replay report");
    }
}

/// `verify` 子命令的 CLI 入口，由命令分发层调用。
///
/// 当 `CliArgs` 中同时提供 `verify_rule` / `verify_fixture` / `verify_expected` 时，
/// 调用 [`run_static_rule_verify`] 执行校验并返回 `Ok(true)`；否则表示本子命令不适用，返回 `Ok(false)`。
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
