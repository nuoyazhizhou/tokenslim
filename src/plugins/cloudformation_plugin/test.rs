//! CloudFormation 插件样例驱动测试。

use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::SliceType;
use crate::plugins::cloudformation_plugin::CloudFormationPlugin;
use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_file};

#[test]
fn detects_cloudformation_events() {
    let plugin = CloudFormationPlugin::new();
    let raw = read_sample_file("cloudformation_plugin", "case_001_events.log");
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

#[test]
fn compresses_cloudformation_failures() {
    let plugin = CloudFormationPlugin::new();
    let raw = read_sample_file("cloudformation_plugin", "case_001_events.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("CREATE_FAILED"));
    assert!(out.contains("WebServer") || out.contains("AWS::"));
    assert!(out.len() <= raw.len());
}
