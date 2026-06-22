//! CloudFormation showcase 报告生成。

use crate::plugins::cloudformation_plugin::CloudFormationPlugin;
use crate::plugins::infra_tools_common::{write_showcase_report, ShowcaseCase};

#[test]
fn generate_cloudformation_showcase_report() {
    let plugin = CloudFormationPlugin::new();
    let cases = [
        ShowcaseCase {
            file_name: "case_001_events.log",
            title: "事件聚合",
        },
        ShowcaseCase {
            file_name: "case_002_rollback.log",
            title: "回滚保留",
        },
        ShowcaseCase {
            file_name: "case_003_create_complete.log",
            title: "创建完成",
        },
        ShowcaseCase {
            file_name: "case_004_update_progress.log",
            title: "更新进度",
        },
        ShowcaseCase {
            file_name: "case_005_drift.log",
            title: "漂移检测",
        },
        ShowcaseCase {
            file_name: "case_006_change_set.log",
            title: "变更集",
        },
        ShowcaseCase {
            file_name: "case_007_validate_error.log",
            title: "模板校验错误",
        },
        ShowcaseCase {
            file_name: "case_008_delete.log",
            title: "删除事件",
        },
        ShowcaseCase {
            file_name: "case_009_nested.log",
            title: "嵌套栈",
        },
        ShowcaseCase {
            file_name: "case_010_stack_policy.log",
            title: "栈策略错误",
        },
        ShowcaseCase {
            file_name: "case_011_no_compress.log",
            title: "不压缩场景",
        },
        ShowcaseCase {
            file_name: "case_012_mixed_events.log",
            title: "混合事件",
        },
    ];
    write_showcase_report(
        &plugin,
        "cloudformation_plugin",
        "cloudformation_compact_showcase_report.txt",
        &cases,
    );
}
