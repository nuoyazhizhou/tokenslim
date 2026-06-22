//! Ansible 插件方法实现。

use super::types::AnsiblePlugin;
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

impl AnsiblePlugin {
    pub fn new() -> Self {
        Self {
            name: "ansible",
            priority: 91,
        }
    }
}

impl Plugin for AnsiblePlugin {
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
            &["play [", "task [", "play recap", "ansible-playbook"],
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
        let compacted = keep_error_signal(&cleaned, compact_ansible(&cleaned));
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
fn compact_ansible(text: &str) -> String {
    static TASK_RE: OnceLock<Regex> = OnceLock::new();
    static HANDLER_RE: OnceLock<Regex> = OnceLock::new();
    static HOST_RE: OnceLock<Regex> = OnceLock::new();
    static RECAP_RE: OnceLock<Regex> = OnceLock::new();
    let task_re = TASK_RE.get_or_init(|| Regex::new(r"^TASK \[(.+?)\]").unwrap());
    let handler_re = HANDLER_RE.get_or_init(|| Regex::new(r"^RUNNING HANDLER \[(.+?)\]").unwrap());
    let host_re = HOST_RE.get_or_init(|| {
        Regex::new(r"^(ok|changed|failed|unreachable|skipping): \[([^\]]+)\](?:\s*=>\s*(.+))?")
            .unwrap()
    });
    let recap_re = RECAP_RE.get_or_init(|| Regex::new(r"^(\S+)\s+:\s+(.+)$").unwrap());

    let mut lines = Vec::new();
    push_anchor(&mut lines, text);
    if let Some(error_summary) = compact_ansible_syntax_error(text) {
        lines.extend(error_summary);
        return fallback_if_anchor_only(lines, text);
    }
    let mut current_task = String::new();
    let mut task_hosts: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    let mut task_order = Vec::new();
    let mut task_details: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut recap = Vec::new();
    let mut in_recap = false;
    let mut in_error_context = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("PLAY RECAP") {
            in_recap = true;
            in_error_context = false;
            continue;
        }
        if let Some(caps) = task_re.captures(trimmed) {
            current_task = caps.get(1).unwrap().as_str().to_string();
            if !task_order.contains(&current_task) {
                task_order.push(current_task.clone());
            }
            in_error_context = false;
            continue;
        }
        if let Some(caps) = handler_re.captures(trimmed) {
            current_task = format!("HANDLER {}", caps.get(1).unwrap().as_str());
            if !task_order.contains(&current_task) {
                task_order.push(current_task.clone());
            }
            in_error_context = false;
            continue;
        }
        if in_recap {
            if let Some(caps) = recap_re.captures(trimmed) {
                recap.push(format!("{} {}", &caps[1], compact_spaces(&caps[2])));
            }
            continue;
        }
        if let Some(caps) = host_re.captures(trimmed) {
            let status = caps.get(1).unwrap().as_str().to_string();
            let mut host = caps.get(2).unwrap().as_str().to_string();
            if let Some(detail) = caps.get(3) {
                if let Some(item) = compact_ansible_loop_item(detail.as_str()) {
                    host = format!("{host}[{item}]");
                }
            }
            if !current_task.is_empty() && !task_order.contains(&current_task) {
                task_order.push(current_task.clone());
            }
            task_hosts
                .entry(current_task.clone())
                .or_default()
                .entry(status.clone())
                .or_default()
                .push(host);
            if let Some(detail) = caps.get(3) {
                if compact_ansible_loop_item(detail.as_str()).is_none() {
                    if let Some(compact_detail) = compact_ansible_detail(&status, detail.as_str()) {
                        task_details
                            .entry(current_task.clone())
                            .or_default()
                            .push(compact_detail);
                    }
                }
            }
        } else if is_error_line(trimmed) {
            lines.push(trimmed.to_string());
            in_error_context = true;
        } else if in_error_context && !trimmed.is_empty() {
            lines.push(compact_spaces(trimmed));
        }
    }

    for task in task_order {
        if task.is_empty() {
            continue;
        }
        let Some(states) = task_hosts.remove(&task) else {
            continue;
        };
        let parts = states
            .into_iter()
            .map(|(status, hosts)| format!("{status}:{}", compact_ansible_hosts(&hosts)))
            .collect::<Vec<_>>()
            .join(" ");
        let mut task_line = format!("TASK [{task}] {parts}");
        if let Some(details) = task_details.remove(&task) {
            task_line.push_str(" detail:");
            task_line.push_str(&details.join("; "));
        }
        lines.push(task_line);
    }
    if !recap.is_empty() {
        lines.push(format!("RECAP: {}", recap.join(" | ")));
    }
    fallback_if_anchor_only(lines, text)
}

#[tracing::instrument(level = "trace", skip_all)]
fn compact_ansible_loop_item(detail: &str) -> Option<String> {
    static ITEM_RE: OnceLock<Regex> = OnceLock::new();
    let item_re = ITEM_RE.get_or_init(|| Regex::new(r"^\(item=([^)]+)\)").unwrap());
    item_re
        .captures(detail.trim())
        .map(|caps| compact_spaces(&caps[1]))
}

#[tracing::instrument(level = "trace", skip_all)]
fn compact_ansible_hosts(hosts: &[String]) -> String {
    static ITEM_HOST_RE: OnceLock<Regex> = OnceLock::new();
    let item_host_re = ITEM_HOST_RE.get_or_init(|| Regex::new(r"^([^\[]+)\[([^\]]+)\]$").unwrap());
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut plain = Vec::new();
    for host in hosts {
        if let Some(caps) = item_host_re.captures(host) {
            grouped
                .entry(caps[1].to_string())
                .or_default()
                .push(caps[2].to_string());
        } else {
            plain.push(host.clone());
        }
    }
    let mut compacted = plain;
    for (host, items) in grouped {
        compacted.push(format!("{host}[{}]", items.join(",")));
    }
    compacted.join(",")
}

#[tracing::instrument(level = "trace", skip_all)]
fn compact_ansible_detail(status: &str, detail: &str) -> Option<String> {
    static MSG_RE: OnceLock<Regex> = OnceLock::new();
    let trimmed = detail.trim();
    if trimmed.is_empty() {
        return None;
    }
    let msg_re = MSG_RE.get_or_init(|| Regex::new(r#""msg"\s*:\s*"([^"]+)""#).unwrap());
    if let Some(caps) = msg_re.captures(trimmed) {
        return Some(format!("msg={}", compact_spaces(&caps[1])));
    }
    let lower = trimmed.to_ascii_lowercase();
    if matches!(status, "failed" | "unreachable")
        || contains_any(&lower, &["error", "exception", "stderr", "traceback"])
    {
        return Some(compact_spaces(trimmed));
    }
    None
}

#[tracing::instrument(level = "trace", skip_all)]
fn compact_ansible_syntax_error(text: &str) -> Option<Vec<String>> {
    static LOC_RE: OnceLock<Regex> = OnceLock::new();
    if !text.contains("ERROR! Syntax Error") {
        return None;
    }
    let mut message = None;
    let mut saw_error = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with("ERROR!") {
            saw_error = true;
            continue;
        }
        if saw_error && !line.is_empty() && !line.starts_with("The ") {
            message = Some(compact_spaces(line));
            break;
        }
    }
    let loc_re =
        LOC_RE.get_or_init(|| Regex::new(r#"'([^']+)': line (\d+), column (\d+)"#).unwrap());
    let mut line = format!(
        "ERROR! YAML syntax: {}",
        message.unwrap_or_else(|| "unknown syntax problem".to_string())
    );
    if let Some(caps) = loc_re.captures(text) {
        line.push_str(&format!(" @{}:{}:{}", &caps[1], &caps[2], &caps[3]));
    }
    Some(vec![line])
}
