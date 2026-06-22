//! Pulumi 插件方法实现。

use super::types::PulumiPlugin;
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

impl PulumiPlugin {
    pub fn new() -> Self {
        Self {
            name: "pulumi",
            priority: 92,
        }
    }
}

impl Plugin for PulumiPlugin {
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
                "previewing update",
                "updating (",
                "pulumi:pulumi:stack",
                "pulumi up",
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
        let compacted = keep_error_signal(&cleaned, compact_pulumi(&cleaned, dict_engine));
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
fn compact_pulumi(text: &str, dict_engine: &mut DictionaryEngine) -> String {
    static RES_RE: OnceLock<Regex> = OnceLock::new();
    let res_re = RES_RE
        .get_or_init(|| Regex::new(r#"^\s*([+\-~])\s+([\w./-]+:[\w:./-]+)\s+([\w./-]+)"#).unwrap());
    let mut lines = Vec::new();
    push_anchor(&mut lines, text);
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut resources = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(caps) = res_re.captures(trimmed) {
            let op = match caps.get(1).unwrap().as_str() {
                "+" => "A",
                "-" => "D",
                "~" => "M",
                _ => "*",
            };
            *counts.entry(op).or_default() += 1;
            let urn = format!("{} {}", &caps[2], &caps[3]);
            resources.push(format!("{op} {}", dict_engine.add_path_layered(&urn)));
        } else if trimmed.starts_with("Resources:") {
            lines.push(compact_spaces(trimmed));
        } else if is_error_line(trimmed) {
            lines.push(trimmed.to_string());
        }
    }

    if !counts.is_empty() {
        lines.push(format!(
            "OPS: +{} ~{} -{}",
            counts.get("A").copied().unwrap_or(0),
            counts.get("M").copied().unwrap_or(0),
            counts.get("D").copied().unwrap_or(0)
        ));
    }
    lines.extend(resources);
    if lines.len() <= 1 {
        let lower = text.to_ascii_lowercase();
        if lower.contains("unchanged") {
            if let Some(line) = text.lines().find(|line| line.trim().contains("unchanged")) {
                lines.push(compact_spaces(line));
            }
        } else if lower.contains("current stack outputs") {
            let count = text
                .lines()
                .filter(|line| line.trim_start().contains(" : "))
                .count();
            lines.push(format!("OUTPUTS: {count}"));
        } else if lower.contains("successfully installed plugin") {
            if let Some(line) = text
                .lines()
                .find(|line| line.contains("Successfully installed"))
            {
                lines.push(compact_spaces(line));
            }
        } else if let Some(version) = text.lines().find(|line| line.starts_with('v')) {
            lines.push(version.to_string());
        }
    }
    fallback_if_anchor_only(lines, text)
}
