//! Protobuf 插件样例驱动测试。

use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::SliceType;
use crate::plugins::protobuf_plugin::ProtobufPlugin;
use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_file};

#[test]
fn detects_protoc_output() {
    let plugin = ProtobufPlugin::new();
    let raw = read_sample_file("protobuf_plugin", "case_001_protoc.log");
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

#[test]
fn compresses_protoc_diagnostics() {
    let plugin = ProtobufPlugin::new();
    let raw = read_sample_file("protobuf_plugin", "case_001_protoc.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("PROTOC: 2 errors, 2 warnings"));
    assert!(out.contains("error"));
    assert!(out.len() <= raw.len());
}
