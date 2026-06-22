//! Terraform 插件样例驱动测试。

use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::SliceType;
use crate::plugins::terraform_plugin::TerraformPlugin;
use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_file};

#[test]
fn detects_terraform_plan() {
    let plugin = TerraformPlugin::new();
    let raw = read_sample_file("terraform_plugin", "case_001_plan.log");
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

#[test]
fn compresses_terraform_plan_summary() {
    let plugin = TerraformPlugin::new();
    let raw = read_sample_file("terraform_plugin", "case_001_plan.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("terraform plan"));
    assert!(out.contains("PLAN: +2 ~1 -0"));
    assert!(out.contains("computed:"));
    assert!(out.len() <= raw.len());
}
