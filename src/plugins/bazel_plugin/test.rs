//! Bazel 插件样例驱动测试。

use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::SliceType;
use crate::plugins::bazel_plugin::BazelPlugin;
use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_file};

#[test]
fn detects_bazel_build() {
    let plugin = BazelPlugin::new();
    let raw = read_sample_file("bazel_plugin", "case_001_build.log");
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

#[test]
fn compresses_bazel_summary() {
    let plugin = BazelPlugin::new();
    let raw = read_sample_file("bazel_plugin", "case_001_build.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("INFO: Analyzed"));
    assert!(out.contains("INFO: Build completed successfully"));
    assert!(out.len() <= raw.len());
}
