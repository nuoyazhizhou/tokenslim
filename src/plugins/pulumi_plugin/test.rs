//! Pulumi 插件样例驱动测试。

use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::SliceType;
use crate::plugins::pulumi_plugin::PulumiPlugin;
use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_file};

#[test]
fn detects_pulumi_preview() {
    let plugin = PulumiPlugin::new();
    let raw = read_sample_file("pulumi_plugin", "case_001_preview.log");
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

#[test]
fn compresses_pulumi_ops() {
    let plugin = PulumiPlugin::new();
    let raw = read_sample_file("pulumi_plugin", "case_001_preview.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("OPS: +2 ~1 -0"));
    assert!(out.contains("Resources:"));
    assert!(out.len() <= raw.len());
}
