use super::types::CloudLogPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use serde_json::Value;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Debug, Clone)]
struct CloudRecord {
    provider: String,
    time: String,
    source: Option<String>,
    level: Option<String>,
    message: String,
}

#[derive(Debug, Clone)]
struct AccessRecord {
    provider: String,
    time: String,
    source: Option<String>,
    level: String,
    ip: String,
    method: String,
    path: String,
    status: String,
    reason: String,
}

#[derive(Debug, Default)]
struct CloudCollectOutput {
    command_lines: Vec<String>,
    records: Vec<CloudRecord>,
    passthrough: Vec<String>,
}

#[derive(Debug, Default)]
struct CloudCollectState {
    csv_headers: Option<Vec<String>>,
    pipe_headers: Option<Vec<String>>,
    provider_hint: Option<String>,
}

impl CloudLogPlugin {
    pub fn new() -> Self {
        Self {
            name: "cloud_log",
            priority: 41,
            aws_tail_pattern: Arc::new(
                Regex::new(r#"^(?P<time>\d{4}-\d{2}-\d{2}T\S+)\s+(?P<source>\S+)\s+(?P<message>.+)$"#)
                    .unwrap(),
            ),
            generic_cloud_line_pattern: Arc::new(
                Regex::new(r#"^(?P<time>\d{4}-\d{2}-\d{2}T\S+)\s+(?P<level>INFO|WARN|WARNING|ERROR|DEBUG|TRACE|CRITICAL|NOTICE)\s+(?P<source>\S+)\s+(?P<message>.+)$"#)
                    .unwrap(),
            ),
            uvicorn_access_pattern: Arc::new(
                Regex::new(r#"^(?P<level>[A-Z]+):\s+(?P<ip>[\da-fA-F:\.]+):\d+\s+-\s+"(?P<method>[A-Z]+)\s+(?P<path>[^\s"]+)\s+HTTP/[0-9.]+"\s+(?P<status>\d{3})\s+(?P<reason>.*)$"#)
                    .unwrap(),
            ),
            aws_lambda_pattern: Arc::new(
                Regex::new(r#"^\[(?P<level>[A-Z]+)\]\s+(?P<time>\d{4}-\d{2}-\d{2}T\S+Z)\s+(?P<source>[a-f0-9\-]+)\s+(?P<message>.+)$"#)
                    .unwrap(),
            ),
            standard_bracket_pattern: Arc::new(
                Regex::new(r#"^\[(?P<time>\d{4}-\d{2}-\d{2}\s\d{2}:\d{2}:\d{2},\d+)\]\s+(?P<level>[A-Z]+):\s+(?P<message>.+)$"#)
                    .unwrap(),
            ),
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn parse_access_record(&self, record: &CloudRecord) -> Option<AccessRecord> {
        let caps = self
            .uvicorn_access_pattern
            .captures(record.message.trim())?;
        Some(AccessRecord {
            provider: record.provider.clone(),
            time: compact_cloud_time(&record.time),
            source: record.source.as_deref().map(compact_cloud_source),
            level: caps.name("level")?.as_str().to_string(),
            ip: caps.name("ip")?.as_str().to_string(),
            method: caps.name("method")?.as_str().to_string(),
            path: caps.name("path")?.as_str().to_string(),
            status: caps.name("status")?.as_str().to_string(),
            reason: compact_spaces(caps.name("reason")?.as_str()),
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn parse_cloud_record(
        &self,
        line: &str,
        csv_headers: Option<&[String]>,
    ) -> Option<CloudRecord> {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_table_noise(trimmed) || is_cloud_command_line(trimmed) {
            return None;
        }

        self.parse_cloud_record_from_structured(trimmed, csv_headers)
            .or_else(|| self.parse_cloud_record_from_text_patterns(trimmed))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn parse_cloud_record_from_structured(
        &self,
        trimmed: &str,
        csv_headers: Option<&[String]>,
    ) -> Option<CloudRecord> {
        if let Some(record) = parse_json_record(trimmed) {
            return Some(record);
        }
        if let Some(headers) = csv_headers {
            if let Some(record) = parse_csv_record(headers, trimmed) {
                return Some(record);
            }
        }
        parse_pipe_table_record(trimmed)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn parse_cloud_record_from_text_patterns(&self, trimmed: &str) -> Option<CloudRecord> {
        if let Some(caps) = self.generic_cloud_line_pattern.captures(trimmed) {
            return Some(CloudRecord {
                provider: infer_provider(trimmed),
                time: caps.name("time")?.as_str().to_string(),
                source: Some(caps.name("source")?.as_str().to_string()),
                level: Some(caps.name("level")?.as_str().to_string()),
                message: caps.name("message")?.as_str().to_string(),
            });
        }

        if let Some(caps) = self.aws_tail_pattern.captures(trimmed) {
            let source = caps.name("source")?.as_str();
            let message = caps.name("message")?.as_str();
            if looks_like_cloud_source(source) && looks_like_inner_log(message) {
                return Some(CloudRecord {
                    provider: "aws".to_string(),
                    time: caps.name("time")?.as_str().to_string(),
                    source: Some(source.to_string()),
                    level: None,
                    message: message.to_string(),
                });
            }
        }

        if let Some(caps) = self.aws_lambda_pattern.captures(trimmed) {
            return Some(CloudRecord {
                provider: "aws".to_string(),
                time: caps.name("time")?.as_str().to_string(),
                source: Some(caps.name("source")?.as_str().to_string()),
                level: Some(caps.name("level")?.as_str().to_string()),
                message: caps.name("message")?.as_str().to_string(),
            });
        }

        if let Some(caps) = self.standard_bracket_pattern.captures(trimmed) {
            return Some(CloudRecord {
                provider: "cloud".to_string(),
                time: caps.name("time")?.as_str().to_string(),
                source: None,
                level: Some(caps.name("level")?.as_str().to_string()),
                message: caps.name("message")?.as_str().to_string(),
            });
        }

        None
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn collect_cloud_records(&self, text: &str) -> CloudCollectOutput {
        let mut output = CloudCollectOutput::default();
        let mut state = CloudCollectState::default();

        for line in text.lines() {
            let trimmed = line.trim();
            if update_collect_state_from_line(line, trimmed, &mut state, &mut output.command_lines)
            {
                continue;
            }
            if is_table_noise(trimmed) {
                continue;
            }
            if let Some(mut record) = try_collect_record_from_line(self, &state, line, trimmed) {
                apply_provider_hint(&mut record, state.provider_hint.as_deref());
                output.records.push(record);
            } else if !trimmed.is_empty() {
                output.passthrough.push(line.to_string());
            }
        }

        output
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn compress_cloud_records(&self, text: &str) -> Option<String> {
        if let Some(output) = render_multiline_csv_records(text) {
            return Some(output);
        }
        let CloudCollectOutput {
            command_lines,
            records,
            passthrough,
        } = self.collect_cloud_records(text);

        if records.is_empty() {
            return None;
        }

        let mut out = String::new();
        append_lines_with_newline(&mut out, &command_lines);

        if let Some(access_output) = self.try_render_access_summary(&records) {
            out.push_str(&access_output);
            return Some(out);
        }

        append_cloud_meta_line(&mut out, &records);
        append_cloud_record_messages(&mut out, &records);
        append_lines_with_newline(&mut out, &passthrough);

        Some(out)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    fn try_render_access_summary(&self, records: &[CloudRecord]) -> Option<String> {
        let access_records = collect_strict_access_records(self, records)?;
        if access_records.len() < 2 {
            return None;
        }
        let grouped_records = group_access_records(access_records);
        let mut out = String::new();
        for grouped in grouped_records {
            if let Some(line) = format_access_summary_line(&grouped) {
                out.push_str(&line);
            }
        }
        Some(out)
    }
}

impl Default for CloudLogPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for CloudLogPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    fn unwrap(&self, text: &str) -> Option<String> {
        let CloudCollectOutput {
            command_lines,
            records,
            passthrough,
        } = self.collect_cloud_records(text);

        if records.is_empty() {
            return None;
        }

        let mut out = String::new();
        for line in command_lines {
            out.push_str(&line);
            out.push('\n');
        }

        for record in records {
            out.push_str(record.message.trim_end());
            out.push('\n');
        }

        for line in passthrough {
            out.push_str(&line);
            out.push('\n');
        }

        Some(out)
    }

    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        let mut score: f32 = 0.0;
        score += detect_command_hint_score(text);
        score += detect_structured_field_hint_score(text);
        score += detect_table_or_csv_hint_score(text);
        score += detect_parse_ratio_score(self, text);

        if score >= 0.35 {
            Some(score.min(1.0))
        } else {
            None
        }
    }

    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();
        let compacted = self
            .compress_cloud_records(text)
            .map(|output| crate::core::utils::roi::prefer_non_expanding(text, output))
            .unwrap_or_else(|| text.to_string());

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(compacted))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn next_plugins(&self) -> Vec<&'static str> {
        vec![]
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn detect_command_hint_score(text: &str) -> f32 {
    if text.contains("aws logs tail")
        || text.contains("gcloud logging")
        || text.contains("az monitor")
        || text.contains("aliyun")
        || text.contains("oci logging")
        || text.contains("tccli cls")
        || text.contains("hcloud lts")
        || text.contains("wrangler tail")
    {
        0.45
    } else {
        0.0
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn detect_structured_field_hint_score(text: &str) -> f32 {
    if text.contains("\"textPayload\"")
        || text.contains("\"jsonPayload\"")
        || text.contains("\"httpRequest\"")
        || text.contains("\"logGroup\"")
        || text.contains("\"logStream\"")
        || text.contains("\"logStreamName\"")
        || text.contains("\"TimeGenerated\"")
        || text.contains("\"@message\"")
        || text.contains("__time__")
        || text.contains("\"timeLocal\"")
        || text.contains("\"logContent\"")
        || text.contains("\"rayID\"")
        || text.contains("\"RayID\"")
    {
        0.45
    } else {
        0.0
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn detect_table_or_csv_hint_score(text: &str) -> f32 {
    let mut score = 0.0f32;
    if text.contains("| timestamp")
        || text.contains("| TimeGenerated")
        || text.contains("| @timestamp")
        || text.contains("| datetime")
    {
        score += 0.35;
    }
    if text.contains("timestamp,")
        || text.contains("TimeGenerated,")
        || text.contains("@timestamp,")
        || text.contains("datetime,")
    {
        score += 0.35;
    }
    score
}

#[tracing::instrument(level = "debug", skip_all)]
fn detect_parse_ratio_score(plugin: &CloudLogPlugin, text: &str) -> f32 {
    let mut matched = 0usize;
    let mut total = 0usize;
    let mut csv_headers: Option<Vec<String>> = None;
    for line in text.lines().take(12) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        total += 1;
        if let Some(headers) = parse_csv_header(trimmed) {
            csv_headers = Some(headers);
            matched += 1;
            continue;
        }
        if plugin
            .parse_cloud_record(trimmed, csv_headers.as_deref())
            .is_some()
        {
            matched += 1;
        }
    }
    if total > 0 {
        (matched as f32 / total as f32) * 0.5
    } else {
        0.0
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn parse_json_record(line: &str) -> Option<CloudRecord> {
    let json = serde_json::from_str::<Value>(line).ok()?;
    let message = extract_cloud_json_message(&json)?;

    Some(CloudRecord {
        provider: infer_provider_from_json(&json).unwrap_or_else(|| infer_provider(line)),
        time: extract_cloud_json_time(&json),
        source: extract_cloud_json_source(&json),
        level: extract_cloud_json_level(&json),
        message,
    })
}

#[tracing::instrument(level = "debug", skip_all)]
fn extract_cloud_json_message(json: &Value) -> Option<String> {
    first_json_string(
        json,
        &[
            &["message"],
            &["@message"],
            &["log"],
            &["msg"],
            &["content"],
            &["textPayload"],
            &["jsonPayload", "message"],
            &["jsonPayload", "msg"],
            &["protoPayload", "status", "message"],
            &["properties", "message"],
            &["LogMessage"],
            &["ResultDescription"],
            &["Message"],
            &["data", "message"],
            &["data", "logContent"],
            &["data", "content"],
            &["logContent"],
            &["log_content"],
            &["Content"],
            &["event"],
            &["request", "url"],
        ],
    )
    .or_else(|| synthesize_http_access_message(json))
}

#[tracing::instrument(level = "debug", skip_all)]
fn extract_cloud_json_time(json: &Value) -> String {
    first_json_string(
        json,
        &[
            &["timestamp"],
            &["time"],
            &["@timestamp"],
            &["datetime"],
            &["TimeGenerated"],
            &["__time__"],
            &["receiveTimestamp"],
            &["eventTime"],
            &["timeLocal"],
            &["Timestamp"],
            &["timeUnixNano"],
            &["EdgeStartTimestamp"],
        ],
    )
    .unwrap_or_default()
}

#[tracing::instrument(level = "debug", skip_all)]
fn extract_cloud_json_source(json: &Value) -> Option<String> {
    first_json_string(
        json,
        &[
            &["logStream"],
            &["@logStream"],
            &["logStreamName"],
            &["logName"],
            &["resource", "labels", "container_name"],
            &["resourceId"],
            &["Category"],
            &["__source__"],
            &["source"],
            &["resourceName"],
            &["topic"],
            &["logsetName"],
            &["logGroupName"],
            &["containerName"],
            &["cloud_RoleName"],
            &["scriptName"],
            &["rayID"],
            &["RayID"],
        ],
    )
}

#[tracing::instrument(level = "debug", skip_all)]
fn extract_cloud_json_level(json: &Value) -> Option<String> {
    first_json_string(
        json,
        &[
            &["level"],
            &["severity"],
            &["SeverityLevel"],
            &["LogLevel"],
            &["priority"],
            &["type"],
            &["status"],
            &["levelName"],
        ],
    )
}

#[tracing::instrument(level = "debug", skip_all)]
fn infer_provider_from_json(json: &Value) -> Option<String> {
    if json.get("logGroup").is_some()
        || json.get("logStream").is_some()
        || json.get("logStreamName").is_some()
        || json.get("@logStream").is_some()
        || json.get("@message").is_some()
    {
        return Some("aws".to_string());
    }
    if json.get("textPayload").is_some()
        || json.get("jsonPayload").is_some()
        || json.get("protoPayload").is_some()
        || json.get("logName").is_some()
    {
        return Some("gcp".to_string());
    }
    if json.get("TimeGenerated").is_some() || json.get("resourceId").is_some() {
        return Some("azure".to_string());
    }
    if json.get("__time__").is_some() || json.get("__source__").is_some() {
        return Some("aliyun".to_string());
    }
    if json.get("logsetName").is_some() || json.get("topic").is_some() {
        return Some("tencent".to_string());
    }
    if json.get("logGroupName").is_some() || json.get("log_content").is_some() {
        return Some("huawei".to_string());
    }
    if json.get("scriptName").is_some() || json.get("rayID").is_some() {
        return Some("cloudflare".to_string());
    }
    if json.get("RayID").is_some()
        || json.get("EdgeStartTimestamp").is_some()
        || json.get("ClientRequestURI").is_some()
    {
        return Some("cloudflare".to_string());
    }
    let source = first_json_string(json, &[&["source"], &["type"]]).unwrap_or_default();
    let lower = source.to_ascii_lowercase();
    if lower.contains("ocid1.") || lower.contains("oraclecloud") {
        return Some("oci".to_string());
    }
    None
}

#[tracing::instrument(level = "debug", skip_all)]
fn first_json_string(json: &Value, paths: &[&[&str]]) -> Option<String> {
    for path in paths {
        let mut cur = json;
        let mut matched = true;
        for key in *path {
            if let Some(next) = cur.get(*key) {
                cur = next;
            } else {
                matched = false;
                break;
            }
        }
        if !matched {
            continue;
        }
        if let Some(value) = cur.as_str() {
            return Some(value.to_string());
        }
        if cur.is_number() || cur.is_boolean() {
            return Some(cur.to_string());
        }
    }
    None
}

#[tracing::instrument(level = "debug", skip_all)]
fn synthesize_http_access_message(json: &Value) -> Option<String> {
    if let Some(message) = synthesize_http_access_from_path(
        json,
        &["httpRequest"],
        &["remoteIp", "clientIp", "ClientIP", "sourceIp", "ip"],
        &["requestMethod", "method", "ClientRequestMethod"],
        &["requestUrl", "requestUri", "url", "ClientRequestURI"],
        &["status", "statusCode", "EdgeResponseStatus"],
    ) {
        return Some(message);
    }

    if let Some(message) = synthesize_http_access_from_path(
        json,
        &["jsonPayload"],
        &["remoteIp", "clientIp", "ClientIP", "sourceIp", "ip"],
        &["requestMethod", "method", "ClientRequestMethod"],
        &[
            "requestUrl",
            "requestUri",
            "path",
            "url",
            "ClientRequestURI",
        ],
        &["status", "statusCode", "EdgeResponseStatus"],
    ) {
        return Some(message);
    }

    synthesize_http_access_from_path(
        json,
        &[],
        &["ClientIP", "remoteIp", "clientIp", "sourceIp", "ip"],
        &["ClientRequestMethod", "requestMethod", "method"],
        &[
            "ClientRequestURI",
            "requestUrl",
            "requestUri",
            "path",
            "url",
        ],
        &[
            "EdgeResponseStatus",
            "status",
            "statusCode",
            "elb_status_code",
        ],
    )
}

#[tracing::instrument(level = "debug", skip_all)]
fn synthesize_http_access_from_path(
    json: &Value,
    object_path: &[&str],
    ip_fields: &[&str],
    method_fields: &[&str],
    path_fields: &[&str],
    status_fields: &[&str],
) -> Option<String> {
    let object = json_at_path(json, object_path)?;
    let method = first_object_string(object, method_fields)?;
    let path = first_object_string(object, path_fields)?;
    let status = first_object_string(object, status_fields)?;
    let ip = first_object_string(object, ip_fields).unwrap_or_else(|| "-".to_string());
    let path = normalize_access_path(&path);
    let reason = if status == "200" { "OK" } else { "ERR" };
    Some(format!(
        "INFO: {ip}:0 - \"{} {} HTTP/1.1\" {} {}",
        method.to_ascii_uppercase(),
        path,
        status,
        reason
    ))
}

#[tracing::instrument(level = "debug", skip_all)]
fn json_at_path<'a>(json: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = json;
    for key in path {
        cur = cur.get(*key)?;
    }
    Some(cur)
}

#[tracing::instrument(level = "debug", skip_all)]
fn first_object_string(object: &Value, fields: &[&str]) -> Option<String> {
    for field in fields {
        if let Some(value) = object.get(*field) {
            if let Some(text) = value.as_str() {
                if !text.trim().is_empty() {
                    return Some(text.trim().to_string());
                }
            }
            if value.is_number() || value.is_boolean() {
                return Some(value.to_string());
            }
        }
    }
    None
}

#[tracing::instrument(level = "debug", skip_all)]
fn normalize_access_path(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        if let Some(after_scheme) = trimmed.split_once("://").map(|(_, rest)| rest) {
            if let Some((_, path)) = after_scheme.split_once('/') {
                return format!("/{path}");
            }
        }
    }
    trimmed.to_string()
}

#[tracing::instrument(level = "debug", skip_all)]
fn parse_csv_header(line: &str) -> Option<Vec<String>> {
    if !line.contains(',') {
        return None;
    }
    let fields = split_csv_line(line);
    let lower = fields
        .iter()
        .map(|field| field.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let has_time = lower.iter().any(|field| {
        matches!(
            field.as_str(),
            "timestamp" | "time" | "timegenerated" | "__time__" | "@timestamp" | "datetime"
        )
    });
    let has_message = lower.iter().any(|field| {
        matches!(
            field.as_str(),
            "message"
                | "@message"
                | "msg"
                | "log"
                | "content"
                | "textpayload"
                | "logmessage"
                | "logcontent"
                | "log_content"
        )
    });
    if has_time && has_message {
        Some(fields)
    } else {
        None
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn infer_provider_from_headers(headers: &[String]) -> Option<String> {
    let lower = headers
        .iter()
        .map(|field| field.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if lower
        .iter()
        .any(|field| field == "__time__" || field == "__source__")
    {
        return Some("aliyun".to_string());
    }
    if lower
        .iter()
        .any(|field| field == "logsetname" || field == "logcontent")
    {
        return Some("tencent".to_string());
    }
    if lower
        .iter()
        .any(|field| field == "loggroupname" || field == "log_content")
    {
        return Some("huawei".to_string());
    }
    if lower
        .iter()
        .any(|field| field == "timegenerated" || field == "resourceid")
    {
        return Some("azure".to_string());
    }
    if lower
        .iter()
        .any(|field| field == "cloud_rolename" || field == "severitylevel")
    {
        return Some("azure".to_string());
    }
    if lower
        .iter()
        .any(|field| field == "@timestamp" || field == "@message" || field == "logstreamname")
    {
        return Some("aws".to_string());
    }
    if lower
        .iter()
        .any(|field| field == "rayid" || field == "clientrequesturi")
    {
        return Some("cloudflare".to_string());
    }
    None
}

#[tracing::instrument(level = "debug", skip_all)]
fn parse_csv_record(headers: &[String], line: &str) -> Option<CloudRecord> {
    if !line.contains(',') {
        return None;
    }
    let fields = split_csv_line(line);
    if fields.len() < headers.len().min(2) {
        return None;
    }

    let message = pick_field_by_header_names(
        headers,
        &fields,
        &[
            "message",
            "@message",
            "msg",
            "log",
            "content",
            "textPayload",
            "LogMessage",
            "logContent",
            "log_content",
            "Content",
        ],
    )?;

    Some(CloudRecord {
        provider: infer_provider_from_headers(headers).unwrap_or_else(|| infer_provider(line)),
        time: pick_field_by_header_names(
            headers,
            &fields,
            &[
                "timestamp",
                "time",
                "TimeGenerated",
                "__time__",
                "@timestamp",
                "datetime",
            ],
        )
        .unwrap_or_default(),
        source: pick_field_by_header_names(
            headers,
            &fields,
            &[
                "logStream",
                "@logStream",
                "logStreamName",
                "resourceId",
                "resource",
                "Category",
                "source",
                "__source__",
                "resourceName",
                "topic",
                "logsetName",
                "logGroupName",
                "containerName",
                "cloud_RoleName",
                "scriptName",
                "rayID",
                "RayID",
            ],
        ),
        level: pick_field_by_header_names(
            headers,
            &fields,
            &[
                "level",
                "severity",
                "SeverityLevel",
                "LogLevel",
                "type",
                "status",
                "levelName",
            ],
        ),
        message: merge_structured_message_fields(headers, &fields, &message),
    })
}

#[tracing::instrument(level = "debug", skip_all)]
fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    fields.push(current.trim().to_string());
    fields
}

#[tracing::instrument(level = "debug", skip_all)]
fn parse_pipe_table_record(line: &str) -> Option<CloudRecord> {
    if !line.starts_with('|') || !line.ends_with('|') {
        return None;
    }
    let cells = line
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .filter(|cell| !cell.is_empty())
        .collect::<Vec<_>>();
    if cells.len() < 2 {
        return None;
    }

    let time = cells
        .iter()
        .find(|cell| looks_like_pipe_table_time(cell))
        .cloned()
        .unwrap_or_default();
    let level = cells.iter().find(|cell| is_pipe_table_level(cell)).cloned();
    let message = cells.last().cloned()?;

    Some(CloudRecord {
        provider: infer_provider(line),
        time,
        source: None,
        level,
        message,
    })
}

#[tracing::instrument(level = "debug", skip_all)]
fn update_collect_state_from_line(
    line: &str,
    trimmed: &str,
    state: &mut CloudCollectState,
    command_lines: &mut Vec<String>,
) -> bool {
    if is_cloud_command_line(trimmed) {
        command_lines.push(line.to_string());
        return true;
    }
    if let Some(headers) = parse_csv_header(trimmed) {
        state.provider_hint = infer_provider_from_headers(&headers);
        state.csv_headers = Some(headers);
        return true;
    }
    if let Some(headers) = parse_pipe_header(trimmed) {
        state.provider_hint = infer_provider_from_headers(&headers);
        state.pipe_headers = Some(headers);
        return true;
    }
    if let Some(hint) = infer_provider_hint_from_pipe_line(trimmed) {
        state.provider_hint = Some(hint.to_string());
    }
    false
}

#[tracing::instrument(level = "debug", skip_all)]
fn try_collect_record_from_line(
    plugin: &CloudLogPlugin,
    state: &CloudCollectState,
    line: &str,
    trimmed: &str,
) -> Option<CloudRecord> {
    if let Some(headers) = state.pipe_headers.as_deref() {
        if let Some(record) = parse_pipe_record_with_headers(headers, trimmed) {
            return Some(record);
        }
    }
    plugin.parse_cloud_record(line, state.csv_headers.as_deref())
}

#[tracing::instrument(level = "debug", skip_all)]
fn infer_provider_hint_from_pipe_line(line: &str) -> Option<&'static str> {
    if !line.contains('|') {
        return None;
    }
    let lower = line.to_ascii_lowercase();
    if lower.contains("timegenerated") || lower.contains("logmessage") {
        Some("azure")
    } else if lower.contains("__time__") || lower.contains("__source__") {
        Some("aliyun")
    } else if lower.contains("logsetname") || lower.contains("tencent") {
        Some("tencent")
    } else if lower.contains("loggroupname") || lower.contains("log_content") {
        Some("huawei")
    } else if lower.contains("timestamp") && lower.contains("message") {
        Some("cloud")
    } else {
        None
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn apply_provider_hint(record: &mut CloudRecord, provider_hint: Option<&str>) {
    if record.provider != "cloud" {
        return;
    }
    if let Some(provider) = provider_hint {
        record.provider = provider.to_string();
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn append_lines_with_newline(out: &mut String, lines: &[String]) {
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn append_cloud_meta_line(out: &mut String, records: &[CloudRecord]) {
    let mut providers = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for record in records {
        providers.insert(record.provider.as_str());
        if let Some(source) = record.source.as_deref() {
            sources.insert(source);
        }
    }
    if records.len() > 1 {
        let compact_sources = sources
            .iter()
            .take(3)
            .map(|source| compact_cloud_source(source))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(",");
        out.push_str(&format!(
            "$CL|META|providers={}|sources={}|records={}\n",
            providers.iter().copied().collect::<Vec<_>>().join(","),
            compact_sources,
            records.len()
        ));
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn append_cloud_record_messages(out: &mut String, records: &[CloudRecord]) {
    let mut seen_messages = BTreeSet::new();
    for record in records {
        let message = compact_cloud_record_message(&record.message);
        let dedupe_key = format!(
            "{}|{}",
            compact_cloud_time(&record.time),
            normalize_cloud_message_for_dedupe(&message)
        );
        if !seen_messages.insert(dedupe_key) {
            continue;
        }
        if let Some(level) = record.level.as_deref() {
            if is_error_level(level) {
                out.push('!');
            }
        } else if contains_error_signal(&message) {
            out.push('!');
        }
        out.push_str("$CL|REC|");
        if !record.time.is_empty() {
            out.push_str("t=");
            out.push_str(&compact_cloud_time(&record.time));
            out.push('|');
        }
        if let Some(level) = record.level.as_deref() {
            out.push_str("lvl=");
            out.push_str(level);
            out.push('|');
        }
        if let Some(source) = record.source.as_deref() {
            out.push_str("src=");
            out.push_str(&compact_cloud_source(source));
            out.push('|');
        }
        out.push_str("msg=");
        out.push_str(message.trim_end());
        out.push('\n');
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn compact_cloud_record_message(message: &str) -> String {
    let mut grouped_parts: Vec<String> = Vec::new();
    for part in message.split(" | ") {
        let trimmed = part.trim();
        if trimmed.contains('=') || grouped_parts.is_empty() {
            grouped_parts.push(trimmed.to_string());
        } else if let Some(last) = grouped_parts.last_mut() {
            last.push_str(" / ");
            last.push_str(trimmed);
        }
    }

    let mut parts = Vec::new();
    for part in grouped_parts {
        let trimmed = part.trim();
        if let Some((key, value)) = trimmed.split_once('=') {
            parts.push(format!(
                "{key}={}",
                compact_structured_field_value(key, value)
            ));
        } else {
            parts.push(trimmed.to_string());
        }
    }
    parts.join(" | ")
}

#[tracing::instrument(level = "debug", skip_all)]
fn compact_structured_field_value(key: &str, value: &str) -> String {
    if matches!(
        key.trim(),
        "示例" | "sample" | "samples" | "example" | "examples"
    ) {
        return compact_offer_examples(value).unwrap_or_else(|| compact_long_value(value, 240));
    }
    compact_long_value(value, 360)
}

#[tracing::instrument(level = "debug", skip_all)]
fn compact_long_value(value: &str, limit: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    let prefix = trimmed.chars().take(limit).collect::<String>();
    format!("{prefix}<TRUNCATED>")
}

#[tracing::instrument(level = "debug", skip_all)]
fn compact_offer_examples(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if !trimmed.contains("'shop'") && !trimmed.contains("\"shop\"") {
        return None;
    }
    let mut offers = Vec::new();
    for chunk in trimmed.split("},").take(3) {
        let shop = extract_object_field(chunk, "shop").unwrap_or_else(|| "-".to_string());
        let price = extract_object_field(chunk, "price_value")
            .or_else(|| extract_object_field(chunk, "price_text"))
            .unwrap_or_else(|| "-".to_string());
        let currency = extract_object_field(chunk, "currency").unwrap_or_default();
        let pos = extract_object_field(chunk, "pos").unwrap_or_else(|| "-".to_string());
        let eta = extract_object_field(chunk, "delivery_eta").unwrap_or_default();
        let rating = extract_object_field(chunk, "rating").unwrap_or_default();
        let mut summary = format!("pos{pos}:{shop}:{price}{currency}");
        if !eta.is_empty() && eta != "None" && eta != "null" {
            summary.push_str(&format!(" eta={eta}"));
        }
        if !rating.is_empty() && rating != "None" && rating != "null" {
            summary.push_str(&format!(" rating={rating}"));
        }
        offers.push(summary);
    }
    if offers.is_empty() {
        None
    } else {
        Some(format!("[{}]", offers.join(";")))
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn extract_object_field(chunk: &str, key: &str) -> Option<String> {
    let quoted_key = format!("'{key}':");
    let double_key = format!("\"{key}\":");
    let start = chunk
        .find(&quoted_key)
        .map(|idx| idx + quoted_key.len())
        .or_else(|| chunk.find(&double_key).map(|idx| idx + double_key.len()))?;
    let rest = chunk[start..].trim_start();
    if let Some(stripped) = rest.strip_prefix('\'') {
        let end = stripped.find('\'')?;
        return Some(stripped[..end].to_string());
    }
    if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        return Some(stripped[..end].to_string());
    }
    let end = rest
        .find(',')
        .or_else(|| rest.find('}'))
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

#[tracing::instrument(level = "debug", skip_all)]
fn normalize_cloud_message_for_dedupe(message: &str) -> String {
    let mut normalized = message.trim().to_string();
    for _ in 0..3 {
        if normalized.starts_with("[INFO] ") {
            let parts = normalized.splitn(4, ' ').collect::<Vec<_>>();
            if parts.len() == 4 && parts[1].contains('T') && parts[2].contains('-') {
                normalized = parts[3].to_string();
                continue;
            }
        }
        if let Some(idx) = normalized.find("] INFO: ") {
            if normalized.starts_with('[') {
                normalized = normalized[idx + 8..].to_string();
                continue;
            }
        }
        break;
    }
    normalized
}

#[tracing::instrument(level = "debug", skip_all)]
fn render_multiline_csv_records(text: &str) -> Option<String> {
    if !text.contains("Traceback (most recent call last):\n") {
        return None;
    }
    let mut lines = text.lines();
    let header_line = lines.next()?.trim();
    let headers = parse_csv_header(header_line)?;
    let mut rows = Vec::new();
    let mut current = String::new();
    for line in lines {
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
        if csv_quotes_balanced(&current) {
            rows.push(current.clone());
            current.clear();
        }
    }
    if !current.trim().is_empty() {
        rows.push(current);
    }
    let mut records = Vec::new();
    for row in rows {
        if let Some(record) = parse_csv_record(&headers, &row) {
            records.push(record);
        }
    }
    if records.is_empty() {
        return None;
    }
    let mut out = String::new();
    append_cloud_meta_line(&mut out, &records);
    append_cloud_record_messages(&mut out, &records);
    Some(out)
}

#[tracing::instrument(level = "trace", skip_all)]
fn csv_quotes_balanced(value: &str) -> bool {
    let mut in_quotes = false;
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            if in_quotes && chars.peek() == Some(&'"') {
                chars.next();
            } else {
                in_quotes = !in_quotes;
            }
        }
    }
    !in_quotes
}

#[tracing::instrument(level = "debug", skip_all)]
fn collect_strict_access_records(
    plugin: &CloudLogPlugin,
    records: &[CloudRecord],
) -> Option<Vec<AccessRecord>> {
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        out.push(plugin.parse_access_record(record)?);
    }
    Some(out)
}

#[tracing::instrument(level = "debug", skip_all)]
fn group_access_records(access_records: Vec<AccessRecord>) -> Vec<Vec<AccessRecord>> {
    let mut groups: BTreeMap<String, Vec<AccessRecord>> = BTreeMap::new();
    for record in access_records {
        let key = format!(
            "{}|{}|{}|{}|{}|{}",
            record.provider,
            record.source.as_deref().unwrap_or("-"),
            record.method,
            record.path,
            record.status,
            record.reason
        );
        groups.entry(key).or_default().push(record);
    }
    let mut grouped_records = groups.into_values().collect::<Vec<_>>();
    grouped_records.sort_by_key(|records| {
        records
            .first()
            .map(|record| record.time.as_str())
            .unwrap_or_default()
            .to_string()
    });
    grouped_records
}

#[tracing::instrument(level = "debug", skip_all)]
fn format_access_summary_line(records: &[AccessRecord]) -> Option<String> {
    let first = records.first()?;
    let last = records.last().unwrap_or(first);
    let mut ips: BTreeMap<&str, usize> = BTreeMap::new();
    for record in records {
        *ips.entry(record.ip.as_str()).or_insert(0) += 1;
    }
    let ip_summary = ips
        .iter()
        .map(|(ip, count)| format!("{ip}:{count}"))
        .collect::<Vec<_>>()
        .join(",");
    let status_num = first.status.parse::<u16>().unwrap_or(0);
    let prefix = if status_num >= 400 { "!$CL" } else { "$CL" };
    let label = if first.method == "GET"
        && first.path == "/health"
        && first.status == "200"
        && first.reason.eq_ignore_ascii_case("OK")
    {
        "WEB_HEALTH"
    } else {
        "WEB_ACCESS"
    };
    Some(format!(
        "{prefix}|{label}|provider={}|{}..{}|source={}|{} {}|{} {}|hits={}|ips={}|level={}\n",
        first.provider,
        first.time,
        last.time,
        first.source.as_deref().unwrap_or("-"),
        first.method,
        first.path,
        first.status,
        first.reason,
        records.len(),
        ip_summary,
        first.level
    ))
}

#[tracing::instrument(level = "debug", skip_all)]
fn looks_like_pipe_table_time(cell: &str) -> bool {
    (cell.chars().all(|c| c.is_ascii_digit()) && cell.len() >= 10)
        || (cell.contains('T') && cell.contains(':'))
}

#[tracing::instrument(level = "debug", skip_all)]
fn is_pipe_table_level(cell: &str) -> bool {
    matches!(
        cell.to_ascii_uppercase().as_str(),
        "INFO" | "WARN" | "WARNING" | "ERROR" | "DEBUG" | "TRACE" | "CRITICAL"
    )
}

#[tracing::instrument(level = "debug", skip_all)]
fn parse_pipe_header(line: &str) -> Option<Vec<String>> {
    let cells = split_pipe_cells(line)?;
    let lower = cells
        .iter()
        .map(|cell| cell.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let has_time = lower.iter().any(|cell| {
        matches!(
            cell.as_str(),
            "timestamp" | "time" | "timegenerated" | "__time__" | "@timestamp" | "datetime"
        )
    });
    let has_message = lower.iter().any(|cell| {
        matches!(
            cell.as_str(),
            "message"
                | "@message"
                | "msg"
                | "log"
                | "content"
                | "textpayload"
                | "logmessage"
                | "logcontent"
                | "log_content"
        )
    });
    if has_time && has_message {
        Some(cells)
    } else {
        None
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn parse_pipe_record_with_headers(headers: &[String], line: &str) -> Option<CloudRecord> {
    let cells = split_pipe_cells(line)?;
    if cells.len() < headers.len().min(2) {
        return None;
    }

    let message = pick_field_by_header_names(
        headers,
        &cells,
        &[
            "message",
            "@message",
            "msg",
            "log",
            "content",
            "textPayload",
            "LogMessage",
            "logContent",
            "log_content",
            "Content",
        ],
    )?;

    Some(CloudRecord {
        provider: infer_provider_from_headers(headers).unwrap_or_else(|| infer_provider(line)),
        time: pick_field_by_header_names(
            headers,
            &cells,
            &[
                "timestamp",
                "time",
                "TimeGenerated",
                "__time__",
                "@timestamp",
                "datetime",
            ],
        )
        .unwrap_or_default(),
        source: pick_field_by_header_names(
            headers,
            &cells,
            &[
                "logStream",
                "@logStream",
                "logStreamName",
                "resourceId",
                "resource",
                "Category",
                "source",
                "__source__",
                "resourceName",
                "topic",
                "logsetName",
                "logGroupName",
                "containerName",
                "cloud_RoleName",
                "scriptName",
                "rayID",
                "RayID",
            ],
        ),
        level: pick_field_by_header_names(
            headers,
            &cells,
            &[
                "level",
                "severity",
                "SeverityLevel",
                "LogLevel",
                "type",
                "status",
                "levelName",
            ],
        ),
        message: merge_structured_message_fields(headers, &cells, &message),
    })
}

#[tracing::instrument(level = "debug", skip_all)]
fn pick_field_by_header_names(
    headers: &[String],
    values: &[String],
    names: &[&str],
) -> Option<String> {
    for name in names {
        if let Some((idx, _)) = headers
            .iter()
            .enumerate()
            .find(|(_, header)| header.eq_ignore_ascii_case(name))
        {
            if let Some(value) = values.get(idx) {
                if !value.trim().is_empty() {
                    return Some(value.trim().to_string());
                }
            }
        }
    }
    None
}

#[tracing::instrument(level = "debug", skip_all)]
fn merge_structured_message_fields(
    headers: &[String],
    values: &[String],
    base_message: &str,
) -> String {
    let mut parts = vec![base_message.trim().to_string()];
    let mut extra_parts: Vec<String> = Vec::new();
    for (idx, value) in values.iter().enumerate() {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed == base_message.trim() {
            continue;
        }
        let header = headers.get(idx).map(|field| field.as_str()).unwrap_or("");
        if is_cloud_envelope_header(header) {
            continue;
        }
        if header.is_empty() {
            if trimmed.contains('=') || extra_parts.is_empty() {
                extra_parts.push(trimmed.to_string());
            } else if let Some(last) = extra_parts.last_mut() {
                last.push_str(" / ");
                last.push_str(trimmed);
            }
        } else {
            extra_parts.push(format!("{header}={trimmed}"));
        }
    }
    parts.extend(extra_parts);
    parts.join(" | ")
}

#[tracing::instrument(level = "trace", skip_all)]
fn is_cloud_envelope_header(header: &str) -> bool {
    matches!(
        header.to_ascii_lowercase().as_str(),
        "timestamp"
            | "time"
            | "timegenerated"
            | "__time__"
            | "@timestamp"
            | "datetime"
            | "message"
            | "@message"
            | "msg"
            | "log"
            | "content"
            | "textpayload"
            | "logmessage"
            | "logcontent"
            | "log_content"
            | "logstream"
            | "@logstream"
            | "logstreamname"
            | "resourceid"
            | "resource"
            | "category"
            | "source"
            | "__source__"
            | "resourcename"
            | "topic"
            | "logsetname"
            | "loggroupname"
            | "containername"
            | "cloud_rolename"
            | "scriptname"
            | "rayid"
            | "level"
            | "severity"
            | "severitylevel"
            | "loglevel"
            | "type"
            | "status"
            | "levelname"
    )
}

#[tracing::instrument(level = "debug", skip_all)]
fn split_pipe_cells(line: &str) -> Option<Vec<String>> {
    if !line.starts_with('|') || !line.ends_with('|') {
        return None;
    }
    let cells = line
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .filter(|cell| !cell.is_empty())
        .collect::<Vec<_>>();
    if cells.is_empty() {
        None
    } else {
        Some(cells)
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn is_table_noise(line: &str) -> bool {
    if line.is_empty() {
        return true;
    }
    let compact = line.trim();
    if compact.chars().all(|c| c == '-' || c == '+') {
        return true;
    }
    if compact.starts_with("|---") || compact.starts_with("|===") {
        return true;
    }
    let lower = compact.to_ascii_lowercase();
    lower.contains('|')
        && (lower.contains("timestamp")
            || lower.contains("timegenerated")
            || lower.contains("message")
            || lower.contains("logmessage"))
}

#[tracing::instrument(level = "debug", skip_all)]
fn is_cloud_command_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("aws ") && lower.contains(" logs ")
        || lower.contains("gcloud logging")
        || lower.contains("az monitor")
        || lower.starts_with("aliyun ")
        || lower.contains(" aliyun ")
        || lower.contains("oci logging")
        || lower.contains("tccli cls")
        || lower.contains("hcloud lts")
        || lower.contains("wrangler tail")
}

#[tracing::instrument(level = "debug", skip_all)]
fn looks_like_cloud_source(value: &str) -> bool {
    value.contains('/')
        || value.contains("projects/")
        || value.contains("subscriptions/")
        || value.contains("Microsoft.")
        || value.contains("ecs")
        || value.contains("app")
        || value.contains("ocid1.")
        || value.contains("tencent")
        || value.contains("cls/")
        || value.contains("lts/")
        || value.contains("cloudflare")
        || value.contains("workers")
}

#[tracing::instrument(level = "debug", skip_all)]
fn looks_like_inner_log(value: &str) -> bool {
    let v = value.trim();
    v.contains("INFO:")
        || v.contains("ERROR:")
        || v.contains("WARNING:")
        || v.contains("Traceback")
        || v.contains("Exception")
        || v.contains(" at ")
        || v.contains("GET ")
        || v.contains("POST ")
        || v.contains("SELECT ")
        || v.contains("duration:")
        || v.contains("Connection accepted")
        || v.contains("client disconnected")
        || v.contains("ERROR")
        || v.contains("WARN")
}

#[tracing::instrument(level = "debug", skip_all)]
fn infer_provider(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    if lower.contains("huaweicloud")
        || lower.contains("hcloud")
        || lower.contains("lts/")
        || lower.contains("log_content")
        || lower.contains("loggroupname")
    {
        "huawei".to_string()
    } else if lower.contains("tencentcloud")
        || lower.contains("tccli")
        || lower.contains("cls/")
        || lower.contains("logsetname")
    {
        "tencent".to_string()
    } else if lower.contains("cloudflare")
        || lower.contains("wrangler")
        || lower.contains("rayid")
        || lower.contains("scriptname")
    {
        "cloudflare".to_string()
    } else if lower.contains("oci") || lower.contains("ocid1.") || lower.contains("oraclecloud") {
        "oci".to_string()
    } else if lower.contains("loggroup") || lower.contains("logstream") || lower.contains("ecs/") {
        "aws".to_string()
    } else if lower.contains("textpayload")
        || lower.contains("gcloud")
        || lower.contains("logname")
        || lower.contains("googleapis.com")
        || lower.contains("projects/")
    {
        "gcp".to_string()
    } else if lower.contains("timegenerated")
        || lower.contains("microsoft.")
        || lower.contains("resourceid")
    {
        "azure".to_string()
    } else if lower.contains("__time__") || lower.contains("__source__") || lower.contains("aliyun")
    {
        "aliyun".to_string()
    } else {
        "cloud".to_string()
    }
}

#[tracing::instrument(level = "debug", skip_all)]
fn compact_cloud_time(value: &str) -> String {
    let trimmed = value.trim();
    if let Some((date, rest)) = trimmed.split_once('T') {
        let time = rest.split(['.', '+', 'Z']).next().unwrap_or(rest);
        if !time.is_empty() {
            return format!("{date} {time}");
        }
    }
    trimmed.to_string()
}

#[tracing::instrument(level = "debug", skip_all)]
fn compact_cloud_source(value: &str) -> String {
    let parts = value.split('/').collect::<Vec<_>>();
    if parts.len() >= 3 {
        let tail = parts[parts.len() - 1];
        let short_tail = tail.get(..8).unwrap_or(tail);
        return format!("{}/{}/{}", parts[0], parts[1], short_tail);
    }
    value.to_string()
}

#[tracing::instrument(level = "debug", skip_all)]
fn compact_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[tracing::instrument(level = "debug", skip_all)]
fn is_error_level(level: &str) -> bool {
    matches!(
        level.to_ascii_uppercase().as_str(),
        "ERROR" | "CRITICAL" | "FATAL" | "SEVERE"
    )
}

#[tracing::instrument(level = "debug", skip_all)]
fn contains_error_signal(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("error")
        || lower.contains("exception")
        || lower.contains("traceback")
        || lower.contains("fatal")
        || lower.contains("panic")
}
