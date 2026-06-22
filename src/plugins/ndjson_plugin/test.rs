//! NDJSON 插件单元测试。

use super::*;
use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::SliceType;
use crate::plugins::test_utils::{compress_to_string, make_test_slice, read_sample_file};

#[test]
fn test_ndjson_plugin_creation() {
    let plugin = NdjsonPlugin::new();
    assert_eq!(plugin.name(), "ndjson");
    assert_eq!(plugin.priority(), 145);
}

#[test]
fn test_detect_go_test_json() {
    let plugin = NdjsonPlugin::new();
    let raw = read_sample_file("ndjson_plugin", "case_001_go_test_fail.log");
    let confidence = plugin.detect(&make_test_slice(&raw, SliceType::Unknown));
    assert!(confidence.is_some());
    assert!(confidence.unwrap() > 0.9);
}

#[test]
fn test_detect_generic_ndjson() {
    let plugin = NdjsonPlugin::new();
    let raw = read_sample_file("ndjson_plugin", "case_003_generic_events.log");
    let confidence = plugin.detect(&make_test_slice(&raw, SliceType::Unknown));
    assert!(confidence.is_some());
    assert!(confidence.unwrap() > 0.8);
}

#[test]
fn test_detect_not_ndjson() {
    let plugin = NdjsonPlugin::new();
    let raw = read_sample_file("ndjson_plugin", "case_004_not_ndjson.log");
    let confidence = plugin.detect(&make_test_slice(&raw, SliceType::Unknown));
    assert!(confidence.is_none());
}

#[test]
fn test_detect_single_json() {
    let plugin = NdjsonPlugin::new();
    let raw = read_sample_file("ndjson_plugin", "case_005_single_json.log");
    let confidence = plugin.detect(&make_test_slice(&raw, SliceType::Unknown));
    assert!(confidence.is_none());
}

#[test]
fn test_compress_go_test_json() {
    let plugin = NdjsonPlugin::new();
    let raw = read_sample_file("ndjson_plugin", "case_001_go_test_fail.log");
    let compressed = compress_to_string(&plugin, &raw, SliceType::Unknown);

    assert!(compressed.contains("go test -json"));
    assert!(compressed.contains("PKG:"));
    assert!(compressed.contains("TestInvalidPassword"));
    assert!(compressed.contains("SUMMARY:"));
    assert!(compressed.contains("3 tests"));
    assert!(compressed.contains("2 passed"));
    assert!(compressed.contains("1 failed"));
}

#[test]
fn test_compress_generic_ndjson() {
    let mut plugin = NdjsonPlugin::new();
    plugin.config.go_test_mode = false;
    let raw = read_sample_file("ndjson_plugin", "case_010_large_generic.log");
    let compressed = compress_to_string(&plugin, &raw, SliceType::Unknown);
    assert!(compressed.contains("truncated"));
}
