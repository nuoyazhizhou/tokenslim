//! SARIF and JUnit artifact summary plugin implementation.

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use crate::plugins::infra_tools_common::{
    compact_spaces, decompress_with_dict, fallback_if_anchor_only, keep_error_signal, push_anchor,
};
use bumpalo::Bump;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde_json::Value;
use std::borrow::Cow;

impl ArtifactSummaryPlugin {
    /// 创建 ArtifactSummaryPlugin 实例（名称 artifact_summary，优先级 55）。
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn new() -> Self {
        Self {
            name: "artifact_summary",
            priority: 55,
        }
    }
}

impl Plugin for ArtifactSummaryPlugin {
    /// 返回插件名称 "artifact_summary"。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回插件优先级 55。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测切片内容是否为 JUnit XML 或 SARIF JSON 构件，命中返回 1.0 置信度。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.trim();
        let lower = text.to_ascii_lowercase();
        if looks_like_junit(&lower) || looks_like_sarif(&lower) {
            return Some(1.0);
        }
        None
    }

    /// 压缩切片：先剥离 ANSI 码，再压缩为 JUnit/SARIF 摘要，保留错误信号并做 ROI 门控。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let raw = slice.text.as_ref();
        let cleaned = crate::core::utils::strip_ansi(raw);
        let compacted =
            keep_error_signal(&cleaned, compact_artifact_summary(&cleaned, dict_engine));
        let final_text = crate::core::utils::roi::prefer_non_expanding(raw, compacted);
        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(final_text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 解压：将压缩文本中的字典 token 用词典还原。
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        decompress_with_dict(compressed, dict)
    }
}

/// 核心压缩逻辑：按内容类型（JUnit/SARIF）解析并渲染摘要行，压缩无收益时回退原文。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_artifact_summary(text: &str, dict_engine: &mut DictionaryEngine) -> String {
    let lower = text.to_ascii_lowercase();
    let mut lines = Vec::new();
    push_anchor(&mut lines, text);

    if looks_like_junit(&lower) {
        if let Some(summary) = parse_junit(text) {
            render_junit_summary(&mut lines, summary, dict_engine);
        }
    } else if looks_like_sarif(&lower) {
        if let Some(summary) = parse_sarif(text, dict_engine) {
            render_sarif_summary(&mut lines, summary);
        }
    }

    fallback_if_anchor_only(lines, text)
}

/// 判断小写文本是否形如 JUnit XML（含 testsuite(s) 与 testcase 标签）。
#[tracing::instrument(level = "debug", skip_all)]
fn looks_like_junit(lower: &str) -> bool {
    (lower.contains("<testsuite") || lower.contains("<testsuites")) && lower.contains("<testcase")
}

/// 判断小写文本是否形如 SARIF JSON（含 runs/tool/results 关键键）。
#[tracing::instrument(level = "debug", skip_all)]
fn looks_like_sarif(lower: &str) -> bool {
    lower.contains("\"runs\"")
        && lower.contains("\"tool\"")
        && lower.contains("\"results\"")
        && (lower.contains("sarif") || lower.contains("\"ruleid\""))
}

/// 用 quick_xml 流式解析 JUnit XML，汇总 suite/test/failure/error/skipped 计数，
/// 收集失败用例详情与关键属性；解析失败或无用例时返回 None。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_junit(text: &str) -> Option<JunitSummary> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);

    let mut summary = JunitSummary::default();
    let mut current_suite = String::new();
    let mut current_case: Option<JunitCase> = None;
    let mut counted_testcases = 0usize;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"testsuite" => {
                    current_suite = xml_attr(&reader, &e, b"name").unwrap_or_default();
                    summary.suites += 1;
                    summary.tests += attr_usize(&reader, &e, b"tests");
                    summary.failures += attr_usize(&reader, &e, b"failures");
                    summary.errors += attr_usize(&reader, &e, b"errors");
                    summary.skipped += attr_usize(&reader, &e, b"skipped");
                    summary.time += attr_f64(&reader, &e, b"time");
                    if let Some(hostname) = xml_attr(&reader, &e, b"hostname") {
                        summary.properties.push(("hostname".to_string(), hostname));
                    }
                }
                b"testcase" => {
                    counted_testcases += 1;
                    current_case = Some(JunitCase {
                        suite: current_suite.clone(),
                        name: xml_attr(&reader, &e, b"name")
                            .unwrap_or_else(|| "unknown".to_string()),
                        class_name: xml_attr(&reader, &e, b"classname").unwrap_or_default(),
                        status: JunitStatus::Pass,
                        message: String::new(),
                        time: attr_f64(&reader, &e, b"time"),
                    });
                }
                b"failure" | b"error" | b"skipped" => {
                    if let Some(case) = current_case.as_mut() {
                        case.status = match e.name().as_ref() {
                            b"failure" => JunitStatus::Failure,
                            b"error" => JunitStatus::Error,
                            _ => JunitStatus::Skipped,
                        };
                        case.message = xml_attr(&reader, &e, b"message")
                            .or_else(|| xml_attr(&reader, &e, b"type"))
                            .unwrap_or_default();
                    }
                }
                b"property" => {
                    capture_junit_property(&reader, &e, &mut summary);
                }
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"testsuite" => {
                    summary.suites += 1;
                    summary.tests += attr_usize(&reader, &e, b"tests");
                    summary.failures += attr_usize(&reader, &e, b"failures");
                    summary.errors += attr_usize(&reader, &e, b"errors");
                    summary.skipped += attr_usize(&reader, &e, b"skipped");
                    summary.time += attr_f64(&reader, &e, b"time");
                }
                b"testcase" => {
                    counted_testcases += 1;
                }
                b"failure" | b"error" | b"skipped" => {
                    if let Some(case) = current_case.as_mut() {
                        case.status = match e.name().as_ref() {
                            b"failure" => JunitStatus::Failure,
                            b"error" => JunitStatus::Error,
                            _ => JunitStatus::Skipped,
                        };
                        case.message = xml_attr(&reader, &e, b"message")
                            .or_else(|| xml_attr(&reader, &e, b"type"))
                            .unwrap_or_default();
                    }
                }
                b"property" => {
                    capture_junit_property(&reader, &e, &mut summary);
                }
                _ => {}
            },
            Ok(Event::Text(e)) => {
                if let Some(case) = current_case.as_mut() {
                    if case.status != JunitStatus::Pass {
                        let text = String::from_utf8_lossy(e.as_ref());
                        if !text.trim().is_empty() {
                            if !case.message.is_empty() {
                                case.message.push(' ');
                            }
                            case.message.push_str(text.trim());
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"testcase" {
                    if let Some(case) = current_case.take() {
                        if case.status != JunitStatus::Pass {
                            summary.cases.push(case);
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return None,
            _ => {}
        }
    }

    if summary.tests == 0 {
        summary.tests = counted_testcases;
    }
    if summary.suites == 0 && summary.tests > 0 {
        summary.suites = 1;
    }
    (summary.tests > 0 || !summary.cases.is_empty()).then_some(summary)
}

/// 用 serde_json 解析 SARIF JSON，统计 run 数、各级别结果数并提取前 12 条 findings。
#[tracing::instrument(level = "debug", skip_all)]
fn parse_sarif(text: &str, _dict_engine: &mut DictionaryEngine) -> Option<SarifSummary> {
    let root: Value = serde_json::from_str(text).ok()?;
    let runs = root.get("runs")?.as_array()?;
    let mut summary = SarifSummary {
        runs: runs.len(),
        ..SarifSummary::default()
    };

    for run in runs {
        if let Some(tool) = run
            .pointer("/tool/driver/name")
            .and_then(Value::as_str)
            .map(str::to_string)
        {
            if !summary.tools.contains(&tool) {
                summary.tools.push(tool);
            }
        }

        // 解析 executions/invocations 执行状态：成功/失败/失败命令，
        // 用于区分干净扫描、未执行扫描与执行失败但未产出结果（SAP-0058）。
        if let Some(invocations) = run.get("invocations").and_then(Value::as_array) {
            for invocation in invocations {
                let ok = invocation
                    .get("executionSuccessful")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                if ok {
                    summary.exec_ok += 1;
                } else {
                    summary.exec_failed += 1;
                    if summary.failed_cmds.len() < 3 {
                        if let Some(cmd) = invocation.get("commandLine").and_then(Value::as_str) {
                            summary.failed_cmds.push(cmd.to_string());
                        }
                    }
                }
            }
        }

        let Some(results) = run.get("results").and_then(Value::as_array) else {
            continue;
        };
        for result in results {
            summary.results += 1;
            let level = result
                .get("level")
                .and_then(Value::as_str)
                .unwrap_or("warning")
                .to_ascii_lowercase();
            match level.as_str() {
                "error" => summary.errors += 1,
                "warning" => summary.warnings += 1,
                "note" => summary.notes += 1,
                "none" => summary.none += 1,
                _ => summary.warnings += 1,
            }
            let rule_id = result
                .get("ruleId")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            // Q504 处置:规则计数须基于全量结果，而非 findings 前 12 条采样，否则顶部规则统计失真。
            *summary.rule_counts.entry(rule_id.clone()).or_insert(0) += 1;
            let message = result
                .pointer("/message/text")
                .and_then(Value::as_str)
                .or_else(|| result.pointer("/message/markdown").and_then(Value::as_str))
                .unwrap_or("")
                .to_string();
            let location = result
                .get("locations")
                .and_then(Value::as_array)
                .and_then(|items| items.first());
            let file = location
                .and_then(|loc| loc.pointer("/physicalLocation/artifactLocation/uri"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| "?".to_string());
            let line = location
                .and_then(|loc| loc.pointer("/physicalLocation/region/startLine"))
                .and_then(Value::as_u64);

            if summary.findings.len() < 12 {
                summary.findings.push(SarifFinding {
                    level,
                    rule_id,
                    file,
                    line,
                    message: compact_spaces(&message),
                });
            }
        }
    }

    Some(summary)
}

/// 将 JUnit 摘要渲染为 JUNIT|SUMMARY/!JUNIT|FAIL/.../JUNIT|SKIP/JUNIT|SLOW 摘要行。
#[tracing::instrument(level = "debug", skip_all)]
fn render_junit_summary(
    lines: &mut Vec<String>,
    summary: JunitSummary,
    _dict_engine: &mut DictionaryEngine,
) {
    lines.push(format!(
        "JUNIT|SUMMARY|suites={} tests={} failures={} errors={} skipped={} time={:.3}s",
        summary.suites,
        summary.tests,
        summary.failures,
        summary.errors,
        summary.skipped,
        summary.time
    ));
    if !summary.properties.is_empty() {
        let props = summary
            .properties
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(format!("JUNIT|PROPS|{props}"));
    }

    let mut slow_cases: Vec<_> = summary
        .cases
        .iter()
        .filter(|case| case.time > 0.0)
        .collect();
    slow_cases.sort_by(|a, b| {
        b.time
            .partial_cmp(&a.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for case in summary
        .cases
        .iter()
        .filter(|case| matches!(case.status, JunitStatus::Failure | JunitStatus::Error))
        .take(10)
    {
        let level = match case.status {
            JunitStatus::Pass => "PASS",
            JunitStatus::Failure => "FAIL",
            JunitStatus::Error => "ERROR",
            JunitStatus::Skipped => "SKIP",
        };
        let suite = compact_junit_name(&case.suite);
        let test = compact_test_name(case);
        lines.push(format!(
            "!JUNIT|{level}|suite={suite} test={test} time={:.3}s msg={}",
            case.time,
            truncate(&one_line(&case.message), 260)
        ));
    }

    let skipped = summary
        .cases
        .iter()
        .filter(|case| case.status == JunitStatus::Skipped)
        .take(4)
        .map(|case| {
            let test = compact_test_name(case);
            if case.message.is_empty() {
                test
            } else {
                format!("{test} reason={}", truncate(&one_line(&case.message), 120))
            }
        })
        .collect::<Vec<_>>();
    if !skipped.is_empty() {
        lines.push(format!("JUNIT|SKIP|sample={}", skipped.join(",")));
    }
    if let Some(case) = slow_cases.first() {
        lines.push(format!(
            "JUNIT|SLOW|test={} time={:.3}s",
            compact_test_name(case),
            case.time
        ));
    }
}

/// 将 SARIF 摘要渲染为 SARIF|SUMMARY/!SARIF|RESULT/SARIF|RULES 摘要行。
#[tracing::instrument(level = "debug", skip_all)]
fn render_sarif_summary(lines: &mut Vec<String>, summary: SarifSummary) {
    // 有 invocation 时附上执行状态，区分干净扫描（exec=ok）与执行失败（exec=fail:N，SAP-0058）。
    let exec_suffix = {
        let invocations = summary.exec_ok + summary.exec_failed;
        if invocations == 0 {
            String::new()
        } else if summary.exec_failed == 0 {
            format!(" inv={invocations} exec=ok")
        } else {
            format!(" inv={invocations} exec=fail:{}", summary.exec_failed)
        }
    };
    lines.push(format!(
        "SARIF|SUMMARY|runs={} results={} error={} warning={} note={} none={} tools={}{}",
        summary.runs,
        summary.results,
        summary.errors,
        summary.warnings,
        summary.notes,
        summary.none,
        if summary.tools.is_empty() {
            "?".to_string()
        } else {
            summary.tools.join(",")
        },
        exec_suffix
    ));

    // 执行失败时保留失败 invocation 的命令行，支撑失败扫描的原因定位（SAP-0058）。
    for cmd in &summary.failed_cmds {
        lines.push(format!("!SARIF|EXEC|cmd={}", truncate(&one_line(cmd), 260)));
    }

    for finding in &summary.findings {
        let line = finding
            .line
            .map(|line| format!(":{line}"))
            .unwrap_or_default();
        lines.push(format!(
            "!SARIF|RESULT|level={} rule={} loc={}{} msg={}",
            finding.level,
            finding.rule_id,
            finding.file,
            line,
            truncate(&one_line(&finding.message), 180)
        ));
    }

    if !summary.rule_counts.is_empty() {
        // Q504 处置:按出现频次降序取 top 6（频次相同再按规则 ID 保证确定性），
        // 不再用 BTreeMap 字典序（字母序）误当作「最常见规则」。
        let mut sorted: Vec<(String, usize)> = summary.rule_counts.clone().into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let top_rules = sorted
            .into_iter()
            .take(6)
            .map(|(rule, count)| format!("{rule}={count}"))
            .collect::<Vec<_>>()
            .join(",");
        lines.push(format!("SARIF|RULES|{top_rules}"));
    }
}

/// 从 XML 事件中读取指定属性名对应的值（自动解码并反转义实体）。
#[tracing::instrument(level = "debug", skip_all)]
fn xml_attr(reader: &Reader<&[u8]>, event: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    event.attributes().flatten().find_map(|attr| {
        (attr.key.as_ref() == key)
            .then(|| attr.decode_and_unescape_value(reader.decoder()).ok())
            .flatten()
            .map(|value| value.into_owned())
    })
}

/// 捕获 JUnit property 元素中受关注的属性（commit/workflow/python/pytest/hostname），避免重复。
#[tracing::instrument(level = "debug", skip_all)]
fn capture_junit_property(
    reader: &Reader<&[u8]>,
    event: &BytesStart<'_>,
    summary: &mut JunitSummary,
) {
    let Some(name) = xml_attr(reader, event, b"name") else {
        return;
    };
    let Some(value) = xml_attr(reader, event, b"value") else {
        return;
    };
    if matches!(
        name.as_str(),
        "commit" | "workflow" | "python" | "pytest" | "hostname"
    ) && !summary
        .properties
        .iter()
        .any(|(existing, _)| existing == &name)
    {
        summary.properties.push((name, value));
    }
}

/// 读取 XML 数值属性并解析为 usize，缺失或非法时回退 0。
#[tracing::instrument(level = "debug", skip_all)]
fn attr_usize(reader: &Reader<&[u8]>, event: &BytesStart<'_>, key: &[u8]) -> usize {
    xml_attr(reader, event, key)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0)
}

/// 读取 XML 数值属性并解析为 f64，缺失或非法时回退 0.0。
#[tracing::instrument(level = "debug", skip_all)]
fn attr_f64(reader: &Reader<&[u8]>, event: &BytesStart<'_>, key: &[u8]) -> f64 {
    xml_attr(reader, event, key)
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// 紧凑化 suite 名称：空名称显示为 "?"。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_junit_name(name: &str) -> String {
    if name.is_empty() {
        "?".to_string()
    } else {
        name.to_string()
    }
}

/// 紧凑化测试名：有 class_name 时格式化为 class::name，否则仅返回名称。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_test_name(case: &JunitCase) -> String {
    if case.class_name.is_empty() {
        compact_junit_name(&case.name)
    } else {
        format!("{}::{}", compact_junit_name(&case.class_name), case.name)
    }
}

/// 将文本截断至指定字节数（按 UTF-8 字符边界调整），超长时追加省略号。
#[tracing::instrument(level = "debug", skip_all)]
fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

/// 将多行文本折叠为单行：换行符替换为空格并压缩连续空白。
#[tracing::instrument(level = "debug", skip_all)]
fn one_line(text: &str) -> String {
    compact_spaces(&text.replace(['\r', '\n'], " "))
}
