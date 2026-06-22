//! Protobuf showcase 报告生成。

use crate::plugins::infra_tools_common::{write_showcase_report, ShowcaseCase};
use crate::plugins::protobuf_plugin::ProtobufPlugin;

#[test]
fn generate_protobuf_showcase_report() {
    let plugin = ProtobufPlugin::new();
    let cases = [
        ShowcaseCase {
            file_name: "case_001_protoc.log",
            title: "protoc 诊断",
        },
        ShowcaseCase {
            file_name: "case_002_buf.log",
            title: "buf/protoc 混合",
        },
        ShowcaseCase {
            file_name: "case_003_success.log",
            title: "成功输出",
        },
        ShowcaseCase {
            file_name: "case_004_enum_warning.log",
            title: "枚举告警",
        },
        ShowcaseCase {
            file_name: "case_005_import_error.log",
            title: "导入错误",
        },
        ShowcaseCase {
            file_name: "case_006_duplicate_symbol.log",
            title: "重复符号",
        },
        ShowcaseCase {
            file_name: "case_007_buf_lint.log",
            title: "buf lint",
        },
        ShowcaseCase {
            file_name: "case_008_grpc_plugin.log",
            title: "gRPC 插件错误",
        },
        ShowcaseCase {
            file_name: "case_009_descriptor.log",
            title: "descriptor 输出",
        },
        ShowcaseCase {
            file_name: "case_010_include_path.log",
            title: "include 路径",
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
        "protobuf_plugin",
        "protobuf_compact_showcase_report.txt",
        &cases,
    );
}
