//! Helm showcase 报告生成。

use crate::plugins::helm_plugin::HelmPlugin;
use crate::plugins::infra_tools_common::{write_showcase_report, ShowcaseCase};

#[test]
fn generate_helm_showcase_report() {
    let plugin = HelmPlugin::new();
    let cases = [
        ShowcaseCase {
            file_name: "case_001_install.log",
            title: "安装摘要",
        },
        ShowcaseCase {
            file_name: "case_002_failed.log",
            title: "失败发布",
        },
        ShowcaseCase {
            file_name: "case_003_template_error.log",
            title: "模板错误",
        },
        ShowcaseCase {
            file_name: "case_004_status.log",
            title: "状态输出",
        },
        ShowcaseCase {
            file_name: "case_005_list.log",
            title: "列表输出",
        },
        ShowcaseCase {
            file_name: "case_006_rollback.log",
            title: "回滚输出",
        },
        ShowcaseCase {
            file_name: "case_007_test.log",
            title: "测试输出",
        },
        ShowcaseCase {
            file_name: "case_008_repo_update.log",
            title: "仓库更新",
        },
        ShowcaseCase {
            file_name: "case_009_dependency.log",
            title: "依赖构建",
        },
        ShowcaseCase {
            file_name: "case_010_lint_error.log",
            title: "lint 错误",
        },
        ShowcaseCase {
            file_name: "case_011_no_compress.log",
            title: "不压缩场景",
        },
        ShowcaseCase {
            file_name: "case_012_uninstall.log",
            title: "卸载输出",
        },
    ];
    write_showcase_report(
        &plugin,
        "helm_plugin",
        "helm_compact_showcase_report.txt",
        &cases,
    );
}
