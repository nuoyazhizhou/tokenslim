//! Terraform showcase 报告生成。

use crate::plugins::infra_tools_common::{write_showcase_report, ShowcaseCase};
use crate::plugins::terraform_plugin::TerraformPlugin;

#[test]
fn generate_terraform_showcase_report() {
    let plugin = TerraformPlugin::new();
    let cases = [
        ShowcaseCase {
            file_name: "case_001_plan.log",
            title: "plan 摘要",
        },
        ShowcaseCase {
            file_name: "case_002_error.log",
            title: "错误保留",
        },
        ShowcaseCase {
            file_name: "case_003_destroy.log",
            title: "destroy 摘要",
        },
        ShowcaseCase {
            file_name: "case_004_apply_success.log",
            title: "apply 成功",
        },
        ShowcaseCase {
            file_name: "case_005_no_changes.log",
            title: "无变更",
        },
        ShowcaseCase {
            file_name: "case_006_module_paths.log",
            title: "模块路径",
        },
        ShowcaseCase {
            file_name: "case_007_drift.log",
            title: "漂移检测",
        },
        ShowcaseCase {
            file_name: "case_008_import.log",
            title: "导入输出",
        },
        ShowcaseCase {
            file_name: "case_009_workspace.log",
            title: "工作区输出",
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
        "terraform_plugin",
        "terraform_compact_showcase_report.txt",
        &cases,
    );
}
