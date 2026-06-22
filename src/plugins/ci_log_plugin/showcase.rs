//! CI/CD 外壳日志 showcase 报告生成。

use crate::plugins::ci_log_plugin::CiLogPlugin;
use crate::plugins::infra_tools_common::{write_showcase_report, ShowcaseCase};

#[test]
fn generate_ci_log_showcase_report() {
    let plugin = CiLogPlugin::new();
    let cases = [
        ShowcaseCase {
            file_name: "case_001_github_actions_success.log",
            title: "GitHub Actions success",
        },
        ShowcaseCase {
            file_name: "case_002_github_actions_failure.log",
            title: "GitHub Actions failure",
        },
        ShowcaseCase {
            file_name: "case_003_github_actions_annotations.log",
            title: "GitHub Actions annotations",
        },
        ShowcaseCase {
            file_name: "case_004_github_actions_docker.log",
            title: "GitHub Actions docker inner",
        },
        ShowcaseCase {
            file_name: "case_005_github_actions_pytest.log",
            title: "GitHub Actions pytest inner",
        },
        ShowcaseCase {
            file_name: "case_006_github_actions_matrix.log",
            title: "GitHub Actions matrix",
        },
        ShowcaseCase {
            file_name: "case_007_gitlab_success.log",
            title: "GitLab success",
        },
        ShowcaseCase {
            file_name: "case_008_gitlab_failure.log",
            title: "GitLab failure",
        },
        ShowcaseCase {
            file_name: "case_009_gitlab_artifacts.log",
            title: "GitLab artifacts",
        },
        ShowcaseCase {
            file_name: "case_010_gitlab_cache.log",
            title: "GitLab cache",
        },
        ShowcaseCase {
            file_name: "case_011_gitlab_kubectl.log",
            title: "GitLab kubectl inner",
        },
        ShowcaseCase {
            file_name: "case_012_gitlab_retry.log",
            title: "GitLab retry",
        },
        ShowcaseCase {
            file_name: "case_013_jenkins_success.log",
            title: "Jenkins success",
        },
        ShowcaseCase {
            file_name: "case_014_jenkins_failure.log",
            title: "Jenkins failure",
        },
        ShowcaseCase {
            file_name: "case_015_jenkins_parallel.log",
            title: "Jenkins parallel",
        },
        ShowcaseCase {
            file_name: "case_016_jenkins_gradle.log",
            title: "Jenkins Gradle inner",
        },
        ShowcaseCase {
            file_name: "case_017_jenkins_artifacts.log",
            title: "Jenkins artifacts",
        },
        ShowcaseCase {
            file_name: "case_018_jenkins_unstable.log",
            title: "Jenkins unstable",
        },
        ShowcaseCase {
            file_name: "case_019_azure_success.log",
            title: "Azure success",
        },
        ShowcaseCase {
            file_name: "case_020_azure_failure.log",
            title: "Azure failure",
        },
        ShowcaseCase {
            file_name: "case_021_azure_warnings.log",
            title: "Azure warnings",
        },
        ShowcaseCase {
            file_name: "case_022_azure_artifacts.log",
            title: "Azure artifacts",
        },
        ShowcaseCase {
            file_name: "case_023_azure_docker.log",
            title: "Azure docker inner",
        },
        ShowcaseCase {
            file_name: "case_024_azure_cancelled.log",
            title: "Azure cancelled",
        },
        ShowcaseCase {
            file_name: "case_025_circleci_success.log",
            title: "CircleCI success",
        },
        ShowcaseCase {
            file_name: "case_026_circleci_failure.log",
            title: "CircleCI failure",
        },
        ShowcaseCase {
            file_name: "case_027_circleci_cache_workspace.log",
            title: "CircleCI cache workspace",
        },
        ShowcaseCase {
            file_name: "case_028_circleci_test_summary.log",
            title: "CircleCI test summary",
        },
        ShowcaseCase {
            file_name: "case_029_buildkite_success.log",
            title: "Buildkite success",
        },
        ShowcaseCase {
            file_name: "case_030_buildkite_failure.log",
            title: "Buildkite failure",
        },
        ShowcaseCase {
            file_name: "case_031_buildkite_annotations.log",
            title: "Buildkite annotations",
        },
        ShowcaseCase {
            file_name: "case_032_buildkite_artifacts.log",
            title: "Buildkite artifacts",
        },
        ShowcaseCase {
            file_name: "case_033_act_success.log",
            title: "act success",
        },
        ShowcaseCase {
            file_name: "case_034_act_failure.log",
            title: "act failure",
        },
        ShowcaseCase {
            file_name: "case_035_mixed_ci_wrappers.log",
            title: "mixed CI wrappers",
        },
        ShowcaseCase {
            file_name: "case_036_no_compress.log",
            title: "no compress fallback",
        },
        ShowcaseCase {
            file_name: "case_037_teamcity_success.log",
            title: "TeamCity success",
        },
        ShowcaseCase {
            file_name: "case_038_teamcity_failure.log",
            title: "TeamCity failure",
        },
        ShowcaseCase {
            file_name: "case_039_teamcity_artifacts_warning.log",
            title: "TeamCity artifacts warning",
        },
        ShowcaseCase {
            file_name: "case_040_travis_success.log",
            title: "Travis success",
        },
        ShowcaseCase {
            file_name: "case_041_travis_failure.log",
            title: "Travis failure",
        },
        ShowcaseCase {
            file_name: "case_042_travis_cache_warning.log",
            title: "Travis cache warning",
        },
        ShowcaseCase {
            file_name: "case_043_custom_banner_failure.log",
            title: "custom banner failure",
        },
        ShowcaseCase {
            file_name: "case_044_custom_banner_success.log",
            title: "custom banner success",
        },
    ];

    write_showcase_report(
        &plugin,
        "ci_log_plugin",
        "ci_log_compact_showcase_report.txt",
        &cases,
    );
}
