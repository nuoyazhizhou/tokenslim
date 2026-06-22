//! pytest showcase 报告生成。

use crate::plugins::infra_tools_common::{write_showcase_report, ShowcaseCase};
use crate::plugins::pytest_plugin::PytestPlugin;

#[test]
fn generate_pytest_showcase_report() {
    let plugin = PytestPlugin::new();
    let cases = [
        ShowcaseCase {
            file_name: "case_001_failed.log",
            title: "失败断言",
        },
        ShowcaseCase {
            file_name: "case_002_all_passed.log",
            title: "全部通过",
        },
        ShowcaseCase {
            file_name: "case_003_many_passed.log",
            title: "大量通过折叠",
        },
        ShowcaseCase {
            file_name: "case_004_collection_error.log",
            title: "收集错误",
        },
        ShowcaseCase {
            file_name: "case_005_skipped_xfail.log",
            title: "跳过与 xfail",
        },
        ShowcaseCase {
            file_name: "case_006_param_fail.log",
            title: "参数化失败",
        },
        ShowcaseCase {
            file_name: "case_007_fixture_error.log",
            title: "fixture 错误",
        },
        ShowcaseCase {
            file_name: "case_008_assertion_diff.log",
            title: "断言 diff",
        },
        ShowcaseCase {
            file_name: "case_009_coverage.log",
            title: "覆盖率输出",
        },
        ShowcaseCase {
            file_name: "case_010_warnings.log",
            title: "警告摘要",
        },
        ShowcaseCase {
            file_name: "case_011_no_compress.log",
            title: "短输入回退",
        },
        ShowcaseCase {
            file_name: "case_012_mixed.log",
            title: "混合场景",
        },
        ShowcaseCase {
            file_name: "case_013_xdist_failure_ci.log",
            title: "pytest xdist CI failure",
        },
        ShowcaseCase {
            file_name: "case_014_rerun_flaky_ci.log",
            title: "pytest rerun flaky CI",
        },
        ShowcaseCase {
            file_name: "case_015_coverage_junitxml_ci.log",
            title: "pytest coverage junitxml CI",
        },
        ShowcaseCase {
            file_name: "case_016_xdist_all_passed_ci.log",
            title: "pytest xdist all passed CI",
        },
        ShowcaseCase {
            file_name: "case_017_coverage_threshold_fail_ci.log",
            title: "pytest coverage threshold failure",
        },
        ShowcaseCase {
            file_name: "case_018_github_actions_junitxml_error.log",
            title: "pytest junitxml collection error",
        },
    ];

    write_showcase_report(
        &plugin,
        "pytest_plugin",
        "pytest_compact_showcase_report.txt",
        &cases,
    );
}
