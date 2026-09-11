//! cli export 子命令

use crate::cli::commands::run::{
    build_run_command_anchor, load_run_routes, plugins_for_run_command,
};
use crate::cli::common::*;
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

/// 返回插件能力索引文件(plugin_capability_index.json)的绝对路径(位于 CARGO_MANIFEST_DIR/docs/audit)。
pub(crate) fn capability_index_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("audit")
        .join("plugin_capability_index.json")
}

/// 从 JSON Value 取 u64 字段，缺失或非数字时返回 0。
pub(crate) fn json_u64(value: &serde_json::Value, key: &str) -> u64 {
    value.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

/// 从 JSON Value 取字符串字段，缺失时返回空串。
pub(crate) fn json_string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// 解析插件的 detect_patterns 字段：取数组前 5 个字符串，缺失则返回空。
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

/// 在插件 JSON 数组中按名称查找指定插件条目<'a>。
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

/// 将单个插件 JSON 解析为 PluginCapabilityEvidence 结构(描述/标签/路由/样本数/检测模式等)。
pub(crate) fn parse_plugin_capability_evidence(
    plugin: &serde_json::Value,
) -> PluginCapabilityEvidence {
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

/// 从能力索引加载指定插件名的 PluginCapabilityEvidence(文件缺失或插件不存在返回 None)。
pub(crate) fn load_plugin_capability_evidence(
    plugin_name: &str,
) -> Option<PluginCapabilityEvidence> {
    let bytes = std::fs::read(capability_index_path()).ok()?;
    let content = decode_capability_index_text(&bytes);
    let root: serde_json::Value = serde_json::from_str(&content).ok()?;
    let plugins = root.get("plugins")?.as_array()?;
    let plugin = find_plugin_entry(plugins, plugin_name)?;
    Some(parse_plugin_capability_evidence(plugin))
}

/// 解码能力索引文本：处理 UTF-16 LE/BE BOM 与 UTF-8(含 BOM) 的字节流为字符串。
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

/// 清理解释字段：去除回车/换行，将管道符替换为斜杠并裁剪首尾空白。
pub(crate) fn sanitize_explain_field(value: &str) -> String {
    value
        .replace('\r', " ")
        .replace('\n', " ")
        .replace('|', "/")
        .trim()
        .to_string()
}

/// 将解释报告按行解析为 key=value 对列表。
pub(crate) fn parse_explain_report_pairs(report: &str) -> Vec<(String, String)> {
    report
        .lines()
        .filter_map(|line| {
            line.split_once('=')
                .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

/// 解析管道分隔属性文本：支持 = 与 : 的 key=value 映射，首段无符号则记为 primary。
pub(crate) fn parse_explain_pipe_attributes(
    value: &str,
) -> serde_json::Map<String, serde_json::Value> {
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

/// 解析 alternative 键中的序号：剥离 alternative_ 前缀取数字，要求 >0。
pub(crate) fn parse_explain_alternative_index(key: &str) -> Option<usize> {
    let suffix = key.strip_prefix("alternative_")?;
    let index_part = suffix.split('_').next()?;
    index_part.parse::<usize>().ok().filter(|idx| *idx > 0)
}

/// 将管道属性文本转为 JSON Map：将 primary 段映射为 description 字段。
pub(crate) fn parse_capability_line_to_json(
    raw: &str,
) -> serde_json::Map<String, serde_json::Value> {
    let mut attrs = parse_explain_pipe_attributes(raw);
    if let Some(primary) = attrs.remove("primary") {
        attrs.insert("description".to_string(), primary);
    }
    attrs
}

/// 返回解释报告必需的字段名清单(输入类型/选中插件/决策/置信度/理由/候选等)。
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

/// 从字段 Map 中取字符串字段，缺失时返回给定的默认值。
pub(crate) fn explain_field_str<'a>(
    fields: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
    default: &'a str,
) -> &'a str {
    fields.get(key).and_then(|v| v.as_str()).unwrap_or(default)
}

/// 构选 selected 小节：输出选中插件、why、capability 与 declared_patterns 字段。
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

/// 构建 alternatives 小节：遍历 pairs，为每个排名条目构造 entry 并排序后返回。
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

/// 构建单条 alternative 条目：解析管道属性、注入 rank/key/raw，附插件名与能力证据后返回 JSON。
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

/// 按 rank 升序排序 alternative 条目，使推荐列表稳定有序。
pub(crate) fn sort_alternative_entries(entries: &mut [serde_json::Value]) {
    entries.sort_by(|a, b| {
        let a_rank = a.get("rank").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
        let b_rank = b.get("rank").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
        a_rank.cmp(&b_rank)
    });
}

/// 判断 key 是否属于 alternative 排名条目：以 alternative_ 开头且非顶层 alternatives/capability/declared_patterns。
pub(crate) fn is_alternative_rank_entry_key(key: &str) -> bool {
    if !key.starts_with("alternative_") || key == "alternatives" {
        return false;
    }
    !key.ends_with("_capability") && !key.ends_with("_declared_patterns")
}

/// 为某 alternative 附加其 capability 与 declared_patterns(按 alternative_{i} 键从 fields 取)。
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

/// 构建推荐小节：汇总 primary/confidence/action/alternatives 与 confidence_gap 来源等字段。
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

/// 校验解释报告契约：检查必需字段是否齐全，返回 (是否通过, 缺失字段列表)。
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

/// 收集解释字段：解析报告为 key=value 对，并构建字段名到值的 Map 返回。
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

/// 构建解释报告的 JSON 值：组装 selected/alternatives/recommendation/contract 与所有字段。
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

/// 将解释报告渲染为美化 JSON 字符串(失败时返回 Serialization 错误)。
pub(crate) fn render_explain_report_json(report: &str) -> Result<String, CliError> {
    let (pairs, fields) = collect_explain_fields(report);
    let value = build_explain_report_json_value(&pairs, &fields);

    serde_json::to_string_pretty(&value).map_err(CliError::Serialization)
}

/// 将解释报告渲染为 Markdown：按 BTreeMap 输出输入类型/选中插件/推荐/证据/候选等小节。
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

/// 按输出格式渲染解释报告：text 原样返回、markdown 结构化、json 走 JSON 构建。
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

/// 向输出追加某插件的能力证据行：加载 capability 索引，写出 description/tags/route/样本数/检测模式等。
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

/// 解析插件解释命令行：支持单/双引号与反斜杠转义的分词，引号未闭合或转义残留则返回 None。
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
                // P3-175：双引号内转义白名单——仅 `\"` 与 `\\` 视为转义；
                // 其余字符前的反斜杠（如 Windows 盘符路径 `C:\git\work`）保持字面，
                // 避免把路径分隔符吞掉导致 explain 路由解析失真。
                if escaped {
                    if ch == '"' || ch == '\\' {
                        current.push(ch);
                    } else {
                        current.push('\\');
                        current.push(ch);
                    }
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

/// 依据路由是否 fallback 与匹配方式，返回命令推荐的置信度(high/medium/low)。
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

/// 构建命令路由推荐：依据路由是否 fallback 推导 retry_plugin/fallback_decision/置信度/action 与 reason。
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

/// 向输出追加命令 alternatives 报告：逐候选写出 alternative_N/路由组/匹配方式/能力证据行。
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

/// 返回无效命令行时的兜底解释报告(固定 plugin_selection 文本)。
pub(crate) fn invalid_command_explain_report() -> String {
    "plugin_selection\ninput_kind=command\nselected_plugin=none\nreason=invalid_command_line\nalternatives=0\n".to_string()
}

/// 构建命令解释上下文：解析命令行令牌、解析运行路由与候选、构造插件候选链与 alternatives。
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

/// 渲染命令式插件选择报告：输出 command/selected_plugin/why/confidence_gap 等字段，
/// 附带候选链与 replay/输出格式提示。
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

/// 为命令行为生成插件选择解释：解析命令行定位路由，必要时回退到 invalid 报告，
/// 再渲染命令式插件选择报告。
pub(crate) fn explain_plugin_for_command_line(command_line: &str, args: &CliArgs) -> String {
    let Some(context) = build_command_explain_context(command_line) else {
        return invalid_command_explain_report();
    };
    render_command_plugin_selection_report(command_line, args, &context)
}

/// 判断某插件名是否为可重试的解释候选：排除 ansi_cleaner/generic_text 等基础插件。
pub(crate) fn is_retryable_explain_plugin(name: &str) -> bool {
    !matches!(
        name,
        "ansi_cleaner"
            | "generic_text"
            | "noise_filter"
            | "smart_code"
            | "smart_path"
            | "static_rule"
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

/// 收集日志切片上的插件检测器打分：调用各插件 detect，过滤 score>0.1，
/// 按分数降序、优先级、名称排序后返回 (插件名, 优先级, 分数)。
pub(crate) fn collect_log_detections(slice: &Slice<'_>) -> Vec<(String, u8, f32)> {
    let trace = std::env::var("TOKENSLIM_REPLAY_TRACE").is_ok();
    let mut detections = get_plugins()
        .into_iter()
        .filter_map(|plugin| {
            let r = plugin.detect(slice);
            if trace {
                eprintln!("[detect] {} -> {:?}", plugin.name(), r);
            }
            r.filter(|score| *score > 0.1)
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

/// 构建日志解释推荐：取最高分检测器为 selected，其余为 alternatives，
/// 依据分数差与阈值判定 fallback/retry 决策、置信度与 action。
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

/// 为日志文本生成插件选择解释报告：构造 Slice、收集检测器打分、构建日志推荐，
/// 输出 selected/alternatives/confidence_gap 等字段及 replay 模板提示。
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
    let mut recommendation = build_log_explain_recommendation(&detections, fallback_gap_threshold);

    // 命令锚点对齐真实路由：若首行是命中的命令行（且非兜底路由），镜像 run 模式
    // 的命令锚点路由（压缩协议法则 0）——以锚点选中的插件为最终 selected。
    // 否则纯内容打分会把含 CRLF 行尾的日志误判给 noise_filter 等内容无关插件。
    let anchor = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .and_then(|first| build_command_explain_context(first.trim()))
        .filter(|ctx| !ctx.route.is_fallback)
        .map(|ctx| {
            (
                ctx.route.plugin_name.clone(),
                ctx.route.route_group.clone(),
                format!(
                    "command_anchor|route_group:{}|tool:{}|matched_by:{}",
                    ctx.route.route_group, ctx.route.command_keyword, ctx.route.matched_by
                ),
            )
        });

    if let Some((ap, _rgroup, _why)) = &anchor {
        let content_top = recommendation.selected.0.clone();
        let prio = get_plugins()
            .into_iter()
            .find(|p| p.name() == *ap)
            .map(|p| p.priority())
            .unwrap_or(0);
        recommendation.selected = (ap.clone(), prio, 1.0);
        recommendation.recommendation_reason =
            format!("command_anchor_aligned|route:{ap}|priority:{prio}|content_top:{content_top}");
    }

    let mut out = String::new();
    out.push_str("plugin_selection\n");
    out.push_str("input_kind=log\n");
    out.push_str(&format!("line_count={}\n", text.lines().count()));
    out.push_str(&format!("byte_count={}\n", text.len()));
    out.push_str(&format!("selected_plugin={}\n", recommendation.selected.0));
    if let Some((_, _, why)) = &anchor {
        out.push_str("selection_source=command_anchor\n");
        out.push_str(&format!("why={}\n", why));
    } else {
        out.push_str("selection_source=content_detector\n");
        out.push_str(&format!(
            "why=content_detector_score:{:.3}|plugin_priority:{}|candidate_rank:1\n",
            recommendation.selected.2, recommendation.selected.1
        ));
    }
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

/// 写出 replay 用例模板：依据 input_kind 生成 replay 命令与结构化模板文件(含输入/输出/审计笔记)。
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

/// 读取 explain 输入文本：文件按字节读取为文本；标准输入且为终端(非管道)则报错要求提供输入。
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

/// 处理 discover 子命令：打开默认 tracker，发现可过滤的命令组并统计潜在 token 节省，
/// 分 filterable/no_filter/already_filtered 三类打印结果。
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
            } else {
                // P2-45：该组无真实 token 元数据（DeepSeek/Claude 解析未补采到 usage），
                // 诚实标注无法估算，而非打印误导性的恒 0 节省。
                println!("{}", t("discover_estimated_unavailable"));
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
            } else {
                // P2-45：无 token 元数据的组不产出估算值，诚实标注无法估算。
                println!("{}", t("discover_estimated_unavailable"));
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

/// 处理 explain-plugin 子命令：按 --explain-command 或输入文本生成插件选择报告，
/// 可选写 replay 模板，并按 text/markdown/json 格式输出到文件或标准输出。
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

// ---------------------------------------------------------------------------
// 插件选择前瞻回放（方案 1：插件选择能力实战化 —— 全量 frozen 样本前瞻比对）
// ---------------------------------------------------------------------------
//
// 目的：把「插件选择」从一个不可度量的黑盒变成一个可度量的门禁。
// 做法：遍历 samples/<plugin>/case_* 全部真实样本，逐条通过运行时的
// `explain_plugin_for_log_text`（与打包/压缩共用同一套选择逻辑，含命令锚点
// 对齐）回放选择结果，与样本所属插件文件夹比对，输出失配清单。
//
// 门禁语义（不脆弱、可长期守住）：
//   1) 命令锚定的内容样本绝不允许被清理类插件（noise_filter/generic_text/
//      ansi_cleaner 等）抢占 —— 这正是修复前 CRLF 行尾导致 VCS 合并日志被
//      noise_filter 抢占的回归类。
//   2) 其余内容重叠导致的「归属插件 != 选中插件」列为信息性失配，写进报告，
//      供人工/LLM 复盘，但不断言（60+ 插件内容检测天然重叠）。
// 跨切面插件目录（noise/ansi/generic/smart/static/template、encoding_fallback、
// explain_plugin 展示样本）不参与严格归属比对，仅统计。

/// 插件选择前瞻回放的统计结果，供测试与报告消费。
#[derive(Debug, Default)]
pub(crate) struct PluginSelectionReplayStats {
    /// 参与比对的样本总数（非跨切面目录）。
    pub(crate) strict_owner_total: usize,
    /// 归属插件与选中插件一致的样本数。
    pub(crate) strict_owner_match: usize,
    /// 命令锚定且命中非兜底路由的样本数。
    pub(crate) command_anchored: usize,
    /// 序列化冲突插件目录（noise/ansi/generic 等跨切面插件）下被跳过的样本数。
    pub(crate) rollup_excluded: usize,
    /// 干净路由被清理类插件抢占的门禁违规清单（本应恒为空）。
    pub(crate) cleanup_steal_violations: Vec<String>,
    /// 信息性完整失配清单（归属插件 != 选中插件）。
    pub(crate) mismatches: Vec<String>,
}

/// 跨切面/不可严格归属的样本地目录：这些插件的样本是给压缩链路做兜底/清理的，
/// 输入往往是通用文本，不保证内容检测把「归属文件夹」当成首选插件。
fn is_replay_rollup_dir(dir: &str) -> bool {
    matches!(
        dir,
        "noise_filter_plugin"
            | "ansi_cleaner_plugin"
            | "generic_text_plugin"
            | "smart_code_plugin"
            | "smart_path_plugin"
            | "static_rule_plugin"
            | "template_driven_plugin"
            | "encoding_fallback"
            | "explain_plugin"
    )
}

/// 解析 explain 文本报告的 selected_plugin 字段值。
fn replay_selected_plugin(report: &str) -> &str {
    report
        .lines()
        .find_map(|l| l.strip_prefix("selected_plugin="))
        .unwrap_or("generic_text")
}

/// 全量 frozen 样本插件选择前瞻回放：遍历 samples/ 逐样本运行运行时的选择逻辑，
/// 产出 markdown 报告文本与统计结果。
pub(crate) fn plugin_selection_replay_report() -> (String, PluginSelectionReplayStats) {
    let mut stats = PluginSelectionReplayStats::default();
    let samples_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples");

    let mut rows: Vec<String> = Vec::new();

    let Ok(dir_iter) = std::fs::read_dir(&samples_root) else {
        return (
            "# Plugin Selection Forward Replay Report\n\n- error: samples dir unreadable\n"
                .to_string(),
            stats,
        );
    };
    for entry in dir_iter.flatten() {
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        let dir_path = entry.path();
        if !dir_path.is_dir() {
            continue;
        }
        // 归属插件名：samples/<dir>/ 目录名去掉 `_plugin` 后缀；无后缀目录
        // （encoding_fallback/explain_plugin）由 is_replay_rollup_dir 直接排除。
        let owner = dir_name
            .strip_suffix("_plugin")
            .map(ToString::to_string)
            .unwrap_or_default();
        let rollup = is_replay_rollup_dir(&dir_name);

        let Ok(case_iter) = std::fs::read_dir(&dir_path) else {
            continue;
        };
        for case_entry in case_iter.flatten() {
            let file_name = case_entry.file_name().to_string_lossy().into_owned();
            let case_path = case_entry.path();
            if !case_entry.file_type().map(|t| t.is_file()).unwrap_or(false)
                || !file_name.starts_with("case_")
                || file_name.contains(".scenario.yaml")
            {
                continue;
            }
            let text = std::fs::read_to_string(&case_path)
                .unwrap_or_default()
                .replace('\r', ""); // 统一 CRLF，避免 CRLF 行尾干扰基线裁剪
                                    // 样本可读性保护：读失败即跳过（如二进制/编码未识别文件）。
            if text.is_empty() && case_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                continue;
            }

            if rollup {
                stats.rollup_excluded += 1;
                rows.push(format!(
                    "| `{owner}(rolled)` | `{file_name}` | - | skip | rollup_dir |"
                ));
                continue;
            }

            stats.strict_owner_total += 1;
            eprintln!("[replay] {dir_name}/{file_name} len={}", text.len());
            let report = explain_plugin_for_log_text(&text, 0.15);
            let selected = replay_selected_plugin(&report);
            let owner = owner.as_str();
            let matched = selected == owner;

            // 命令锚点核对：首行若是命中非兜底路由的命令，则选中插件被命令锚点
            // 强制对齐到路由插件。此分支是「清理类插件抢占」回归的门禁位。
            let anchor_plugin = text
                .lines()
                .find(|l| !l.trim().is_empty())
                .and_then(|first| build_command_explain_context(first.trim()))
                .filter(|ctx| !ctx.route.is_fallback)
                .map(|ctx| ctx.route.plugin_name.clone());

            if let Some(anchor) = &anchor_plugin {
                stats.command_anchored += 1;
                // 清理类插件无法成为命令路由；若锚定后 selected 反而落到清理类
                // 插件，说明命令锚点对齐失效——这是最严重的误判回归。
                let is_cleanup = matches!(
                    selected,
                    "noise_filter" | "ansi_cleaner" | "generic_text" | "smart_code" | "smart_path"
                );
                if is_cleanup || (anchor.as_str() != selected && &*selected != owner) {
                    stats.cleanup_steal_violations.push(format!(
                        "samples/{dir_name}/{file_name}: owner={owner} anchor={anchor} selected={selected}"
                    ));
                }
            }

            if matched {
                stats.strict_owner_match += 1;
                rows.push(format!(
                    "| `{owner}` | `{file_name}` | `{selected}` | match | - |"
                ));
            } else {
                let note = if let Some(a) = &anchor_plugin {
                    format!("anchor_route:{a}")
                } else {
                    "content_overlap".to_string()
                };
                stats.mismatches.push(format!(
                    "samples/{dir_name}/{file_name}: owner={owner} selected={selected} note={note}"
                ));
                rows.push(format!(
                    "| `{owner}` | `{file_name}` | `{selected}` | mismatch | {note} |"
                ));
            }
        }
    }

    let match_rate = if stats.strict_owner_total > 0 {
        stats.strict_owner_match as f64 / stats.strict_owner_total as f64
    } else {
        0.0
    };
    let mut md = String::new();
    md.push_str("# Plugin Selection Forward Replay Report\n\n");
    md.push_str(&format!(
        "- strict_owner_total: {}\n",
        stats.strict_owner_total
    ));
    md.push_str(&format!(
        "- strict_owner_match: {}\n",
        stats.strict_owner_match
    ));
    md.push_str(&format!("- strict_owner_match_rate: {:.3}\n", match_rate));
    md.push_str(&format!("- command_anchored: {}\n", stats.command_anchored));
    md.push_str(&format!("- rollup_excluded: {}\n", stats.rollup_excluded));
    md.push_str(&format!(
        "- cleanup_steal_violations: {}\n",
        stats.cleanup_steal_violations.len()
    ));
    md.push_str(&format!("- mismatch_count: {}\n", stats.mismatches.len()));
    md.push('\n');
    md.push_str("## Mismatch List\n\n");
    if stats.mismatches.is_empty() {
        md.push_str("- none\n");
    } else {
        for m in &stats.mismatches {
            md.push_str(&format!("- {m}\n"));
        }
    }
    md.push_str("\n## Cleanup Steal Violations\n\n");
    if stats.cleanup_steal_violations.is_empty() {
        md.push_str("- none\n");
    } else {
        for v in &stats.cleanup_steal_violations {
            md.push_str(&format!("- {v}\n"));
        }
    }
    md.push_str("\n## Per-Sample Detail\n\n");
    md.push_str("| owning | sample | selected | verdict | note |\n|---|---|---|---|---|\n");
    for r in &rows {
        md.push_str(&format!("{r}\n"));
    }

    (md, stats)
}
