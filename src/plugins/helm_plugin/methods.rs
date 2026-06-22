//! Helm 插件方法实现。

use super::types::HelmPlugin;
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
use std::borrow::Cow;
use std::collections::BTreeSet;

impl HelmPlugin {
    pub fn new() -> Self {
        Self {
            name: "helm",
            priority: 94,
        }
    }
}

impl Plugin for HelmPlugin {
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
                "helm install",
                "helm upgrade",
                "last deployed:",
                "release \"",
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
        let compacted = keep_error_signal(&cleaned, compact_helm(&cleaned));
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
fn compact_helm(text: &str) -> String {
    let mut lines = Vec::new();
    push_anchor(&mut lines, text);
    let mut fields = Vec::new();
    let mut resources = BTreeSet::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("NAME:")
            || trimmed.starts_with("LAST DEPLOYED:")
            || trimmed.starts_with("NAMESPACE:")
            || trimmed.starts_with("STATUS:")
            || trimmed.starts_with("REVISION:")
            || trimmed.starts_with("TEST SUITE:")
            || trimmed.starts_with("Last Started:")
            || trimmed.starts_with("Last Completed:")
            || trimmed.starts_with("Phase:")
        {
            fields.push(compact_spaces(trimmed));
        } else if trimmed.starts_with("deployment/")
            || trimmed.starts_with("service/")
            || trimmed.starts_with("configmap/")
            || trimmed.starts_with("secret/")
        {
            resources.insert(
                trimmed
                    .split_whitespace()
                    .next()
                    .unwrap_or(trimmed)
                    .to_string(),
            );
        } else if is_error_line(trimmed) {
            fields.push(trimmed.to_string());
        }
    }

    if !fields.is_empty() {
        lines.push(fields.join(" | "));
    }
    if !resources.is_empty() {
        lines.push(format!(
            "RES: {}",
            resources.into_iter().collect::<Vec<_>>().join(",")
        ));
    }
    if lines.len() <= 1 {
        let lower = text.to_ascii_lowercase();
        if lower.contains("rollback was a success") {
            lines.push("ROLLBACK: success".to_string());
        } else if lower.contains("update complete") {
            let repos = text.matches("Successfully got an update").count();
            lines.push(format!("REPO_UPDATE: repos={repos}"));
        } else if lower.contains("saving ") && lower.contains("charts") {
            let downloads = text.matches("Downloading ").count();
            lines.push(format!("DEPS: downloads={downloads}"));
        } else if lower.contains("uninstalled") {
            if let Some(line) = text.lines().find(|line| line.contains("uninstalled")) {
                lines.push(compact_spaces(line));
            }
        } else if let Some(version) = text.lines().find(|line| line.contains("Version:")) {
            lines.push(compact_spaces(version));
        }
    }
    fallback_if_anchor_only(lines, text)
}
