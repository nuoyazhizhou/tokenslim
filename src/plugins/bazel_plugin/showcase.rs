//! Bazel showcase 报告生成。

use crate::plugins::bazel_plugin::BazelPlugin;
use crate::plugins::infra_tools_common::{write_showcase_report, ShowcaseCase};

#[test]
fn generate_bazel_showcase_report() {
    let plugin = BazelPlugin::new();
    let cases = [
        ShowcaseCase {
            file_name: "case_001_build.log",
            title: "构建摘要",
        },
        ShowcaseCase {
            file_name: "case_002_error.log",
            title: "失败目标",
        },
        ShowcaseCase {
            file_name: "case_003_test_pass.log",
            title: "测试通过",
        },
        ShowcaseCase {
            file_name: "case_004_test_fail.log",
            title: "测试失败",
        },
        ShowcaseCase {
            file_name: "case_005_cache.log",
            title: "缓存命中",
        },
        ShowcaseCase {
            file_name: "case_006_remote_exec.log",
            title: "远程执行",
        },
        ShowcaseCase {
            file_name: "case_007_query.log",
            title: "query 输出",
        },
        ShowcaseCase {
            file_name: "case_008_sync.log",
            title: "sync 输出",
        },
        ShowcaseCase {
            file_name: "case_009_clean.log",
            title: "clean 输出",
        },
        ShowcaseCase {
            file_name: "case_010_ansi.log",
            title: "ANSI 净化",
        },
        ShowcaseCase {
            file_name: "case_011_no_compress.log",
            title: "不压缩场景",
        },
        ShowcaseCase {
            file_name: "case_012_no_action.log",
            title: "无动作",
        },
    ];
    write_showcase_report(
        &plugin,
        "bazel_plugin",
        "bazel_compact_showcase_report.txt",
        &cases,
    );
}
