//! CloudFormation 插件方法实现。

use super::types::CloudFormationPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use crate::plugins::infra_tools_common::{
    compact_spaces, contains_any, decompress_with_dict, fallback_if_anchor_only, is_error_line,
    keep_error_signal, push_anchor,
};
use bumpalo::Bump;
use regex::Regex;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::OnceLock;

impl CloudFormationPlugin {
    pub fn new() -> Self {
        Self {
            name: "cloudformation",
            priority: 93,
        }
    }
}

impl Plugin for CloudFormationPlugin {
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
                "cloudformation",
                "aws cloudformation",
                "create_in_progress",
                "update_rollback",
            ],
        )
        .then_some(0.9)
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
        let compacted = keep_error_signal(&cleaned, compact_cloudformation(&cleaned));
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
fn compact_cloudformation(text: &str) -> String {
    static EVENT_RE: OnceLock<Regex> = OnceLock::new();
    let event_re = EVENT_RE.get_or_init(|| {
        Regex::new(r"(?i)([A-Z0-9]+_[A-Z0-9_]+)\s+([A-Za-z0-9-]+)?\s*(.*)$").unwrap()
    });
    let mut lines = Vec::new();
    push_anchor(&mut lines, text);
    let mut status_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    let mut event_lines = Vec::new();
    let anchor = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed == anchor
            || trimmed.chars().all(|ch| ch == '-' || ch == '|')
        {
            continue;
        }
        if let Some(caps) = event_re.captures(trimmed) {
            let status = caps.get(1).unwrap().as_str().to_ascii_uppercase();
            *status_counts.entry(status.clone()).or_default() += 1;
            event_lines.push(compact_spaces(trimmed));
            if status.contains("FAILED") || status.contains("ROLLBACK") {
                failures.push(compact_spaces(trimmed));
            }
        } else if is_error_line(trimmed) {
            failures.push(trimmed.to_string());
        } else {
            event_lines.push(compact_spaces(trimmed));
        }
    }

    if !status_counts.is_empty() {
        let summary = status_counts
            .iter()
            .map(|(status, count)| format!("{status}={count}"))
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(format!("EVENTS: {summary}"));
    }
    lines.extend(event_lines.into_iter().take(80));
    for failure in failures.into_iter().take(8) {
        if !lines.contains(&failure) {
            lines.push(failure);
        }
    }
    if lines.len() <= 1 {
        lines.extend(
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(compact_spaces)
                .take(80),
        );
    }
    fallback_if_anchor_only(lines, text)
}
