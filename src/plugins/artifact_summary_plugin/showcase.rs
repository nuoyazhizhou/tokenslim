//! SARIF/JUnit showcase report generation.

use crate::plugins::artifact_summary_plugin::ArtifactSummaryPlugin;
use crate::plugins::infra_tools_common::{write_showcase_report, ShowcaseCase};

#[test]
fn generate_artifact_summary_showcase_report() {
    let plugin = ArtifactSummaryPlugin::new();
    let cases = [
        ShowcaseCase {
            file_name: "case_001_junit_success.xml",
            title: "JUnit success suite",
        },
        ShowcaseCase {
            file_name: "case_002_junit_failures.xml",
            title: "JUnit assertion failures",
        },
        ShowcaseCase {
            file_name: "case_003_junit_errors_skipped.xml",
            title: "JUnit errors and skipped tests",
        },
        ShowcaseCase {
            file_name: "case_004_junit_multi_suite.xml",
            title: "JUnit multi-suite artifact",
        },
        ShowcaseCase {
            file_name: "case_005_junit_pytest_properties.xml",
            title: "Pytest JUnit XML artifact",
        },
        ShowcaseCase {
            file_name: "case_006_junit_large_repeated_failures.xml",
            title: "JUnit repeated failure artifact",
        },
        ShowcaseCase {
            file_name: "case_007_sarif_codeql_error.json",
            title: "CodeQL SARIF errors",
        },
        ShowcaseCase {
            file_name: "case_008_sarif_semgrep_warnings.json",
            title: "Semgrep SARIF warnings",
        },
        ShowcaseCase {
            file_name: "case_009_sarif_trivy_security.json",
            title: "Trivy SARIF security findings",
        },
        ShowcaseCase {
            file_name: "case_010_sarif_multiple_runs.json",
            title: "SARIF multiple tools",
        },
        ShowcaseCase {
            file_name: "case_011_sarif_no_results.json",
            title: "SARIF clean artifact",
        },
        ShowcaseCase {
            file_name: "case_012_junit_windows_paths.xml",
            title: "JUnit Windows path classes",
        },
    ];
    write_showcase_report(
        &plugin,
        "artifact_summary_plugin",
        "artifact_summary_compact_showcase_report.txt",
        &cases,
    );
}
