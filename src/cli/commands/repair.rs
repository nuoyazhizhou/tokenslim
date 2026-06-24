//! cli repair 子命令

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


pub(crate) fn default_repair_output_path_from_input_arg(input_arg: &str) -> String {
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


pub(crate) fn default_backup_output_path(input: &std::path::Path) -> std::path::PathBuf {
    let parent = input.parent().unwrap_or(std::path::Path::new("."));
    let file_name = input
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| format!("{s}.bak"))
        .unwrap_or_else(|| "backup.bak".to_string());
    parent.join(file_name)
}


pub(crate) fn normalize_match_path(path: &str) -> String {
    path.replace('\\', "/")
}


pub(crate) fn wildcard_match(pattern: &str, text: &str) -> bool {
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


pub(crate) fn should_repair_path(
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
pub(crate) struct RepairOutcome {
    pub(crate) path: std::path::PathBuf,
    pub(crate) detected_enc: String,
    pub(crate) confidence: String,
    pub(crate) strategy: String,
    pub(crate) repair_chain: String,
    pub(crate) steps: Vec<String>,
    pub(crate) evidence_items: Vec<String>,
    pub(crate) evidence: String,
    pub(crate) changed: bool,
    pub(crate) skipped: bool,
    pub(crate) reason: String,
}


#[derive(Debug, Clone, Serialize)]
pub(crate) struct RepairJsonRecord {
    pub(crate) path: String,
    pub(crate) status: String,
    pub(crate) detected_encoding: String,
    pub(crate) confidence: String,
    pub(crate) strategy: String,
    pub(crate) repair_chain: String,
    pub(crate) changed: bool,
    pub(crate) skipped: bool,
    pub(crate) reason: String,
    pub(crate) steps: Vec<String>,
    pub(crate) evidence: Vec<String>,
}


#[derive(Debug, Clone, Serialize)]
pub(crate) struct RepairJsonSummary {
    pub(crate) changed: usize,
    pub(crate) unchanged: usize,
    pub(crate) skipped: usize,
    pub(crate) failures: usize,
}


#[derive(Debug, Clone, Serialize)]
pub(crate) struct RepairJsonReport {
    pub(crate) kind: &'static str,
    pub(crate) version: &'static str,
    pub(crate) input: String,
    pub(crate) directory_mode: bool,
    pub(crate) dry_run: bool,
    pub(crate) summary: RepairJsonSummary,
    pub(crate) records: Vec<RepairJsonRecord>,
    pub(crate) failures: Vec<String>,
    pub(crate) stdout_payload: Option<String>,
}


pub(crate) fn classify_repair_strategy(
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


pub(crate) fn to_repair_json_record(outcome: &RepairOutcome) -> RepairJsonRecord {
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


pub(crate) fn collect_repair_targets(
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


pub(crate) fn run_single_repair(
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


pub(crate) struct SingleRepairResult {
    detected_enc: String,
    repaired: String,
    steps: Vec<String>,
    confidence: String,
    evidence_items: Vec<String>,
    changed: bool,
}


pub(crate) fn compute_single_repair_result(bytes: &[u8]) -> SingleRepairResult {
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


pub(crate) fn build_single_repair_outcome(
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


pub(crate) fn build_binary_guard_outcome(input_path: &std::path::Path) -> RepairOutcome {
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


pub(crate) fn persist_repaired_text(
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


pub(crate) fn validate_repair_file_request(
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


pub(crate) fn resolve_single_repair_target<'a>(
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


pub(crate) fn run_repair_file_command(args: &CliArgs) -> Result<(), CliError> {
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


pub(crate) fn run_repair_file_directory_mode(
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


pub(crate) fn run_repair_file_single_mode(
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


pub(crate) fn should_skip_single_repair_by_filters(args: &CliArgs, input_path: &std::path::Path) -> bool {
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


pub(crate) fn build_include_exclude_skipped_outcome(input_path: &std::path::Path) -> RepairOutcome {
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


pub(crate) fn build_single_mode_json_report(
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


pub(crate) fn emit_single_mode_json_report(
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

