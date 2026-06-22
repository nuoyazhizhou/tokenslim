//! Helm 插件样例驱动测试。

use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::SliceType;
use crate::plugins::helm_plugin::HelmPlugin;
use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_file};

#[test]
fn detects_helm_output() {
    let plugin = HelmPlugin::new();
    let raw = read_sample_file("helm_plugin", "case_001_install.log");
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

#[test]
fn compresses_helm_resources() {
    let plugin = HelmPlugin::new();
    let raw = read_sample_file("helm_plugin", "case_001_install.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("STATUS: deployed"));
    assert!(out.contains("RES:"));
    assert!(out.len() <= raw.len());
}
