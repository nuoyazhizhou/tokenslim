//! CI/CD 外壳日志插件方法实现。

use super::types::CiLogPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use crate::plugins::infra_tools_common::{
    compact_spaces, contains_any, decompress_with_dict, fallback_if_anchor_only, keep_error_signal,
    push_anchor,
};
use bumpalo::Bump;
use std::borrow::Cow;

#[derive(Clone, Debug)]
struct CiStep {
    name: String,
    lines: usize,
    errors: usize,
    warnings: usize,
    failed: bool,
}

#[derive(Default)]
struct CiStats {
    provider: &'static str,
    status: &'static str,
    steps: Vec<CiStep>,
    error_total: usize,
    errors: Vec<String>,
    warnings: usize,
    artifacts: usize,
    caches: usize,
    retries: usize,
}

impl CiLogPlugin {
    pub fn new() -> Self {
        Self {
            name: "ci_log",
            priority: 40,
        }
    }
}

impl Plugin for CiLogPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    fn unwrap(&self, text: &str) -> Option<String> {
        let mut out = String::with_capacity(text.len());
        let mut has_unwrapped = false;

        for line in text.split('\n') {
            let mut current = line;

            // Strip GitHub Actions timestamps: e.g. "2024-04-08T01:01:12.1141231Z "
            if current.len() > 28
                && current.as_bytes()[28] == b' '
                && current[..28].ends_with("Z")
                && current[..4].chars().all(|c| c.is_ascii_digit())
            {
                current = &current[29..];
                has_unwrapped = true;
            }

            // Strip Jenkins [Pipeline] prefixes
            if let Some(rest) = current.strip_prefix("[Pipeline] ") {
                current = rest;
                has_unwrapped = true;
            }

            out.push_str(current);
            out.push('\n');
        }

        if has_unwrapped {
            out.pop(); // Remove the last extra newline added by split
            Some(out)
        } else {
            None
        }
    }

    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let lower = slice.text.to_ascii_lowercase();
        contains_any(
            &lower,
            &[
                "::group::",
                "::endgroup::",
                "::error",
                "running with gitlab-runner",
                "section_start:",
                "section_end:",
                "[pipeline]",
                "##[section]",
                "##[error]",
                "circleci",
                "buildkite-agent",
                "##teamcity[",
                "travis_fold:",
                "travis_time:",
                "travis job",
                "[ci]",
                "[acme-ci]",
                "### step:",
                ">>> [",
                "process completed with exit code",
                "finished: failure",
                "finished: unstable",
                "error: job failed",
                "exited with code",
            ],
        )
        .then_some(0.94)
    }

    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let raw = slice.text.as_ref();
        let cleaned = crate::core::utils::strip_ansi(raw);
        let compacted = keep_error_signal(raw, compact_ci_log(&cleaned));
        let final_text = crate::core::utils::roi::prefer_non_expanding(raw, compacted);
        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(final_text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        decompress_with_dict(compressed, dict)
    }

    fn next_plugins(&self) -> Vec<&'static str> {
        vec![]
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn compact_ci_log(text: &str) -> String {
    let mut stats = CiStats {
        provider: detect_provider(text),
        status: "unknown",
        ..CiStats::default()
    };
    let mut current_step: Option<usize> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(step) = parse_step_name(trimmed) {
            current_step = Some(push_step(&mut stats.steps, step));
            continue;
        }

        if is_step_end(trimmed) {
            current_step = None;
            continue;
        }

        let idx = current_step.unwrap_or_else(|| push_step(&mut stats.steps, "job".to_string()));
        stats.steps[idx].lines += 1;

        let lower = trimmed.to_ascii_lowercase();
        if is_cache_line(&lower) {
            stats.caches += 1;
        }
        if is_artifact_line(&lower) {
            stats.artifacts += 1;
        }
        if lower.contains("retrying")
            || lower.contains("re-running")
            || lower.contains("job retry")
            || lower.contains("retry attempt")
        {
            stats.retries += 1;
        }
        if is_warning_line(&lower) {
            stats.warnings += 1;
            stats.steps[idx].warnings += 1;
        }
        if is_error_line(&lower) {
            stats.error_total += 1;
            stats.steps[idx].errors += 1;
            stats.steps[idx].failed = true;
            if stats.errors.is_empty() {
                stats.errors.push(format!(
                    "s={} msg={}",
                    stats.steps[idx].name,
                    clean_ci_msg(trimmed)
                ));
            }
        }

        if let Some(status) = parse_status(&lower) {
            stats.status = status;
            if status == "failed" {
                stats.steps[idx].failed = true;
            }
        }
    }

    if stats.status == "unknown" {
        stats.status = if stats.error_total == 0 {
            "success"
        } else {
            "failed"
        };
    }

    let mut lines = Vec::new();
    push_anchor(&mut lines, text);
    let step_count = stats.steps.len();
    lines.push(format!(
        "CI|SUMMARY|provider={} status={} steps={} errors={} warn={} art={} cache={} retry={}",
        stats.provider,
        stats.status,
        step_count,
        stats.error_total,
        stats.warnings,
        stats.artifacts,
        stats.caches,
        stats.retries
    ));

    let has_signal_steps = stats
        .steps
        .iter()
        .any(|step| step.failed || step.errors > 0 || step.warnings > 0);
    let selected_steps = stats
        .steps
        .iter()
        .filter(|step| !has_signal_steps || step.failed || step.errors > 0 || step.warnings > 0);
    for step in selected_steps.take(6) {
        let status = if step.failed { "failed" } else { "ok" };
        lines.push(format!(
            "CI|STEP|n={} st={} l={} e={} w={}",
            step.name, status, step.lines, step.errors, step.warnings
        ));
    }
    for error in stats.errors {
        lines.push(format!("!CI|ERROR|{}", error));
    }

    fallback_if_anchor_only(lines, text)
}

#[tracing::instrument(level = "debug", skip_all)]
fn detect_provider(text: &str) -> &'static str {
    let lower = text.to_ascii_lowercase();
    if contains_any(
        &lower,
        &["github actions", "::group::", "::error", "gh run view"],
    ) {
        "github_actions"
    } else if contains_any(&lower, &["running with gitlab-runner", "section_start:"]) {
        "gitlab_ci"
    } else if contains_any(&lower, &["[pipeline]", "finished: failure", "jenkins"]) {
        "jenkins"
    } else if contains_any(&lower, &["##[section]", "##[error]", "azure pipelines"]) {
        "azure_pipelines"
    } else if contains_any(&lower, &["circleci", "circleci received exit code"]) {
        "circleci"
    } else if contains_any(&lower, &["buildkite-agent", "buildkite", "^^^ +++"]) {
        "buildkite"
    } else if contains_any(&lower, &["act -j", "[build/"]) {
        "act"
    } else if contains_any(&lower, &["##teamcity["]) {
        "teamcity"
    } else if contains_any(&lower, &["travis_fold:", "travis_time:", "travis job"]) {
        "travis_ci"
    } else if contains_any(&lower, &["[acme-ci]", "### step:", ">>> [", "[ci]"]) {
        "custom_ci"
    } else {
        "ci"
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn push_step(steps: &mut Vec<CiStep>, name: String) -> usize {
    if let Some((idx, _)) = steps
        .iter()
        .enumerate()
        .rev()
        .find(|(_, step)| step.name == name)
    {
        return idx;
    }
    steps.push(CiStep {
        name,
        lines: 0,
        errors: 0,
        warnings: 0,
        failed: false,
    });
    steps.len() - 1
}

#[tracing::instrument(level = "debug", skip_all)]
fn parse_step_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("::group::") {
        return Some(clean_step_name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("##[group]") {
        return Some(clean_step_name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("##[section]Starting:") {
        return Some(clean_step_name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("##[section]") {
        return Some(clean_step_name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("--- ") {
        return Some(clean_step_name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("+++ ") {
        return Some(clean_step_name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("[Pipeline] { (") {
        return Some(clean_step_name(rest.trim_end_matches(')')));
    }
    if trimmed.starts_with("section_start:") {
        let after_marker = trimmed
            .split_once(']')
            .map(|(_, rest)| rest)
            .unwrap_or(trimmed)
            .trim();
        if !after_marker.is_empty() {
            return Some(clean_step_name(after_marker));
        }
        let parts = trimmed.split(':').collect::<Vec<_>>();
        if parts.len() >= 3 {
            return Some(clean_step_name(
                parts[2].split('[').next().unwrap_or(parts[2]),
            ));
        }
    }
    if trimmed.starts_with("##teamcity[blockOpened") {
        if let Some(name) = extract_service_value(trimmed, "name") {
            return Some(clean_step_name(&name));
        }
    }
    if trimmed.starts_with("##teamcity[compilationStarted") {
        if let Some(name) = extract_service_value(trimmed, "compiler") {
            return Some(clean_step_name(&format!("compile {}", name)));
        }
        return Some("compile".to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("travis_fold:start:") {
        return Some(clean_step_name(rest));
    }
    if let Some((_, rest)) = trimmed.split_once("### Step:") {
        return Some(clean_step_name(rest));
    }
    if let Some((_, rest)) = trimmed.split_once("[ci] step:") {
        return Some(clean_step_name(rest));
    }
    if let Some((_, rest)) = trimmed.split_once("[ACME-CI] STEP ") {
        return Some(clean_step_name(rest));
    }
    if let Some(rest) = trimmed.strip_prefix(">>> [") {
        if let Some((name, _)) = rest.split_once(']') {
            return Some(clean_step_name(name));
        }
    }
    if trimmed.starts_with("Run ") && trimmed.len() < 120 {
        return Some(clean_step_name(trimmed.trim_start_matches("Run ")));
    }
    None
}

fn is_step_end(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "::endgroup::"
        || trimmed.starts_with("section_end:")
        || trimmed.starts_with("##teamcity[blockClosed")
        || trimmed.starts_with("##teamcity[compilationFinished")
        || trimmed.starts_with("travis_fold:end:")
        || trimmed.starts_with("##[endgroup]")
        || trimmed.starts_with("##[section]Finishing:")
        || trimmed == "[Pipeline] }"
}

fn is_cache_line(lower: &str) -> bool {
    contains_any(
        lower,
        &[
            "restore cache",
            "restoring cache",
            "saving cache",
            "save cache",
            "cache hit",
            "cache miss",
            "setting up build cache",
            "store cache",
        ],
    )
}

fn is_artifact_line(lower: &str) -> bool {
    contains_any(
        lower,
        &[
            "uploading artifact",
            "uploading artifacts",
            "download artifact",
            "downloading artifact",
            "store_artifacts",
            "artifacts uploaded",
            "publish artifacts",
            "publishing artifacts",
        ],
    )
}

fn is_warning_line(lower: &str) -> bool {
    contains_any(
        lower,
        &[
            "::warning",
            "##[warning]",
            "warning:",
            "warn ",
            "status='warning'",
        ],
    )
}

fn is_error_line(lower: &str) -> bool {
    if contains_any(
        lower,
        &[
            "failures: 0",
            "failure: 0",
            "errors: 0",
            "failed: 0",
            "0 failed",
            "0 failures",
            "0 errors",
        ],
    ) {
        return false;
    }
    contains_any(
        lower,
        &[
            "::error",
            "##[error]",
            "error:",
            "failed",
            "failure",
            "exception",
            "exited with code",
            "exit code 1",
            "finished: failure",
            "buildproblem",
            "status='error'",
            "status='failure'",
            "errored",
            "the command",
        ],
    )
}

fn parse_status(lower: &str) -> Option<&'static str> {
    if contains_any(
        lower,
        &[
            "process completed with exit code 1",
            "error: job failed",
            "finished: failure",
            "failed",
            "exited with code 1",
            "exit status 1",
            "travis job failed",
            "buildstatus status='failure'",
            "status='failure'",
        ],
    ) {
        Some("failed")
    } else if contains_any(
        lower,
        &[
            "process completed with exit code 0",
            "finished: success",
            "job succeeded",
            "success",
            "travis job succeeded",
            "buildstatus status='success'",
            "status='success'",
        ],
    ) {
        Some("success")
    } else if contains_any(lower, &["finished: unstable", "unstable"]) {
        Some("unstable")
    } else if contains_any(lower, &["canceled", "cancelled"]) {
        Some("canceled")
    } else {
        None
    }
}

fn clean_step_name(name: &str) -> String {
    let cleaned = compact_spaces(name)
        .trim_matches(|ch| matches!(ch, ':' | '"' | '\'' | '[' | ']' | '(' | ')'))
        .to_string();
    if cleaned.is_empty() {
        "step".to_string()
    } else {
        truncate(&cleaned, 64)
    }
}

fn clean_ci_msg(line: &str) -> String {
    let mut msg = line
        .replace("::error::", "")
        .replace("##[error]", "")
        .replace("ERROR:", "")
        .replace("Error:", "");
    if let Some(text) = extract_service_value(&msg, "text") {
        msg = text;
    }
    if let Some((_, rest)) = msg.split_once("::") {
        msg = rest.to_string();
    }
    truncate(&compact_spaces(&msg), 64)
}

#[tracing::instrument(level = "debug", skip_all)]
fn extract_service_value(line: &str, key: &str) -> Option<String> {
    let needle = format!("{}='", key);
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = text
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    out
}
