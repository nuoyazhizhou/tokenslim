//! Ansible 插件样例驱动测试。

use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::SliceType;
use crate::plugins::ansible_plugin::AnsiblePlugin;
use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_file};

#[test]
fn detects_ansible_play() {
    let plugin = AnsiblePlugin::new();
    let raw = read_sample_file("ansible_plugin", "case_001_play.log");
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

#[test]
fn compresses_ansible_tasks() {
    let plugin = AnsiblePlugin::new();
    let raw = read_sample_file("ansible_plugin", "case_001_play.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("TASK [Gathering Facts]"));
    assert!(out.contains("RECAP:"));
    assert!(out.len() <= raw.len());
}
