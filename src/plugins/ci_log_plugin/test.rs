//! CI/CD 外壳日志插件样本驱动测试。

use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::SliceType;
use crate::plugins::ci_log_plugin::CiLogPlugin;
use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_file};

/// 测试：GitHub Actions 失败日志被插件识别。
#[test]
fn detects_github_actions_shell() {
    let plugin = CiLogPlugin::new();
    let raw = read_sample_file("ci_log_plugin", "case_002_github_actions_failure.log");
    assert!(plugin.detect(&make_log_slice(&raw)).is_some());
}

/// 测试：GitHub Actions 失败日志压缩为含 provider/status 与错误行的摘要且不膨胀。
#[test]
fn compresses_github_actions_failure() {
    let plugin = CiLogPlugin::new();
    let raw = read_sample_file("ci_log_plugin", "case_002_github_actions_failure.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("CI|SUMMARY|provider=github_actions"));
    assert!(out.contains("status=failed"));
    assert!(out.contains("!CI|ERROR|"));
    assert!(out.len() <= raw.len());
}

/// 测试：GitLab 失败日志压缩为含 provider 与错误的摘要且不膨胀。
#[test]
fn compresses_gitlab_sections() {
    let plugin = CiLogPlugin::new();
    let raw = read_sample_file("ci_log_plugin", "case_008_gitlab_failure.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("provider=gitlab_ci"));
    assert!(out.contains("ERROR"));
    assert!(out.len() <= raw.len());
}

/// 测试：Jenkins pipeline 失败日志压缩为含 provider 与失败状态的摘要且不膨胀。
#[test]
fn compresses_jenkins_pipeline() {
    let plugin = CiLogPlugin::new();
    let raw = read_sample_file("ci_log_plugin", "case_014_jenkins_failure.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("provider=jenkins"));
    assert!(out.contains("Finished: FAILURE") || out.contains("status=failed"));
    assert!(out.len() <= raw.len());
}

/// 测试：Jenkins parallel 三个分支各自压缩为含平台名与通过测试计数的摘要且不膨胀。
#[test]
fn compresses_jenkins_parallel_branches() {
    let plugin = CiLogPlugin::new();
    let raw = read_sample_file("ci_log_plugin", "case_015_jenkins_parallel.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("provider=jenkins"));
    for branch in ["n=linux", "n=windows", "n=macos"] {
        assert!(out.contains(branch), "应保留并行分支 {}", branch);
    }
    // 三个分支均通过且携带 4 个测试证据，不得被 "0 failed" 误标为失败。
    assert!(out.contains("st=ok"));
    assert!(out.contains("t=4 f=0"));
    assert!(!out.contains("n=linux st=failed"));
    assert!(out.len() <= raw.len());
}

/// 测试：CircleCI 与 Buildkite 失败日志分别压缩为含各自 provider 的摘要且不膨胀。
#[test]
fn compresses_circleci_and_buildkite() {
    let plugin = CiLogPlugin::new();
    let circle = read_sample_file("ci_log_plugin", "case_026_circleci_failure.log");
    let buildkite = read_sample_file("ci_log_plugin", "case_030_buildkite_failure.log");
    let circle_out = compress_to_string(&plugin, &circle, SliceType::LogBlock);
    let buildkite_out = compress_to_string(&plugin, &buildkite, SliceType::LogBlock);
    assert!(circle_out.contains("provider=circleci"));
    assert!(buildkite_out.contains("provider=buildkite"));
    assert!(circle_out.len() <= circle.len());
    assert!(buildkite_out.len() <= buildkite.len());
}

/// 测试：Azure Pipelines 注释日志压缩为含 provider 与错误的摘要且不膨胀。
#[test]
fn compresses_azure_annotations() {
    let plugin = CiLogPlugin::new();
    let raw = read_sample_file("ci_log_plugin", "case_020_azure_failure.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("provider=azure_pipelines"));
    assert!(out.contains("!CI|ERROR|"));
    assert!(out.len() <= raw.len());
}

/// 测试：极小未知输入经 ROI 门控原样返回。
#[test]
fn keeps_tiny_unknown_input_by_roi() {
    let plugin = CiLogPlugin::new();
    let raw = read_sample_file("ci_log_plugin", "case_036_no_compress.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert_eq!(out, raw);
}

/// 测试：TeamCity 服务消息日志压缩为含 provider 与错误的摘要且不膨胀。
#[test]
fn compresses_teamcity_service_messages() {
    let plugin = CiLogPlugin::new();
    let raw = read_sample_file("ci_log_plugin", "case_038_teamcity_failure.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("CI|SUMMARY|provider=teamcity"));
    assert!(out.contains("status=failed"));
    assert!(out.contains("Unit tests"));
    assert!(out.contains("!CI|ERROR|"));
    assert!(out.len() <= raw.len());
}

/// 测试：Travis fold 标记日志压缩为含 provider 与错误的摘要且不膨胀。
#[test]
fn compresses_travis_fold_markers() {
    let plugin = CiLogPlugin::new();
    let raw = read_sample_file("ci_log_plugin", "case_041_travis_failure.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("CI|SUMMARY|provider=travis_ci"));
    assert!(out.contains("status=failed"));
    assert!(out.contains("script"));
    assert!(out.contains("!CI|ERROR|"));
    assert!(out.len() <= raw.len());
}

/// 测试：自定义 CI banner 日志压缩为含 provider 与错误的摘要且不膨胀。
#[test]
fn compresses_custom_ci_banners() {
    let plugin = CiLogPlugin::new();
    let raw = read_sample_file("ci_log_plugin", "case_043_custom_banner_failure.log");
    let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
    assert!(out.contains("CI|SUMMARY|provider=custom_ci"));
    assert!(out.contains("status=failed"));
    assert!(out.contains("smoke-test"));
    assert!(out.contains("!CI|ERROR|"));
    assert!(out.len() <= raw.len());
}
