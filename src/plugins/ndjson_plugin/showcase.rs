//! NDJSON showcase 报告生成。

use super::*;
use crate::plugins::infra_tools_common::{write_showcase_report, ShowcaseCase};

#[test]
fn generate_ndjson_showcase_report() {
    let plugin = NdjsonPlugin::new();
    let cases = [
        ShowcaseCase {
            file_name: "case_001_go_test_fail.log",
            title: "go test 失败聚合",
        },
        ShowcaseCase {
            file_name: "case_002_multi_package.log",
            title: "多包 go test 聚合",
        },
        ShowcaseCase {
            file_name: "case_003_generic_events.log",
            title: "通用事件 NDJSON",
        },
        ShowcaseCase {
            file_name: "case_004_not_ndjson.log",
            title: "非 NDJSON 回退",
        },
        ShowcaseCase {
            file_name: "case_005_single_json.log",
            title: "单行 JSON 回退",
        },
        ShowcaseCase {
            file_name: "case_006_test_output.log",
            title: "测试输出保留",
        },
        ShowcaseCase {
            file_name: "case_007_skipped.log",
            title: "跳过测试聚合",
        },
        ShowcaseCase {
            file_name: "case_008_package_fail.log",
            title: "包级失败",
        },
        ShowcaseCase {
            file_name: "case_009_malformed_mixed.log",
            title: "混合坏行",
        },
        ShowcaseCase {
            file_name: "case_010_large_generic.log",
            title: "大通用 NDJSON 截断",
        },
        ShowcaseCase {
            file_name: "case_011_benchmark.log",
            title: "基准测试输出",
        },
        ShowcaseCase {
            file_name: "case_012_no_compress.log",
            title: "短输入回退",
        },
    ];

    write_showcase_report(
        &plugin,
        "ndjson_plugin",
        "ndjson_compact_showcase_report.txt",
        &cases,
    );
}
