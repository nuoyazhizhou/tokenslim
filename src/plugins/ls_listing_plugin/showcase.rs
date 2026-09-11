//! ls_listing showcase 报告生成。

use crate::plugins::infra_tools_common::{write_showcase_report, ShowcaseCase};
use crate::plugins::ls_listing_plugin::LsListingPlugin;

/// 测试：生成 ls_listing 插件的 showcase 对比报告并写入 target 目录。
#[test]
fn generate_ls_listing_showcase_report() {
    let plugin = LsListingPlugin::new();
    let cases = [ShowcaseCase {
        file_name: "case_001_s3_ls_recursive.log",
        title: "aws s3 ls --recursive 大输入列式清单·跨行折叠",
    }];

    write_showcase_report(
        &plugin,
        "ls_listing_plugin",
        "ls_listing_compact_showcase_report.txt",
        &cases,
    );
}
