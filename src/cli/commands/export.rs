//! cli export 子命令

use crate::cli::common::*;
use crate::cli::commands::run::{
    build_run_command_anchor, load_run_routes, plugins_for_run_command,
};
use crate::cli::get_plugins;
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


/// 获取 VCS 意图（兼容旧代码的 Option 签名）
#[derive(Debug, Clone, Default)]
pub(crate) struct PluginCapabilityEvidence {
    pub(crate) description: String,
    pub(crate) tags: String,
    pub(crate) route_group: String,
    pub(crate) sample_cases: u64,
    pub(crate) showcase_cases: u64,
    pub(crate) audit_cases: u64,
    pub(crate) frozen_cases: u64,
    pub(crate) coverage_status: String,
    pub(crate) detect_patterns: Vec<String>,
}


pub(crate) fn capability_index_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("audit")
        .join("plugin_capability_index.json")
}


pub(crate) fn json_u64(value: &serde_json::Value, key: &str) -> u64 {
    value.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}


pub(crate) fn json_string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}


pub(crate) fn parse_detect_patterns(value: &serde_json::Value) -> Vec<String> {
    value
        .get("detect_patterns")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .take(5)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}


pub(crate) fn find_plugin_entry<'a>(
    plugins: &'a [serde_json::Value],
    plugin_name: &str,
) -> Option<&'a serde_json::Value> {
    plugins.iter().find(|plugin| {
        plugin
            .get("name")
            .and_then(|n| n.as_str())
            .is_some_and(|name| name == plugin_name)
    })
}


pub(crate) fn parse_plugin_capability_evidence(plugin: &serde_json::Value) -> PluginCapabilityEvidence {
    PluginCapabilityEvidence {
        description: json_string(plugin, "description"),
        tags: json_string(plugin, "capability_tags"),
        route_group: json_string(plugin, "route_group"),
        sample_cases: json_u64(plugin, "sample_cases"),
        showcase_cases: json_u64(plugin, "showcase_cases"),
        audit_cases: json_u64(plugin, "audit_cases"),
        frozen_cases: json_u64(plugin, "frozen_cases"),
        coverage_status: json_string(plugin, "coverage_status"),
        detect_patterns: parse_detect_patterns(plugin),
    }
}


pub(crate) fn load_plugin_capability_evidence(plugin_name: &str) -> Option<PluginCapabilityEvidence> {
    let bytes = std::fs::read(capability_index_path()).ok()?;
    let content = decode_capability_index_text(&bytes);
    let root: serde_json::Value = serde_json::from_str(&content).ok()?;
    let plugins = root.get("plugins")?.as_array()?;
    let plugin = find_plugin_entry(plugins, plugin_name)?;
    Some(parse_plugin_capability_evidence(plugin))
}


pub(crate) fn decode_capability_index_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16_lossy(&units)
            .trim_start_matches('\u{feff}')
            .to_string();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16_lossy(&units)
            .trim_start_matches('\u{feff}')
            .to_string();
    }
    String::from_utf8_lossy(bytes)
        .trim_start_matches('\u{feff}')
        .to_string()
}


pub(crate) fn sanitize_explain_field(value: &str) -> String {
    value
        .replace('\r', " ")
        .replace('\n', " ")
        .replace('|', "/")
        .trim()
        .to_string()
}


pub(crate) fn parse_explain_report_pairs(report: &str) -> Vec<(String, String)> {
    report
        .lines()
        .filter_map(|line| {
            line.split_once('=')
                .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}


pub(crate) fn parse_explain_pipe_attributes(value: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut attrs = serde_json::Map::new();
    let mut parts = value.split('|');
    if let Some(first) = parts.next() {
        let first = first.trim();
        if !first.is_empty() {
            if let Some((k, v)) = first.split_once('=') {
                attrs.insert(
                    k.trim().to_string(),
                    serde_json::Value::String(v.trim().to_string()),
                );
            } else if let Some((k, v)) = first.split_once(':') {
                attrs.insert(
                    k.trim().to_string(),
                    serde_json::Value::String(v.trim().to_string()),
                );
            } else {
                attrs.insert(
                    "primary".to_string(),
                    serde_json::Value::String(first.to_string()),
                );
            }
        }
    }
    for part in parts {
        if let Some((k, v)) = part.split_once('=') {
            attrs.insert(
                k.trim().to_string(),
                serde_json::Value::String(v.trim().to_string()),
            );
        } else if let Some((k, v)) = part.split_once(':') {
            attrs.insert(
                k.trim().to_string(),
                serde_json::Value::String(v.trim().to_string()),
            );
        }
    }
    attrs
}


pub(crate) fn parse_explain_alternative_index(key: &str) -> Option<usize> {
    let suffix = key.strip_prefix("alternative_")?;
    let index_part = suffix.split('_').next()?;
    index_part.parse::<usize>().ok().filter(|idx| *idx > 0)
}


pub(crate) fn parse_capability_line_to_json(raw: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut attrs = parse_explain_pipe_attributes(raw);
    if let Some(primary) = attrs.remove("primary") {
        attrs.insert("description".to_string(), primary);
    }
    attrs
}


pub(crate) fn explain_required_fields() -> &'static [&'static str] {
    &[
        "input_kind",
        "selected_plugin",
        "fallback_decision",
        "retry_plugin",
        "recommendation_primary",
        "recommendation_confidence",
        "recommendation_action",
        "recommendation_reason",
        "confidence_gap",
        "confidence_gap_source",
        "alternatives",
    ]
}


pub(crate) fn explain_field_str<'a>(
    fields: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
    default: &'a str,
) -> &'a str {
    fields.get(key).and_then(|v| v.as_str()).unwrap_or(default)
}


pub(crate) fn build_selected_section(
    fields: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut selected = serde_json::Map::new();
    selected.insert(
        "plugin".to_string(),
        serde_json::Value::String(explain_field_str(fields, "selected_plugin", "none").to_string()),
    );
    if let Some(why) = fields.get("why").and_then(|v| v.as_str()) {
        selected.insert(
            "why".to_string(),
            serde_json::Value::Object(parse_explain_pipe_attributes(why)),
        );
    }
    if let Some(cap) = fields.get("selected_capability").and_then(|v| v.as_str()) {
        selected.insert(
            "capability".to_string(),
            serde_json::Value::Object(parse_capability_line_to_json(cap)),
        );
    }
    if let Some(patterns) = fields
        .get("selected_declared_patterns")
        .and_then(|v| v.as_str())
    {
        selected.insert(
            "declared_patterns".to_string(),
            serde_json::Value::String(patterns.to_string()),
        );
    }
    selected
}


pub(crate) fn build_alternatives_section(
    pairs: &[(String, String)],
    fields: &serde_json::Map<String, serde_json::Value>,
) -> Vec<serde_json::Value> {
    let mut alternatives = Vec::new();
    for (k, v) in pairs {
        if let Some(entry) = build_alternative_entry(k, v, fields) {
            alternatives.push(entry);
        }
    }
    sort_alternative_entries(&mut alternatives);
    alternatives
}


pub(crate) fn build_alternative_entry(
    key: &str,
    raw: &str,
    fields: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    if !is_alternative_rank_entry_key(key) {
        return None;
    }

    let index = parse_explain_alternative_index(key)?;
    let mut alt = parse_explain_pipe_attributes(raw);
    alt.insert(
        "rank".to_string(),
        serde_json::Value::Number(serde_json::Number::from(index as u64)),
    );
    alt.insert(
        "key".to_string(),
        serde_json::Value::String(key.to_string()),
    );
    alt.insert(
        "raw".to_string(),
        serde_json::Value::String(raw.to_string()),
    );

    if let Some(plugin_name) = alt
        .get("primary")
        .and_then(|x| x.as_str())
        .map(ToString::to_string)
    {
        attach_alternative_capability_and_patterns(&mut alt, fields, index);
        alt.insert("plugin".to_string(), serde_json::Value::String(plugin_name));
    }
    Some(serde_json::Value::Object(alt))
}


pub(crate) fn sort_alternative_entries(entries: &mut [serde_json::Value]) {
    entries.sort_by(|a, b| {
        let a_rank = a.get("rank").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
        let b_rank = b.get("rank").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
        a_rank.cmp(&b_rank)
    });
}


pub(crate) fn is_alternative_rank_entry_key(key: &str) -> bool {
    if !key.starts_with("alternative_") || key == "alternatives" {
        return false;
    }
    !key.ends_with("_capability") && !key.ends_with("_declared_patterns")
}


pub(crate) fn attach_alternative_capability_and_patterns(
    alt: &mut serde_json::Map<String, serde_json::Value>,
    fields: &serde_json::Map<String, serde_json::Value>,
    index: usize,
) {
    let cap_key = format!("alternative_{}_capability", index);
    if let Some(cap) = fields.get(&cap_key).and_then(|x| x.as_str()) {
        alt.insert(
            "capability".to_string(),
            serde_json::Value::Object(parse_capability_line_to_json(cap)),
        );
    }
    let patterns_key = format!("alternative_{}_declared_patterns", index);
    if let Some(patterns) = fields.get(&patterns_key).and_then(|x| x.as_str()) {
        alt.insert(
            "declared_patterns".to_string(),
            serde_json::Value::String(patterns.to_string()),
        );
    }
}


pub(crate) fn build_recommendation_section(
    fields: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "primary": explain_field_str(fields, "recommendation_primary", "none"),
        "confidence": explain_field_str(fields, "recommendation_confidence", "unknown"),
        "action": explain_field_str(fields, "recommendation_action", "none"),
        "alternative_1": explain_field_str(fields, "recommendation_alternative_1", "none"),
        "alternative_2": explain_field_str(fields, "recommendation_alternative_2", "none"),
        "confidence_gap": explain_field_str(fields, "confidence_gap", "not_available"),
        "confidence_gap_source": explain_field_str(fields, "confidence_gap_source", "unknown"),
        "reason": explain_field_str(fields, "recommendation_reason", ""),
    })
}


pub(crate) fn build_explain_contract(
    fields: &serde_json::Map<String, serde_json::Value>,
) -> (bool, Vec<serde_json::Value>) {
    let required_fields = explain_required_fields();
    let missing_required_fields = required_fields
        .iter()
        .filter(|key| !fields.contains_key(**key))
        .map(|key| serde_json::Value::String((*key).to_string()))
        .collect::<Vec<_>>();
    (missing_required_fields.is_empty(), missing_required_fields)
}


pub(crate) fn collect_explain_fields(
    report: &str,
) -> (
    Vec<(String, String)>,
    serde_json::Map<String, serde_json::Value>,
) {
    let pairs = parse_explain_report_pairs(report);
    let mut fields = serde_json::Map::new();
    for (k, v) in &pairs {
        fields.insert(k.to_string(), serde_json::Value::String(v.to_string()));
    }
    (pairs, fields)
}


pub(crate) fn build_explain_report_json_value(
    pairs: &[(String, String)],
    fields: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    let selected = build_selected_section(fields);
    let alternatives = build_alternatives_section(pairs, fields);
    let recommendation = build_recommendation_section(fields);
    let (contract_ok, missing_required_fields) = build_explain_contract(fields);
    let required_fields = explain_required_fields();

    serde_json::json!({
        "contract_version": "explain.v1",
        "contract_ok": contract_ok,
        "required_fields": required_fields,
        "missing_required_fields": missing_required_fields,
        "kind": explain_field_str(fields, "plugin_selection", "plugin_selection"),
        "input_kind": explain_field_str(fields, "input_kind", "unknown"),
        "selected_plugin": explain_field_str(fields, "selected_plugin", "none"),
        "fallback_decision": explain_field_str(fields, "fallback_decision", "none"),
        "retry_plugin": explain_field_str(fields, "retry_plugin", "none"),
        "selected": selected,
        "recommendation": recommendation,
        "alternatives": alternatives,
        "fields": fields,
    })
}


pub(crate) fn render_explain_report_json(report: &str) -> Result<String, CliError> {
    let (pairs, fields) = collect_explain_fields(report);
    let value = build_explain_report_json_value(&pairs, &fields);

    serde_json::to_string_pretty(&value).map_err(CliError::Serialization)
}


pub(crate) fn render_explain_report_markdown(report: &str) -> String {
    let pairs = parse_explain_report_pairs(report);
    let mut map = std::collections::BTreeMap::new();
    for (k, v) in pairs {
        map.insert(k, v);
    }

    let mut out = String::new();
    out.push_str("# Plugin Selection\n\n");
    out.push_str(&format!(
        "- input_kind: `{}`\n",
        map.get("input_kind")
            .map(String::as_str)
            .unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "- selected_plugin: `{}`\n",
        map.get("selected_plugin")
            .map(String::as_str)
            .unwrap_or("none")
    ));
    out.push_str(&format!(
        "- fallback_decision: `{}`\n",
        map.get("fallback_decision")
            .map(String::as_str)
            .unwrap_or("none")
    ));
    out.push_str(&format!(
        "- retry_plugin: `{}`\n\n",
        map.get("retry_plugin")
            .map(String::as_str)
            .unwrap_or("none")
    ));

    out.push_str("## Recommendation\n\n");
    out.push_str(&format!(
        "- primary: `{}`\n",
        map.get("recommendation_primary")
            .map(String::as_str)
            .unwrap_or("none")
    ));
    out.push_str(&format!(
        "- confidence: `{}`\n",
        map.get("recommendation_confidence")
            .map(String::as_str)
            .unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "- action: `{}`\n",
        map.get("recommendation_action")
            .map(String::as_str)
            .unwrap_or("none")
    ));
    out.push_str(&format!(
        "- alternative_1: `{}`\n",
        map.get("recommendation_alternative_1")
            .map(String::as_str)
            .unwrap_or("none")
    ));
    out.push_str(&format!(
        "- alternative_2: `{}`\n",
        map.get("recommendation_alternative_2")
            .map(String::as_str)
            .unwrap_or("none")
    ));
    out.push_str(&format!(
        "- confidence_gap: `{}`\n",
        map.get("confidence_gap")
            .map(String::as_str)
            .unwrap_or("not_available")
    ));
    out.push_str(&format!(
        "- confidence_gap_source: `{}`\n",
        map.get("confidence_gap_source")
            .map(String::as_str)
            .unwrap_or("unknown")
    ));
    out.push_str(&format!(
        "- reason: `{}`\n",
        map.get("recommendation_reason")
            .map(String::as_str)
            .unwrap_or("")
    ));

    if let Some(cap) = map.get("selected_capability") {
        out.push_str("\n## Evidence\n\n");
        out.push_str(&format!("- selected_capability: `{}`\n", cap));
        if let Some(patterns) = map.get("selected_declared_patterns") {
            out.push_str(&format!("- selected_declared_patterns: `{}`\n", patterns));
        }
    }

    out.push_str("\n## Alternatives\n\n");
    let mut i = 1usize;
    loop {
        let key = format!("alternative_{}", i);
        if let Some(alt) = map.get(&key) {
            out.push_str(&format!("- {}: `{}`\n", key, alt));
            let cap_key = format!("{}_capability", key);
            if let Some(cap) = map.get(&cap_key) {
                out.push_str(&format!("  - capability: `{}`\n", cap));
            }
            let patterns_key = format!("{}_declared_patterns", key);
            if let Some(patterns) = map.get(&patterns_key) {
                out.push_str(&format!("  - declared_patterns: `{}`\n", patterns));
            }
            i += 1;
            continue;
        }
        break;
    }

    out
}


pub(crate) fn render_explain_report_by_format(
    report: &str,
    format: &OutputFormat,
) -> Result<String, CliError> {
    match format {
        OutputFormat::Text => Ok(report.to_string()),
        OutputFormat::Markdown => Ok(render_explain_report_markdown(report)),
        OutputFormat::Json => render_explain_report_json(report),
    }
}


pub(crate) fn render_capability_evidence_line(prefix: &str, plugin_name: &str, out: &mut String) {
    if let Some(evidence) = load_plugin_capability_evidence(plugin_name) {
        out.push_str(&format!(
            "{}_capability=description:{}|tags:{}|route:{}|samples:{}|showcase:{}|audit:{}|frozen:{}|status:{}\n",
            prefix,
            sanitize_explain_field(&evidence.description),
            sanitize_explain_field(&evidence.tags),
            if evidence.route_group.is_empty() {
                "none"
            } else {
                evidence.route_group.as_str()
            },
            evidence.sample_cases,
            evidence.showcase_cases,
            evidence.audit_cases,
            evidence.frozen_cases,
            if evidence.coverage_status.is_empty() {
                "unknown"
            } else {
                evidence.coverage_status.as_str()
            }
        ));
        if !evidence.detect_patterns.is_empty() {
            out.push_str(&format!(
                "{}_declared_patterns={}\n",
                prefix,
                sanitize_explain_field(&evidence.detect_patterns.join(" ; "))
            ));
        }
    } else {
        out.push_str(&format!("{}_capability=missing_index_entry\n", prefix));
    }
}


pub(crate) fn parse_plugin_explain_command_line(line: &str) -> Option<Vec<String>> {
    #[derive(Clone, Copy)]
    enum QuoteMode {
        None,
        Single,
        Double,
    }

    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut mode = QuoteMode::None;
    let mut escaped = false;

    for ch in line.chars() {
        match mode {
            QuoteMode::None => {
                if ch.is_whitespace() {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                } else if ch == '\'' {
                    mode = QuoteMode::Single;
                } else if ch == '"' {
                    mode = QuoteMode::Double;
                } else {
                    current.push(ch);
                }
            }
            QuoteMode::Single => {
                if ch == '\'' {
                    mode = QuoteMode::None;
                } else {
                    current.push(ch);
                }
            }
            QuoteMode::Double => {
                if escaped {
                    current.push(ch);
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    mode = QuoteMode::None;
                } else {
                    current.push(ch);
                }
            }
        }
    }

    if !matches!(mode, QuoteMode::None) {
        return None;
    }
    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    (!tokens.is_empty()).then_some(tokens)
}


pub(crate) fn explain_recommendation_confidence_for_command(
    route_fallback: bool,
    matched_by: &str,
) -> &'static str {
    if route_fallback {
        "low"
    } else if matches!(matched_by, "command_exact" | "arg_prefix" | "arg_exact") {
        "high"
    } else {
        "medium"
    }
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandRouteRecommendation {
    pub(crate) retry_plugin: String,
    pub(crate) fallback_decision: &'static str,
    pub(crate) recommendation_confidence: &'static str,
    pub(crate) recommendation_action: &'static str,
    pub(crate) recommendation_alternative_1: String,
    pub(crate) recommendation_alternative_2: String,
    pub(crate) confidence_gap: String,
    pub(crate) recommendation_reason: String,
}


pub(crate) fn build_command_route_recommendation(
    route: &plugin_config_loader::RunRouteDecision,
    alternatives: &[&plugin_config_loader::RunRouteDecision],
) -> CommandRouteRecommendation {
    let retry_plugin = if route.is_fallback {
        alternatives
            .first()
            .map(|candidate| candidate.plugin_name.as_str())
            .unwrap_or("none")
            .to_string()
    } else {
        "none".to_string()
    };
    let fallback_decision = if route.is_fallback {
        "fallback_selected"
    } else {
        "stable_route"
    };
    let recommendation_confidence =
        explain_recommendation_confidence_for_command(route.is_fallback, &route.matched_by);
    let recommendation_action = if route.is_fallback {
        "review_and_retry"
    } else {
        "accept"
    };
    let recommendation_alternative_1 = alternatives
        .first()
        .map(|candidate| candidate.plugin_name.as_str())
        .unwrap_or("none")
        .to_string();
    let recommendation_alternative_2 = alternatives
        .get(1)
        .map(|candidate| candidate.plugin_name.as_str())
        .unwrap_or("none")
        .to_string();
    let route_priority_gap = match (
        route.priority,
        alternatives.first().and_then(|c| c.priority),
    ) {
        (Some(selected_priority), Some(top_alt_priority)) => {
            i64::from(selected_priority) - i64::from(top_alt_priority)
        }
        _ => 0,
    };
    let confidence_gap = if alternatives.is_empty() {
        "not_applicable".to_string()
    } else {
        route_priority_gap.to_string()
    };
    let recommendation_reason = if route.is_fallback {
        format!(
            "fallback_route_selected|retry_plugin:{}|top_alternative:{}",
            retry_plugin, recommendation_alternative_1
        )
    } else {
        format!(
            "route_match:{}|pattern:{}|intent:{}|priority:{}",
            route.matched_by,
            route.matched_pattern.as_deref().unwrap_or("none"),
            route.intent.as_deref().unwrap_or("none"),
            route
                .priority
                .map(|p| p.to_string())
                .unwrap_or_else(|| "none".to_string())
        )
    };

    CommandRouteRecommendation {
        retry_plugin,
        fallback_decision,
        recommendation_confidence,
        recommendation_action,
        recommendation_alternative_1,
        recommendation_alternative_2,
        confidence_gap,
        recommendation_reason,
    }
}


pub(crate) fn append_command_alternatives_report(
    out: &mut String,
    alternatives: &[&plugin_config_loader::RunRouteDecision],
) {
    for (idx, candidate) in alternatives.iter().enumerate() {
        out.push_str(&format!(
            "alternative_{}={}|group={}|matched_by={}|pattern={}|intent={}|priority={}|fallback={}\n",
            idx + 1,
            candidate.plugin_name,
            candidate.route_group,
            candidate.matched_by,
            candidate.matched_pattern.as_deref().unwrap_or("none"),
            candidate.intent.as_deref().unwrap_or("none"),
            candidate
                .priority
                .map(|p| p.to_string())
                .unwrap_or_else(|| "none".to_string()),
            candidate.is_fallback
        ));
        render_capability_evidence_line(
            &format!("alternative_{}", idx + 1),
            &candidate.plugin_name,
            out,
        );
    }
}


pub(crate) struct CommandExplainContext {
    prog: String,
    cmd_args: Vec<String>,
    route: plugin_config_loader::RunRouteDecision,
    alternatives: Vec<plugin_config_loader::RunRouteDecision>,
    chain: String,
}


pub(crate) fn invalid_command_explain_report() -> String {
    "plugin_selection\ninput_kind=command\nselected_plugin=none\nreason=invalid_command_line\nalternatives=0\n".to_string()
}


pub(crate) fn build_command_explain_context(command_line: &str) -> Option<CommandExplainContext> {
    let tokens = parse_plugin_explain_command_line(command_line)?;
    let prog = tokens[0].clone();
    let cmd_args = tokens.iter().skip(1).cloned().collect::<Vec<_>>();
    let caps = load_run_routes();
    let route = plugin_config_loader::resolve_run_route(&caps, &prog, &cmd_args);
    let route_candidates =
        plugin_config_loader::explain_run_route_candidates(&caps, &prog, &cmd_args);
    let chain = plugins_for_run_command(&prog, &cmd_args, None)
        .iter()
        .map(|plugin| plugin.name())
        .collect::<Vec<_>>()
        .join(", ");

    let alternatives = route_candidates
        .iter()
        .filter(|candidate| candidate.plugin_name != route.plugin_name)
        .cloned()
        .collect::<Vec<_>>();

    Some(CommandExplainContext {
        prog,
        cmd_args,
        route,
        alternatives,
        chain,
    })
}


pub(crate) fn render_command_plugin_selection_report(
    command_line: &str,
    args: &CliArgs,
    context: &CommandExplainContext,
) -> String {
    let alt_refs = context.alternatives.iter().collect::<Vec<_>>();
    let recommendation = build_command_route_recommendation(&context.route, &alt_refs);

    let mut out = String::new();
    out.push_str("plugin_selection\n");
    out.push_str("input_kind=command\n");
    out.push_str(&format!(
        "command={}\n",
        sanitize_explain_field(&build_run_command_anchor(&context.prog, &context.cmd_args))
    ));
    out.push_str(&format!("selected_plugin={}\n", context.route.plugin_name));
    out.push_str(&format!("route_group={}\n", context.route.route_group));
    out.push_str(&format!(
        "why=command_tool:{} matched_by:{} pattern:{} intent:{} priority:{} fallback:{}\n",
        context.route.command_keyword,
        context.route.matched_by,
        context.route.matched_pattern.as_deref().unwrap_or("none"),
        context.route.intent.as_deref().unwrap_or("none"),
        context
            .route
            .priority
            .map(|p| p.to_string())
            .unwrap_or_else(|| "none".to_string()),
        context.route.is_fallback
    ));
    render_capability_evidence_line("selected", &context.route.plugin_name, &mut out);
    out.push_str(&format!(
        "fallback_decision={}\n",
        recommendation.fallback_decision
    ));
    out.push_str(&format!(
        "top_score_gap={}\n",
        recommendation.confidence_gap
    ));
    out.push_str(&format!(
        "confidence_gap={}\n",
        recommendation.confidence_gap
    ));
    out.push_str("confidence_gap_source=route_priority\n");
    out.push_str(&format!(
        "fallback_threshold={:.3}\n",
        args.explain_fallback_gap
    ));
    out.push_str(&format!("retry_plugin={}\n", recommendation.retry_plugin));
    out.push_str(&format!(
        "recommendation_primary={}\n",
        context.route.plugin_name
    ));
    out.push_str(&format!(
        "recommendation_confidence={}\n",
        recommendation.recommendation_confidence
    ));
    out.push_str(&format!(
        "recommendation_action={}\n",
        recommendation.recommendation_action
    ));
    out.push_str(&format!(
        "recommendation_alternative_1={}\n",
        recommendation.recommendation_alternative_1
    ));
    out.push_str(&format!(
        "recommendation_alternative_2={}\n",
        recommendation.recommendation_alternative_2
    ));
    out.push_str(&format!(
        "recommendation_reason={}\n",
        sanitize_explain_field(&recommendation.recommendation_reason)
    ));
    out.push_str(&format!("alternatives={}\n", context.alternatives.len()));
    append_command_alternatives_report(&mut out, &alt_refs);
    out.push_str(&format!(
        "candidate_plugin_chain={}\n",
        sanitize_explain_field(&context.chain)
    ));
    out.push_str(&format!(
        "run_route_view=available_with:tokenslim run --explain-route {}\n",
        sanitize_explain_field(command_line)
    ));
    out.push_str(&format!(
        "output_format={}\n",
        match args.output_format {
            OutputFormat::Json => "json",
            OutputFormat::Markdown => "markdown",
            OutputFormat::Text => "text",
        }
    ));
    out.push_str("replay_case_template=available_with:--explain-replay-out <path>\n");
    out
}


pub(crate) fn explain_plugin_for_command_line(command_line: &str, args: &CliArgs) -> String {
    let Some(context) = build_command_explain_context(command_line) else {
        return invalid_command_explain_report();
    };
    render_command_plugin_selection_report(command_line, args, &context)
}


pub(crate) fn is_retryable_explain_plugin(name: &str) -> bool {
    !matches!(
        name,
        "ansi_cleaner"
            | "generic_text"
            | "noise_filter"
            | "smart_code"
            | "smart_path"
            | "static_rule"
            | "template_driven"
    )
}


#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LogExplainRecommendation {
    pub(crate) selected: (String, u8, f32),
    pub(crate) alternatives: Vec<(String, u8, f32)>,
    pub(crate) top_score_gap: f32,
    pub(crate) retry_score_gap: f32,
    pub(crate) fallback_decision: &'static str,
    pub(crate) retry_plugin: String,
    pub(crate) recommendation_confidence: &'static str,
    pub(crate) recommendation_action: &'static str,
    pub(crate) recommendation_alternative_1: String,
    pub(crate) recommendation_alternative_2: String,
    pub(crate) recommendation_reason: String,
    pub(crate) fallback_note: Option<String>,
}


pub(crate) fn collect_log_detections(slice: &Slice<'_>) -> Vec<(String, u8, f32)> {
    let mut detections = get_plugins()
        .into_iter()
        .filter_map(|plugin| {
            plugin
                .detect(slice)
                .filter(|score| *score > 0.1)
                .map(|score| (plugin.name().to_string(), plugin.priority(), score))
        })
        .collect::<Vec<_>>();
    detections.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.0.cmp(&b.0))
    });
    detections
}


pub(crate) fn build_log_explain_recommendation(
    detections: &[(String, u8, f32)],
    fallback_gap_threshold: f32,
) -> LogExplainRecommendation {
    let selected = detections
        .first()
        .cloned()
        .unwrap_or_else(|| ("generic_text".to_string(), 255, 0.0));
    let alternatives = detections
        .iter()
        .skip(1)
        .take(7)
        .cloned()
        .collect::<Vec<_>>();
    let top_score_gap = alternatives
        .first()
        .map(|(_, _, score)| selected.2 - *score)
        .unwrap_or(selected.2);
    let retry_candidate = alternatives
        .iter()
        .find(|(name, _, _)| is_retryable_explain_plugin(name));
    let retry_score_gap = retry_candidate
        .map(|(_, _, score)| selected.2 - *score)
        .unwrap_or(selected.2);
    let fallback_decision = if detections.is_empty() {
        "fallback_selected"
    } else if retry_candidate.is_some() && retry_score_gap < fallback_gap_threshold {
        "review_recommended"
    } else {
        "stable_detector"
    };
    let retry_plugin = if fallback_decision == "review_recommended" {
        retry_candidate
            .map(|(name, _, _)| name.as_str())
            .unwrap_or("none")
            .to_string()
    } else {
        "none".to_string()
    };
    let recommendation_confidence = if detections.is_empty() {
        "low"
    } else if fallback_decision == "review_recommended" {
        "medium"
    } else if top_score_gap >= fallback_gap_threshold {
        "high"
    } else {
        "medium"
    };
    let recommendation_action = if detections.is_empty() {
        "review_generic_fallback"
    } else if fallback_decision == "review_recommended" {
        "review_and_retry"
    } else {
        "accept"
    };
    let recommendation_alternative_1 = alternatives
        .first()
        .map(|(name, _, _)| name.as_str())
        .unwrap_or("none")
        .to_string();
    let recommendation_alternative_2 = alternatives
        .get(1)
        .map(|(name, _, _)| name.as_str())
        .unwrap_or("none")
        .to_string();
    let recommendation_reason = if detections.is_empty() {
        "no_detector_above_threshold".to_string()
    } else if fallback_decision == "review_recommended" {
        format!(
            "close_competitor|retry_plugin:{}|retry_score_gap:{:.3}|threshold:{:.3}",
            retry_plugin, retry_score_gap, fallback_gap_threshold
        )
    } else {
        format!(
            "detector_stable|selected_score:{:.3}|top_score_gap:{:.3}|threshold:{:.3}",
            selected.2, top_score_gap, fallback_gap_threshold
        )
    };
    let fallback_note = if fallback_decision == "stable_detector" {
        alternatives.first().and_then(|(name, _, _)| {
            if !is_retryable_explain_plugin(name) && top_score_gap < fallback_gap_threshold {
                Some(format!("nearest_candidate_non_retryable:{}", name))
            } else {
                None
            }
        })
    } else {
        None
    };

    LogExplainRecommendation {
        selected,
        alternatives,
        top_score_gap,
        retry_score_gap,
        fallback_decision,
        retry_plugin,
        recommendation_confidence,
        recommendation_action,
        recommendation_alternative_1,
        recommendation_alternative_2,
        recommendation_reason,
        fallback_note,
    }
}


pub(crate) fn explain_plugin_for_log_text(text: &str, fallback_gap_threshold: f32) -> String {
    let slice = Slice {
        id: 1,
        text: Cow::Borrowed(text),
        slice_type: SliceType::LogBlock,
        offset: 0,
        line_start: 1,
        line_end: text.lines().count().max(1),
        file_metadata: None,
        flags: SliceFlags::default(),
    };

    let detections = collect_log_detections(&slice);
    let recommendation = build_log_explain_recommendation(&detections, fallback_gap_threshold);

    let mut out = String::new();
    out.push_str("plugin_selection\n");
    out.push_str("input_kind=log\n");
    out.push_str(&format!("line_count={}\n", text.lines().count()));
    out.push_str(&format!("byte_count={}\n", text.len()));
    out.push_str(&format!("selected_plugin={}\n", recommendation.selected.0));
    out.push_str(&format!(
        "why=content_detector_score:{:.3}|plugin_priority:{}|candidate_rank:1\n",
        recommendation.selected.2, recommendation.selected.1
    ));
    render_capability_evidence_line("selected", &recommendation.selected.0, &mut out);
    out.push_str(&format!(
        "fallback_decision={}\n",
        recommendation.fallback_decision
    ));
    out.push_str(&format!(
        "top_score_gap={:.3}\n",
        recommendation.top_score_gap
    ));
    out.push_str(&format!(
        "confidence_gap={:.3}\n",
        recommendation.top_score_gap
    ));
    out.push_str("confidence_gap_source=detector_score\n");
    out.push_str(&format!(
        "retry_score_gap={:.3}\n",
        recommendation.retry_score_gap
    ));
    out.push_str(&format!(
        "fallback_threshold={:.3}\n",
        fallback_gap_threshold
    ));
    out.push_str(&format!("retry_plugin={}\n", recommendation.retry_plugin));
    out.push_str(&format!(
        "recommendation_primary={}\n",
        recommendation.selected.0
    ));
    out.push_str(&format!(
        "recommendation_confidence={}\n",
        recommendation.recommendation_confidence
    ));
    out.push_str(&format!(
        "recommendation_action={}\n",
        recommendation.recommendation_action
    ));
    out.push_str(&format!(
        "recommendation_alternative_1={}\n",
        recommendation.recommendation_alternative_1
    ));
    out.push_str(&format!(
        "recommendation_alternative_2={}\n",
        recommendation.recommendation_alternative_2
    ));
    out.push_str(&format!(
        "recommendation_reason={}\n",
        sanitize_explain_field(&recommendation.recommendation_reason)
    ));
    if let Some(note) = recommendation.fallback_note.as_deref() {
        out.push_str(&format!("fallback_note={}\n", note));
    }
    out.push_str(&format!(
        "alternatives={}\n",
        recommendation.alternatives.len()
    ));
    for (idx, (name, priority, score)) in recommendation.alternatives.iter().enumerate() {
        out.push_str(&format!(
            "alternative_{}={}|score={:.3}|priority={}\n",
            idx + 1,
            name,
            score,
            priority
        ));
        render_capability_evidence_line(&format!("alternative_{}", idx + 1), name, &mut out);
    }
    if detections.is_empty() {
        out.push_str("fallback_reason=no_plugin_detector_above_threshold\n");
    }
    out.push_str("replay_case_template=available_with:--explain-replay-out <path>\n");
    out
}


pub(crate) fn write_explain_replay_template(
    path: &std::path::Path,
    input_kind: &str,
    replay_input: &str,
    report: &str,
) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(CliError::Io)?;
        }
    }

    let replay_command = if input_kind == "command" {
        format!(
            "tokenslim explain-plugin --explain-command \"{}\"",
            replay_input.replace('"', "\\\"")
        )
    } else {
        "tokenslim explain-plugin --input <log_file>".to_string()
    };

    let template = format!(
        "# Route Misclassification Replay Case\n\n\
status: todo\n\
input_kind: {input_kind}\n\
expected_plugin: <fill_when_known>\n\
observed_plugin: <copy_from_explain_output>\n\
retry_plugin: <copy_from_retry_plugin>\n\
recommendation_confidence: <copy_from_recommendation_confidence>\n\
recommendation_action: <copy_from_recommendation_action>\n\
decision: pass | needs_route_fix | needs_detector_fix | waived\n\n\
## Replay Command\n\n```powershell\n{replay_command}\n```\n\n\
## Input\n\n```text\n{replay_input}\n```\n\n\
## Explain Output\n\n```text\n{report}\n```\n\n\
## Audit Notes\n\n\
- Confirm whether `selected_plugin` is correct for this input.\n\
- Inspect `recommendation_primary/recommendation_confidence/recommendation_action/recommendation_reason` before deciding route vs detector fix.\n\
- If `fallback_decision=review_recommended`, replay with the `retry_plugin` parser path or add a focused sample case.\n\
- If this is a real misroute, create or update the plugin's sample/showcase/audit case before freezing.\n"
    );

    std::fs::write(path, template).map_err(CliError::Io)
}


pub(crate) fn read_explain_input_text(input: &InputSource) -> Result<String, CliError> {
    match input {
        InputSource::File(path) => {
            let bytes = std::fs::read(path).map_err(CliError::Io)?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
        InputSource::Stdin => {
            if std::io::stdin().is_terminal() {
                return Err(CliError::InvalidArgs(
                    crate::utils::i18n::t("err_explain_plugin_requires_input").to_string(),
                ));
            }
            let mut buffer = Vec::new();
            io::stdin().read_to_end(&mut buffer).map_err(CliError::Io)?;
            Ok(String::from_utf8_lossy(&buffer).into_owned())
        }
    }
}


pub(crate) fn handle_discover_action(args: &CliArgs) -> Result<bool, CliError> {
    let tracker =
        crate::core::tracking::Tracker::open_default().map_err(|e| CliError::Config(e))?;

    let result = crate::core::filter_discover::discover_filters(&args.discover, &tracker)
        .map_err(|e| CliError::Config(e))?;

    println!("{}", t("discover_result_header"));
    println!("{}", t1("discover_total_commands", result.total_commands));
    println!(
        "{}",
        t1(
            "discover_total_potential_savings",
            result.total_potential_savings
        )
    );

    if !result.filterable.is_empty() {
        println!(
            "{}",
            t1("discover_filterable_groups", result.filterable.len())
        );
        for group in &result.filterable {
            println!("{}", t2("discover_group_line", &group.key, group.count));
            println!(
                "{}",
                t1("discover_output_tokens", group.total_output_tokens)
            );
            if let Some(pct) = group.estimated_savings_pct {
                println!(
                    "{}",
                    t("discover_estimated_savings_pct").replace("{:.1}", &format!("{pct:.1}"))
                );
            }
            if let Some(saved) = group.estimated_tokens_saved {
                println!("{}", t1("discover_estimated_savings_tokens", saved));
            }
            println!();
        }
    }

    if !result.no_filter.is_empty() {
        println!(
            "{}",
            t1("discover_no_filter_groups", result.no_filter.len())
        );
        for group in &result.no_filter {
            println!("{}", t2("discover_group_line", &group.key, group.count));
            println!(
                "{}",
                t1("discover_output_tokens", group.total_output_tokens)
            );
            if let Some(saved) = group.estimated_tokens_saved {
                println!("{}", t1("discover_estimated_savings_tokens_default", saved));
            }
            println!();
        }
    }

    if !result.already_filtered.is_empty() {
        println!(
            "{}",
            t1(
                "discover_already_filtered_groups",
                result.already_filtered.len()
            )
        );
        for group in &result.already_filtered {
            println!("{}", t2("discover_group_line", &group.key, group.count));
        }
    }

    Ok(true)
}


pub(crate) fn handle_explain_plugin_action(args: &CliArgs) -> Result<bool, CliError> {
    let (mut raw_report, input_kind, replay_input) =
        if let Some(command_line) = args.explain_command.as_deref() {
            (
                explain_plugin_for_command_line(command_line, args),
                "command".to_string(),
                command_line.to_string(),
            )
        } else {
            let input_text = read_explain_input_text(&args.input)?;
            (
                explain_plugin_for_log_text(&input_text, args.explain_fallback_gap),
                "log".to_string(),
                input_text,
            )
        };

    if let Some(path) = args.explain_replay_out.as_deref() {
        write_explain_replay_template(path, &input_kind, &replay_input, &raw_report)?;
        raw_report.push_str(&format!(
            "replay_case_template_path={}\n",
            path.to_string_lossy()
        ));
    }
    let report = render_explain_report_by_format(&raw_report, &args.output_format)?;

    match &args.output {
        OutputTarget::File(path) => std::fs::write(path, report).map_err(CliError::Io)?,
        OutputTarget::Stdout => println!("{}", report),
    }
    Ok(true)
}

