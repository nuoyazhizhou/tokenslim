//! Pulumi showcase 报告生成。

use crate::plugins::infra_tools_common::{write_showcase_report, ShowcaseCase};
use crate::plugins::pulumi_plugin::PulumiPlugin;

#[test]
fn generate_pulumi_showcase_report() {
    let plugin = PulumiPlugin::new();
    let cases = [
        ShowcaseCase {
            file_name: "case_001_preview.log",
            title: "preview 聚合",
        },
        ShowcaseCase {
            file_name: "case_002_error.log",
            title: "诊断错误",
        },
        ShowcaseCase {
            file_name: "case_003_refresh.log",
            title: "refresh 摘要",
        },
        ShowcaseCase {
            file_name: "case_004_destroy.log",
            title: "destroy 摘要",
        },
        ShowcaseCase {
            file_name: "case_005_no_changes.log",
            title: "无变更",
        },
        ShowcaseCase {
            file_name: "case_006_policy_violation.log",
            title: "策略违规",
        },
        ShowcaseCase {
            file_name: "case_007_stack_outputs.log",
            title: "stack output",
        },
        ShowcaseCase {
            file_name: "case_008_plugin_install.log",
            title: "插件安装",
        },
        ShowcaseCase {
            file_name: "case_009_diff.log",
            title: "diff 输出",
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
            file_name: "case_012_mixed.log",
            title: "混合输出",
        },
    ];
    write_showcase_report(
        &plugin,
        "pulumi_plugin",
        "pulumi_compact_showcase_report.txt",
        &cases,
    );
}
