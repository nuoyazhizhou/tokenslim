//! Bazel 插件样例驱动测试。

use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::SliceType;
use crate::plugins::bazel_plugin::BazelPlugin;
use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_file};

/// 测试：bazel build 日志被插件识别（detect 返回 Some）。
#[test]
fn detects_bazel_build() {
    let plugin = BazelPlugin::new();
    let raw = read_sample_file("bazel_plugin", "case_001_build.log");
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

/// 测试：bazel 日志被压缩为含分析/构建完成信息的摘要且不膨胀。
#[test]
fn compresses_bazel_summary() {
    let plugin = BazelPlugin::new();
    let raw = read_sample_file("bazel_plugin", "case_001_build.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("INFO: Analyzed"));
    assert!(out.contains("INFO: Build completed successfully"));
    assert!(out.len() <= raw.len());
}

/// 测试：零 action 成功构建必须保留 `Target ... up-to-date` 状态与缩进产物路径（SAP-0071）。
#[test]
fn preserves_up_to_date_output_path() {
    let plugin = BazelPlugin::new();
    let raw = read_sample_file("bazel_plugin", "case_012_no_action.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(
        out.contains("Target //app:server up-to-date"),
        "应保留 up-to-date 状态行，实际输出：{out}"
    );
    assert!(
        out.contains("bazel-bin/app/server"),
        "应保留产物输出路径，实际输出：{out}"
    );
    assert!(out.len() <= raw.len());
}
