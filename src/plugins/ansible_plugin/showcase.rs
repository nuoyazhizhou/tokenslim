//! Ansible showcase 报告生成。

use crate::plugins::ansible_plugin::AnsiblePlugin;
use crate::plugins::infra_tools_common::{write_showcase_report, ShowcaseCase};

#[test]
fn generate_ansible_showcase_report() {
    let plugin = AnsiblePlugin::new();
    let cases = [
        ShowcaseCase {
            file_name: "case_001_play.log",
            title: "play 聚合",
        },
        ShowcaseCase {
            file_name: "case_002_failed.log",
            title: "失败任务",
        },
        ShowcaseCase {
            file_name: "case_003_skipped.log",
            title: "跳过任务",
        },
        ShowcaseCase {
            file_name: "case_004_unreachable.log",
            title: "主机不可达",
        },
        ShowcaseCase {
            file_name: "case_005_no_changes.log",
            title: "无变更",
        },
        ShowcaseCase {
            file_name: "case_006_verbose_json.log",
            title: "冗长 JSON",
        },
        ShowcaseCase {
            file_name: "case_007_loop.log",
            title: "循环任务",
        },
        ShowcaseCase {
            file_name: "case_008_handler.log",
            title: "handler 输出",
        },
        ShowcaseCase {
            file_name: "case_009_syntax_error.log",
            title: "语法错误",
        },
        ShowcaseCase {
            file_name: "case_010_vault_error.log",
            title: "vault 错误",
        },
        ShowcaseCase {
            file_name: "case_011_check_mode.log",
            title: "check mode",
        },
        ShowcaseCase {
            file_name: "case_012_no_compress.log",
            title: "不压缩场景",
        },
    ];
    write_showcase_report(
        &plugin,
        "ansible_plugin",
        "ansible_compact_showcase_report.txt",
        &cases,
    );
}
