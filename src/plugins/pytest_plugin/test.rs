//! pytest 插件样本驱动测试。

use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::SliceType;
use crate::plugins::pytest_plugin::PytestPlugin;
use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_file};

#[test]
fn detects_pytest_session() {
    let plugin = PytestPlugin::new();
    let raw = read_sample_file("pytest_plugin", "case_001_failed.log");
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

#[test]
fn compresses_failed_summary() {
    let plugin = PytestPlugin::new();
    let raw = read_sample_file("pytest_plugin", "case_001_failed.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("PYTEST:"));
    assert!(out.contains("failed=1"));
    assert!(out.contains("AssertionError"));
    assert!(out.len() <= raw.len());
}

#[test]
fn compresses_collection_error() {
    let plugin = PytestPlugin::new();
    let raw = read_sample_file("pytest_plugin", "case_004_collection_error.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("errors=1") || out.contains("ERROR"));
    assert!(out.contains("ModuleNotFoundError"));
    assert!(out.len() <= raw.len());
}

#[test]
fn compresses_xdist_and_rerun_ci_signals() {
    let plugin = PytestPlugin::new();
    let xdist = read_sample_file("pytest_plugin", "case_013_xdist_failure_ci.log");
    let xdist_out = compress_to_string(&plugin, &xdist, SliceType::LogBlock);
    assert!(xdist_out.contains("XDIST:"));
    assert!(xdist_out.contains("failed=1"));
    assert!(xdist_out.contains("test_rejects_expired_token"));
    assert!(xdist_out.len() <= xdist.len());

    let rerun = read_sample_file("pytest_plugin", "case_014_rerun_flaky_ci.log");
    let rerun_out = compress_to_string(&plugin, &rerun, SliceType::LogBlock);
    assert!(rerun_out.contains("reruns=2"));
    assert!(rerun_out.contains("test_gateway_timeout"));
    assert!(rerun_out.len() <= rerun.len());
}

#[test]
fn preserves_coverage_and_junitxml_ci_signals() {
    let plugin = PytestPlugin::new();
    let raw = read_sample_file("pytest_plugin", "case_017_coverage_threshold_fail_ci.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("COVERAGE: TOTAL"));
    assert!(out.contains("JUNITXML:"));
    assert!(out.contains("Coverage failure"));
    assert!(out.len() <= raw.len());
}
