//! pytest 插件方法实现。

use super::types::PytestPlugin;
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
use regex::Regex;
use std::borrow::Cow;
use std::sync::OnceLock;

impl PytestPlugin {
    pub fn new() -> Self {
        Self {
            name: "pytest",
            priority: 97,
        }
    }
}

impl Plugin for PytestPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let lower = slice.text.to_ascii_lowercase();
        contains_any(
            &lower,
            &[
                "pytest",
                "test session starts",
                "collected ",
                "short test summary info",
                "::test_",
            ],
        )
        .then_some(0.9)
    }

    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let raw = slice.text.as_ref();
        let cleaned = crate::core::utils::strip_ansi(raw);
        let compacted = keep_error_signal(&cleaned, compact_pytest(&cleaned, dict_engine));
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
}

#[tracing::instrument(level = "debug", skip_all)]
fn compact_pytest(text: &str, dict_engine: &mut DictionaryEngine) -> String {
    static RESULT_RE: OnceLock<Regex> = OnceLock::new();
    static XDIST_RESULT_RE: OnceLock<Regex> = OnceLock::new();
    static SUMMARY_RE: OnceLock<Regex> = OnceLock::new();
    let result_re = RESULT_RE.get_or_init(|| {
        Regex::new(
            r"^(?P<test>\S+::\S+)\s+(?P<status>PASSED|FAILED|SKIPPED|ERROR|XFAIL|XPASS|RERUN)\b",
        )
        .unwrap()
    });
    let xdist_result_re = XDIST_RESULT_RE.get_or_init(|| {
        Regex::new(r"^\[gw\d+\]\s+\[\s*\d+%\]\s+(?P<status>PASSED|FAILED|SKIPPED|ERROR|RERUN)\s+(?P<test>\S+::\S+)")
            .unwrap()
    });
    let summary_re = SUMMARY_RE.get_or_init(|| {
        Regex::new(r"(?i)(?P<count>\d+)\s+(?P<kind>failed|passed|skipped|error|errors|xfailed|xpassed|rerun|reruns|warnings?)")
            .unwrap()
    });

    let mut lines = Vec::new();
    push_anchor(&mut lines, text);

    let collected = text
        .lines()
        .find(|line| line.trim_start().starts_with("collected "))
        .map(compact_spaces);

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;
    let mut xfail = 0usize;
    let mut xpass = 0usize;
    let mut reruns = 0usize;
    let mut important = Vec::new();
    let mut ci_notes = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if let Some(caps) = result_re.captures(trimmed) {
            let test = caps.name("test").unwrap().as_str();
            let status = caps.name("status").unwrap().as_str();
            match status {
                "PASSED" => passed += 1,
                "FAILED" => {
                    failed += 1;
                    important.push(format!("FAILED {}", compact_test_id(test, dict_engine)));
                }
                "SKIPPED" => skipped += 1,
                "ERROR" => {
                    errors += 1;
                    important.push(format!("ERROR {}", compact_test_id(test, dict_engine)));
                }
                "XFAIL" => xfail += 1,
                "XPASS" => xpass += 1,
                "RERUN" => {
                    reruns += 1;
                    important.push(format!("RERUN {}", compact_test_id(test, dict_engine)));
                }
                _ => {}
            }
        } else if let Some(caps) = xdist_result_re.captures(trimmed) {
            let test = caps.name("test").unwrap().as_str();
            let status = caps.name("status").unwrap().as_str();
            match status {
                "PASSED" => passed += 1,
                "FAILED" => {
                    failed += 1;
                    important.push(format!("FAILED {}", compact_test_id(test, dict_engine)));
                }
                "SKIPPED" => skipped += 1,
                "ERROR" => {
                    errors += 1;
                    important.push(format!("ERROR {}", compact_test_id(test, dict_engine)));
                }
                "RERUN" => {
                    reruns += 1;
                    important.push(format!("RERUN {}", compact_test_id(test, dict_engine)));
                }
                _ => {}
            }
        } else if trimmed.starts_with("FAILED ") || trimmed.starts_with("ERROR ") {
            important.push(compact_spaces(trimmed));
        } else if lower.contains("created:") && lower.contains("worker") {
            ci_notes.push(format!("XDIST: {}", compact_spaces(trimmed)));
        } else if lower.contains("scheduling tests via") {
            ci_notes.push(format!("XDIST: {}", compact_spaces(trimmed)));
        } else if lower.starts_with("generated xml file:")
            || lower.contains("--junitxml")
            || lower.contains("junitxml=")
        {
            ci_notes.push(format!("JUNITXML: {}", compact_spaces(trimmed)));
        } else if trimmed.starts_with("TOTAL ") && trimmed.contains('%') {
            ci_notes.push(format!("COVERAGE: {}", compact_spaces(trimmed)));
        } else if lower.contains("coverage failure")
            || lower.contains("required test coverage")
            || lower.contains("failed to generate report")
        {
            important.push(compact_spaces(trimmed));
        } else if trimmed.starts_with("E   ")
            || trimmed.starts_with(">")
            || trimmed.contains("AssertionError")
            || trimmed.contains("ModuleNotFoundError")
            || trimmed.contains("FixtureLookupError")
        {
            if important.len() < 12 {
                important.push(compact_spaces(trimmed));
            }
        }
    }

    if passed + failed + skipped + errors + xfail + xpass == 0 {
        if let Some(summary_line) = text
            .lines()
            .rev()
            .find(|line| summary_re.is_match(line) && line.contains(" in "))
        {
            for caps in summary_re.captures_iter(summary_line) {
                let count = caps["count"].parse::<usize>().unwrap_or(0);
                match &caps["kind"].to_ascii_lowercase()[..] {
                    "passed" => passed += count,
                    "failed" => failed += count,
                    "skipped" => skipped += count,
                    "error" | "errors" => errors += count,
                    "xfailed" => xfail += count,
                    "xpassed" => xpass += count,
                    "rerun" | "reruns" => reruns += count,
                    _ => {}
                }
            }
        }
    }

    if let Some(collected) = collected {
        lines.push(collected);
    }
    if passed + failed + skipped + errors + xfail + xpass + reruns > 0 {
        let mut summary =
            format!("PYTEST: passed={passed} failed={failed} skipped={skipped} errors={errors} xfail={xfail} xpass={xpass}");
        if reruns > 0 {
            summary.push_str(&format!(" reruns={reruns}"));
        }
        lines.push(summary);
    }
    lines.extend(ci_notes.into_iter().take(8));
    lines.extend(important.into_iter().take(12));
    fallback_if_anchor_only(lines, text)
}

fn compact_test_id(test: &str, dict_engine: &mut DictionaryEngine) -> String {
    if let Some((path, name)) = test.split_once("::") {
        format!("{}::{name}", dict_engine.add_path_layered(path))
    } else {
        test.to_string()
    }
}
