//! SARIF/JUnit artifact plugin tests.

use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::SliceType;
use crate::plugins::artifact_summary_plugin::ArtifactSummaryPlugin;
use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_file};

#[test]
fn detects_junit_xml_artifact() {
    let plugin = ArtifactSummaryPlugin::new();
    let raw = read_sample_file("artifact_summary_plugin", "case_002_junit_failures.xml");
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

#[test]
fn detects_sarif_json_artifact() {
    let plugin = ArtifactSummaryPlugin::new();
    let raw = read_sample_file(
        "artifact_summary_plugin",
        "case_007_sarif_codeql_error.json",
    );
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

#[test]
fn compresses_junit_failures() {
    let plugin = ArtifactSummaryPlugin::new();
    let raw = read_sample_file("artifact_summary_plugin", "case_002_junit_failures.xml");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("JUNIT|SUMMARY|"));
    assert!(out.contains("!JUNIT|FAIL|"));
    assert!(out.len() <= raw.len());
}

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

#[test]
fn keeps_clean_artifact_summary() {
    let plugin = ArtifactSummaryPlugin::new();
    let raw = read_sample_file("artifact_summary_plugin", "case_011_sarif_no_results.json");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("SARIF|SUMMARY|runs=1 results=0"));
    assert!(out.len() <= raw.len());
}
