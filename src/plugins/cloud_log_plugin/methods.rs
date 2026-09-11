use super::types::CloudLogPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, DocumentSkin, Plugin};
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
    /// 创建 CloudLogPlugin 实例（名称 cloud_log，优先级 41），预编译 5 个解析正则。
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

    /// 用 uvicorn 访问日志正则解析 CloudRecord 为 AccessRecord（含 IP/方法/路径/状态/原因）。
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

    /// 将单行解析为 CloudRecord：过滤空行/表格噪音/命令行，先尝试结构化解析（JSON/CSV/管道表）再文本模式。
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

    /// 结构化解析：依次尝试 JSON 行 → CSV 行（有表头时）→ 管道表格行。
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

    /// 文本模式解析：依次匹配通用云行/aws tail/aws lambda/标准括号四种正则。
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

    /// 逐行收集云记录：更新状态（命令行/表头/provider hint），可解析行入 records，其余入 passthrough。
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

    /// 压缩主入口：优先渲染多行 CSV 记录；否则收集记录并渲染访问摘要/元数据/记录行。
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

    /// 尝试渲染访问汇总：所有记录均可解析为访问记录且至少 2 条时，分组输出汇总行。
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
    /// Default 实现：等价于 new()。
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for CloudLogPlugin {
    /// 返回插件名称 "cloud_log"。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 解包：剥离云日志外壳，仅保留命令行、记录消息与 passthrough 行。
    /// L1 第二阶段（2026-08-19）：与 detect 同源云外壳前置——无云平台/云日志外壳信号时
    /// 不得剥离（unwrap_recursive 会先于 dispatch 对任意文本调用 unwrap，否则普通
    /// timestamp/level 日志会被 generic 解析裁剪，T-004 Group A k8s case_003 根因）。
    fn unwrap(&self, text: &str) -> Option<String> {
        if !has_cloud_shell_signal(text) {
            return None;
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

    /// 文档级剥皮（P1-06）：把云日志外壳（命令行/云平台信号）与内层记录消息分离。
    ///
    /// - 外壳摘要：命令行（含首行命令锚点，法则 0）+ 云记录元数据行——与
    ///   [`Self::unwrap`] 同源的 `collect_cloud_records` 收集；
    /// - 内层正文：记录消息 + passthrough 行，交内层管线重新切片定向压缩；
    /// - 无云外壳信号或无记录（非云日志文档）返回 `None`，回退现状路径。
    fn peel_document(&self, text: &str) -> Option<DocumentSkin> {
        if !has_cloud_shell_signal(text) {
            return None;
        }
        let CloudCollectOutput {
            command_lines,
            records,
            passthrough,
        } = self.collect_cloud_records(text);

        if records.is_empty() {
            return None;
        }

        let mut summary = String::new();
        for line in &command_lines {
            summary.push_str(line);
            summary.push('\n');
        }
        append_cloud_meta_line(&mut summary, &records);

        let mut inner_lines: Vec<&str> = records.iter().map(|r| r.message.trim_end()).collect();
        inner_lines.extend(passthrough.iter().map(|s| s.as_str()));

        Some(DocumentSkin {
            summary,
            inner_body: inner_lines.join("\n"),
        })
    }

    /// 返回插件优先级 41。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 多维度打分检测（命令提示/结构化字段/表格 CSV/解析比例），总分 ≥0.35 时命中。
    /// L1 第二阶段（2026-08-19）：云外壳前置条件——无任何云平台/云日志外壳信号时，
    /// generic_cloud_line_pattern 的行解析不得单独触发命中，防止普通 timestamp/level
    /// 日志被 CloudLogPlugin 抢占（T-004 Group A k8s case_003 根因）。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();

        // 云外壳前置门：命令提示 / 结构化字段 / 表格表头 / 行内云 provider 信号，
        // 至少存在其一才允许继续打分（generic 行解析不再独立构成命中依据）。
        if !has_cloud_shell_signal(text) {
            return None;
        }

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

    /// 压缩切片：压缩云记录并做 ROI 门控，无收益时回退原文。
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

    /// 解压：原文透传（云日志解压无需字典还原）。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    /// 返回后续插件列表（当前为空）。
    fn next_plugins(&self) -> Vec<&'static str> {
        vec![]
    }
}

/// 命令提示打分：含云 CLI 日志命令（aws logs tail 等）得 0.45 分。
#[tracing::instrument(level = "debug", skip_all)]
/// 云外壳信号判定（L1 第二阶段 2026-08-19）：文本中是否存在可验证的云平台/云日志外壳信号。
/// 无任何云信号时，generic_cloud_line_pattern 解析的行不得参与检测——普通 timestamp/level
/// 日志（如 kubectl logs、系统日志）不得被 CloudLogPlugin 抢占。
/// 信号源：① 云 CLI 命令（含 `aws --profile … logs tail` 变体）/ 结构化云字段 / 表格表头；
/// ② 行内云 provider 关键词；③ JSON 行结构化解析成功且能推断具体云 provider（非通用）。
#[tracing::instrument(level = "debug", skip_all)]
fn has_cloud_shell_signal(text: &str) -> bool {
    if detect_command_hint_score(text) > 0.0
        || detect_structured_field_hint_score(text) > 0.0
        || detect_table_or_csv_hint_score(text) > 0.0
        || text.contains("logs tail")
    {
        return true;
    }
    // 行内云 provider 信号（与 declared_patterns / infer_provider_hint_from_pipe_line 一致）
    let lower = text.to_ascii_lowercase();
    if lower.contains("timegenerated")
        || lower.contains("logmessage")
        || lower.contains("__time__")
        || lower.contains("__source__")
        || lower.contains("logsetname")
        || lower.contains("loggroupname")
        || lower.contains("log_content")
        || lower.contains("logstream")
        || lower.contains("ocid1.")
        || lower.contains("oraclecloud")
        || (lower.contains("timestamp") && lower.contains("message"))
    {
        return true;
    }
    // JSON 行：结构化解析成功且推断出具体云 provider（非通用 "cloud"/空）→ 云信号
    for line in text.lines().take(12) {
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            if let Some(rec) = parse_json_record(trimmed) {
                if !rec.provider.is_empty() && rec.provider != "cloud" {
                    return true;
                }
            }
        }
    }
    false
}

/// 命令提示打分：云 CLI 命令（aws logs tail/gcloud logging/az monitor 等）得 0.45 分。
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

/// 结构化字段提示打分：含 textPayload/jsonPayload/logGroup 等云日志键得 0.45 分。
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

/// 表格/CSV 提示打分：含时间/消息列头（| timestamp | 或 timestamp,）时加分。
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

/// 解析比例打分：统计前 12 行中可解析比例 × 0.5。
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

/// 将 JSON 行解析为 CloudRecord：提取 message/time/source/level 并推断 provider。
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

/// 按候选路径列表从 JSON 提取消息字段，失败时尝试合成 HTTP 访问消息。
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

/// 按候选路径列表从 JSON 提取时间字段。
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

/// 按候选路径列表从 JSON 提取来源字段（logStream/logName/resource 等）。
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

/// 按候选路径列表从 JSON 提取级别字段（level/severity/status 等）。
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

/// 依据 JSON 键特征推断云提供商（aws/gcp/azure/aliyun/tencent/huawei/cloudflare/oci）。
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

/// 依次沿候选路径取值，返回首个字符串/数字/布尔值。
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

/// 尝试从 httpRequest/jsonPayload/根对象三个位置合成 HTTP 访问日志消息。
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

/// 从指定对象路径提取 method/path/status/ip 字段，合成访问日志格式消息。
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

/// 沿路径逐键取 JSON 值，任一键缺失返回 None。
#[tracing::instrument(level = "debug", skip_all)]
fn json_at_path<'a>(json: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = json;
    for key in path {
        cur = cur.get(*key)?;
    }
    Some(cur)
}

/// 从对象中按字段顺序取首个非空字符串/数字/布尔值。
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

/// 规范化访问路径：完整 URL 仅保留 path 部分，其余原样返回。
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

/// 解析 CSV 表头：必须同时含时间列与消息列才识别为表头。
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

/// 依据表头字段推断云提供商（__time__→aliyun、logsetname→tencent 等）。
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

/// 按表头解析 CSV 数据行：提取 message/time/source/level 并合并消息字段。
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

/// 拆分 CSV 行，支持双引号包裹与 "" 转义引号。
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

/// 解析管道表格行（须以 | 开头和结尾），提取时间/级别/消息。
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

/// 更新收集状态：命令行入列、CSV/管道表头记录、provider hint 更新；命中状态行返回 true。
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

/// 尝试从行收集记录：优先按管道表头解析，否则走通用云记录解析。
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

/// 从含 | 的行推断 provider hint（timegenerated→azure、__time__→aliyun 等）。
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

/// 对 provider 为 "cloud" 的通用记录应用 hint 覆盖为具体提供商。
#[tracing::instrument(level = "debug", skip_all)]
fn apply_provider_hint(record: &mut CloudRecord, provider_hint: Option<&str>) {
    if record.provider != "cloud" {
        return;
    }
    if let Some(provider) = provider_hint {
        record.provider = provider.to_string();
    }
}

/// 将行列表逐行追加到输出（每行带换行）。
#[tracing::instrument(level = "debug", skip_all)]
fn append_lines_with_newline(out: &mut String, lines: &[String]) {
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
}

/// 追加 $CL|META 汇总行（providers/sources/records 数，仅多记录时输出）。
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

/// 追加 $CL|REC 记录行：按时间+归一化消息去重，错误级别加 ! 前缀。
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

/// 紧凑化记录消息：按 " | " 分组，key=value 形式的值做结构化压缩。
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

/// 按 key 压缩字段值：示例类字段走 offer 压缩，其余按长度截断。
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

/// 超长值截断：取前 limit 个字符并追加 <TRUNCATED> 标记。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_long_value(value: &str, limit: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    let prefix = trimmed.chars().take(limit).collect::<String>();
    format!("{prefix}<TRUNCATED>")
}

/// 从示例文本提取前 3 个 offer 摘要（shop/price/pos/eta/rating 字段）。
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

/// 从字典片段中提取指定 key 的值（支持单引号/双引号/无引号三种形态）。
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

/// 去重归一化：剥离 [INFO] 时间戳等前缀，得到稳定的消息指纹。
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

/// 渲染多行 CSV 记录：按引号平衡合并跨行 traceback，解析后输出元数据与记录行。
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

/// 检查 CSV 引号是否成对平衡（正确处理 "" 转义）。
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

/// 严格收集访问记录：任一记录无法解析为访问记录时整体返回 None。
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

/// 按 provider/source/method/path/status/reason 分组访问记录，组内按时间排序。
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

/// 格式化访问汇总行：含时间范围、IP 分布、hits 数，4xx 加 ! 前缀，健康检查标 WEB_HEALTH。
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

/// 判断单元格是否形如时间（纯数字且 ≥10 位，或含 T 与冒号）。
#[tracing::instrument(level = "debug", skip_all)]
fn looks_like_pipe_table_time(cell: &str) -> bool {
    (cell.chars().all(|c| c.is_ascii_digit()) && cell.len() >= 10)
        || (cell.contains('T') && cell.contains(':'))
}

/// 判断单元格是否为日志级别（INFO/WARN/ERROR/DEBUG/TRACE/CRITICAL）。
#[tracing::instrument(level = "debug", skip_all)]
fn is_pipe_table_level(cell: &str) -> bool {
    matches!(
        cell.to_ascii_uppercase().as_str(),
        "INFO" | "WARN" | "WARNING" | "ERROR" | "DEBUG" | "TRACE" | "CRITICAL"
    )
}

/// 解析管道表头：须同时含时间列与消息列。
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

/// 按表头解析管道记录行，提取 message/time/source/level 并合并消息字段。
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

/// 按候选列名在表头中定位列索引并取值（大小写不敏感，空值跳过）。
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

/// 合并结构化消息字段：base 消息 + 非信封字段转为 key=value 追加。
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

/// 判断表头是否为云日志信封字段（时间/消息/来源/级别等，不并入消息体）。
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

/// 拆分管道表格单元格（须以 | 开头和结尾，过滤空单元格）。
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

/// 判断是否为表格噪音行（空行、分隔线、重复的表头行）。
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

/// 判断是否为云 CLI 命令行（aws logs/gcloud logging/az monitor 等）。
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

/// 判断来源值是否形如云资源（含 /、projects/、Microsoft.、ecs 等）。
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

/// 判断消息是否形如内部日志（INFO:/ERROR:/Traceback/GET/SELECT 等）。
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

/// 依据文本特征推断云提供商（huawei/tencent/cloudflare/oci/aws/gcp/azure/aliyun/cloud）。
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

/// 紧凑化时间：ISO 8601 T 分隔转空格，剥离小数秒与时区后缀。
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

/// 紧凑化来源：路径截断保留前两段与尾段前 8 字符。
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

/// 将连续空白压缩为单个空格。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 判断级别是否为错误级（ERROR/CRITICAL/FATAL/SEVERE）。
#[tracing::instrument(level = "debug", skip_all)]
fn is_error_level(level: &str) -> bool {
    matches!(
        level.to_ascii_uppercase().as_str(),
        "ERROR" | "CRITICAL" | "FATAL" | "SEVERE"
    )
}

/// 判断消息是否含错误信号（error/exception/traceback/fatal/panic）。
#[tracing::instrument(level = "debug", skip_all)]
fn contains_error_signal(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("error")
        || lower.contains("exception")
        || lower.contains("traceback")
        || lower.contains("fatal")
        || lower.contains("panic")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 契约测试：`has_cloud_shell_signal` 识别云 CLI 命令（logs tail）与行内云 provider 关键词，
    /// 对普通纯文本返回 false（不误判非云日志）。
    #[test]
    fn has_cloud_shell_signal_detects_cloud_markers_only() {
        // 云 CLI 命令变体应命中。
        assert!(has_cloud_shell_signal(
            "aws logs tail /service/prod --follow"
        ));
        // 行内云 provider 关键词应命中（logstream）。
        assert!(
            has_cloud_shell_signal("2026-05-21T10:00:00Z INFO api logstream=abc order placed"),
            "含 logstream 关键词的日志应判定为云信号"
        );
        // JSON 行含云特征字段（loggroupname）应命中。
        assert!(
            has_cloud_shell_signal("{\"logGroupName\":\"/aws/lambda/fn\",\"message\":\"ok\"}"),
            "含 loggroupname 的 JSON 行应判定为云信号"
        );
        // 普通文本无任何云特征，应返回 false。
        assert!(
            !has_cloud_shell_signal("request completed in 200ms for user 42"),
            "普通日志不得误判为云信号"
        );
    }

    /// 契约测试：`parse_pipe_table_record` 解析以 | 包裹的表格行为 CloudRecord，
    /// 对非管道行与单元格不足的行返回 None。
    #[test]
    fn parse_pipe_table_record_extracts_time_level_message() {
        let rec = parse_pipe_table_record("| 2026-05-21T10:00:00Z | ERROR | order failed |")
            .expect("合法管道行应解析成功");
        assert_eq!(rec.time, "2026-05-21T10:00:00Z", "应提取时间列");
        assert_eq!(rec.level.as_deref(), Some("ERROR"), "应提取级别列");
        assert_eq!(rec.message, "order failed", "末列应为消息");

        // 非管道包裹的行返回 None
        assert!(parse_pipe_table_record("ERROR: order failed").is_none());
        // 单元格不足 2 个返回 None
        assert!(parse_pipe_table_record("| only |").is_none());
    }

    /// 契约测试：`merge_structured_message_fields` 保留基础消息、跳过云信封字段与空值/重复值，
    /// 非信封字段按 key=value 追加并以 ` | ` 连接。
    #[test]
    fn merge_structured_message_fields_skips_envelope_and_appends_extra() {
        let headers: Vec<String> = vec![
            "timestamp".into(),
            "message".into(),
            "trace_id".into(),
            "job_id".into(),
        ];
        let values: Vec<String> = vec![
            "ignored".into(),
            "request received".into(),
            "9f2c".into(),
            "a1b2".into(),
        ];
        let out = merge_structured_message_fields(&headers, &values, " request received ");
        assert_eq!(
            out, "request received | trace_id=9f2c | job_id=a1b2",
            "信封字段 timestamp/message 应跳过，trace_id/job_id 以 key=value 追加"
        );

        // 空值应被丢弃，仅保留基础消息。
        let empty_headers: Vec<String> = Vec::new();
        let empty_values: Vec<String> = vec![String::from("   ")];
        assert_eq!(
            merge_structured_message_fields(&empty_headers, &empty_values, "base"),
            "base"
        );
    }

    /// 契约测试：`render_multiline_csv_records` 仅在出现 traceback 标记时进入多行合并，
    /// 解析 CSV 头并输出元数据与记录消息；无标记时返回 None。
    #[test]
    fn render_multiline_csv_records_requires_traceback_marker() {
        // 无 traceback 标记返回 None。
        assert!(
            render_multiline_csv_records("timestamp,message\n2026-05-21T10:00:00Z,ok\n").is_none(),
            "缺少 traceback 标记应返回 None"
        );

        // 含 traceback 标记：表头 + 两行有效记录 + 尾部 traceback 块。
        let text = "timestamp,message,level\n\
                    2026-05-21T10:00:00Z,ok one,INFO\n\
                    2026-05-21T10:00:00Z,ok two,WARN\n\
                    Traceback (most recent call last):\n\
                      File \"/app/f.py\", line 10, in <module>\n\
                    RuntimeError: boom\n";
        let out = render_multiline_csv_records(text).expect("含 traceback 的 CSV 应输出");
        assert!(out.contains("$CL|META|"), "应输出元数据行: {out}");
        assert!(out.contains("ok one"), "第一条记录消息应保留: {out}");
        assert!(out.contains("ok two"), "第二条记录消息应保留: {out}");
    }

    /// 契约测试：`compress_cloud_records` 对无记录输入返回 None，对云记录输出元数据与消息行。
    #[test]
    fn compress_cloud_records_renders_meta_and_messages() {
        let plugin = CloudLogPlugin::new();
        // 空输入无记录 → None。
        assert!(plugin.compress_cloud_records("").is_none());
        assert!(plugin
            .compress_cloud_records("just a plain line without cloud markers")
            .is_none());

        // 两条结构化云日志 → 渲染出元数据行并保留消息。
        let text = "2026-05-21T10:00:00.123Z INFO svc a worker started\n\
                    2026-05-21T10:00:00.456Z WARN svc b retry in 3s\n";
        let out = plugin
            .compress_cloud_records(text)
            .expect("云记录应渲染成功");
        assert!(out.contains("$CL|META|providers="), "应输出元数据行: {out}");
        assert!(out.contains("a worker started"), "第一条消息应保留: {out}");
        assert!(out.contains("retry in 3s"), "第二条消息应保留: {out}");
    }
}
