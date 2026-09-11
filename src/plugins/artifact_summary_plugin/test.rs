//! SARIF/JUnit artifact plugin tests.

use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::SliceType;
use crate::plugins::artifact_summary_plugin::ArtifactSummaryPlugin;
use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_file};

/// 测试：JUnit XML 构件被插件识别（detect 返回 Some）。
#[test]
fn detects_junit_xml_artifact() {
    let plugin = ArtifactSummaryPlugin::new();
    let raw = read_sample_file("artifact_summary_plugin", "case_002_junit_failures.xml");
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

/// 测试：SARIF JSON 构件被插件识别（detect 返回 Some）。
#[test]
fn detects_sarif_json_artifact() {
    let plugin = ArtifactSummaryPlugin::new();
    let raw = read_sample_file(
        "artifact_summary_plugin",
        "case_007_sarif_codeql_error.json",
    );
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

/// 测试：JUnit 失败构件被压缩为摘要行且不膨胀。
#[test]
fn compresses_junit_failures() {
    let plugin = ArtifactSummaryPlugin::new();
    let raw = read_sample_file("artifact_summary_plugin", "case_002_junit_failures.xml");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("JUNIT|SUMMARY|"));
    assert!(out.contains("!JUNIT|FAIL|"));
    assert!(out.len() <= raw.len());
}

/// 测试：SARIF findings 构件被压缩为摘要行且不膨胀。
#[test]
fn compresses_sarif_findings() {
    let plugin = ArtifactSummaryPlugin::new();
    let raw = read_sample_file(
        "artifact_summary_plugin",
        "case_007_sarif_codeql_error.json",
    );
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("SARIF|SUMMARY|"));
    assert!(out.contains("!SARIF|RESULT|level=error"));
    assert!(out.contains("SARIF|RULES|"));
    assert!(out.len() <= raw.len());
}

/// 测试：无结果的 SARIF 构件仍输出汇总行且不膨胀。
#[test]
fn keeps_clean_artifact_summary() {
    let plugin = ArtifactSummaryPlugin::new();
    let raw = read_sample_file("artifact_summary_plugin", "case_011_sarif_no_results.json");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("SARIF|SUMMARY|runs=1 results=0"));
    // 空结果+执行成功必须同时保留，证明是干净扫描而非执行失败（SAP-0058）。
    assert!(
        out.contains("inv=1 exec=ok"),
        "应保留 invocation 执行成功证据，实际输出：{out}"
    );
    assert!(out.len() <= raw.len());
}
