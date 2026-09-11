use super::methods::*;
use std::path::PathBuf;

fn sample_dir() -> PathBuf {
    crate::plugins::test_utils::vcs_sample_dir("vcs_gh_plugin")
}
fn read_case(c: &str) -> String {
    crate::plugins::test_utils::vcs_read_case("vcs_gh_plugin", c)
}

// ============================================================================
// Case 92: pr list — 命令锚点 + 列解析
// ============================================================================
/// 测试：gh pr list 样例（case 92）行解析。
#[test]
fn test_pr_list_case_92() {
    let c = compact_gh_log_for_ai(&read_case("case_92_gh_pr_list"));
    assert!(c.starts_with("gh pr list"));
    assert!(c.contains("#22 ST:open OW:@alice CR:2026-04-01 Add dark mode support"));
    assert!(c.contains("#21 ST:merged OW:@bob CR:2026-03-28 Fix login redirect issue"));
    assert!(c.contains("#20 ST:closed OW:@alice CR:2026-03-25 Clear cache on logout"));
}

// ============================================================================
// Case 93: issue list — 命令锚点 + 列解析
// ============================================================================
/// 测试：gh issue list 样例（case 93）行解析。
#[test]
fn test_issue_list_case_93() {
    let c = compact_gh_log_for_ai(&read_case("case_93_gh_issue_list"));
    assert!(c.starts_with("gh issue list"));
    assert!(c.contains("#45 ST:open OW:@alice CR:2026-04-05 Performance optimization"));
    assert!(c.contains("#44 ST:closed OW:@bob CR:2026-04-03 Add unit tests for auth module"));
}

// ============================================================================
// Case 99: run list — 命令锚点 + WF 块压缩
// ============================================================================
/// 测试：gh run list 样例（case 99）KV 块压缩。
#[test]
fn test_run_list_case_99() {
    let c = compact_gh_log_for_ai(&read_case("case_99_gh_run_list"));
    assert!(c.starts_with("gh run list"));
    assert!(c.contains("WF:ci-build"));
    assert!(c.contains("ST:success"));
    assert!(c.contains("RN:#5678"));
    assert!(c.contains("BR:main"));
    assert!(c.contains("DUR:3m 24s"));
    assert!(c.contains("CM:abc123def"));
    assert!(c.contains("WF:ci-test"));
    assert!(c.contains("ST:in_progress"));
}

// ============================================================================
// Case 106: api — 命令锚点 + JSON 平面化
// ============================================================================
/// 测试：gh api 样例（case 106）JSON 平面化。
#[test]
fn test_api_case_106() {
    let c = compact_gh_log_for_ai(&read_case("case_106_gh_api"));
    assert!(c.starts_with("gh api repos/owner/repo"));
    assert!(!c.contains('{'), "JSON 括号应被消除");
    assert!(c.contains("NM:my-repo"));
    assert!(c.contains("ID:123456"));
    assert!(c.contains("FN:owner/my-repo"));
    assert!(c.contains("DESC:A sample repository"));
    assert!(c.contains("URL:gh:owner/my-repo"));
}

// ============================================================================
// Case 155: pr create — 命令锚点 + ✓ 去除 + A:
// ============================================================================
/// 测试：gh pr create 样例（case 155）A: 与标签映射。
#[test]
fn test_pr_create_case_155() {
    let c = compact_gh_log_for_ai(&read_case("case_155_gh_pr_create"));
    assert!(c.starts_with("gh pr create"));
    assert!(!c.contains('✓'));
    assert!(c.contains("A:#150"));
    assert!(c.contains("LB:enhancement"));
}

// ============================================================================
// Case 157: issue create — 命令锚点 + ✓ 去除 + A:
// ============================================================================
/// 测试：gh issue create 样例（case 157）A: 与标签映射。
#[test]
fn test_issue_create_case_157() {
    let c = compact_gh_log_for_ai(&read_case("case_157_gh_issue_create"));
    assert!(c.starts_with("gh issue create"));
    assert!(!c.contains('✓'));
    assert!(c.contains("A:#42"));
    assert!(c.contains("LB:bug"));
}

/// 测试：gh issue view 样例（case 158）保留错误信号。
#[test]
fn test_issue_view_case_158_preserves_error() {
    let c = compact_gh_log_for_ai(&read_case("case_158_gh_issue_view"));
    assert!(c.starts_with("gh issue view 42"));
    assert!(c.to_ascii_lowercase().contains("error"));
    assert!(c.contains("Error 500"));
}

// ============================================================================
// 短输入 + 噪音 + 报警
// ============================================================================
/// 测试：短输入直接返回原文（不压缩）。
#[test]
fn test_short_input_fallback() {
    let c = compact_gh_log_for_ai("gh help");
    assert_eq!(c, "gh help");
}

/// 测试：gh 警报行映射。
#[test]
fn test_gh_alert_mapping() {
    assert!(super::methods::map_gh_alert("CONFLICT: merge conflict").is_some());
    assert!(super::methods::map_gh_alert("error: something wrong").is_some());
    assert!(super::methods::map_gh_alert("#22 Add feature").is_none());
}

// ============================================================================
// 负路径回归（P3-187：无标题 PR 行切片 panic 防御）
// ============================================================================
/// 回归：无标题行（如 `#1 a/b [open] 2026-01-01`）时 title_start > author_idx，
/// 不得触发 tokens[title_start..author_idx] 切片 panic。
#[test]
fn test_pr_row_no_title_no_panic() {
    // P3-202：手写 mock 物理化为 samples 测试专用样本。
    let c = compact_gh_log_for_ai(&read_case("test_gh_pr_list_no_title"));
    assert!(c.starts_with("gh pr list"), "{}", c);
    assert!(!c.contains("panicked"), "{}", c);
}

// ============================================================================
// Case 98: pr view — 命令锚点 + 通用 fallback 保语义
// ============================================================================
/// 测试：gh pr view 样例（case 98）经通用压缩保留行身份标识与锚点。
#[test]
fn test_pr_view_case_98() {
    let c = compact_gh_log_for_ai(&read_case("case_98_gh_pr_view"));
    assert!(c.starts_with("gh pr view"), "命令锚点丢失: {}", c);
    assert!(c.contains("#22"), "行号标识丢失: {}", c);
    assert!(c.contains("feature/new-ui"), "分支名丢失: {}", c);
    assert!(c.contains("2026-04-05"), "日期列丢失: {}", c);
    assert!(c.contains("#1234"), "build 引用丢失: {}", c);
}

// ============================================================================
// Case 107: auth — 命令锚点 + 登录账号 K-V 扁平化
// ============================================================================
/// 测试：gh auth status 样例（case 107）映射 OW/ACC/MGR。
#[test]
fn test_auth_case_107() {
    let c = compact_gh_log_for_ai(&read_case("case_107_gh_auth"));
    assert!(c.starts_with("gh auth"), "命令锚点丢失: {}", c);
    assert!(!c.contains('✓'), "应清除 ✓ 符号: {}", c);
    assert!(
        c.contains("OW:@alice"),
        "Logged in 应映射为 OW:@alice: {}",
        c
    );
    assert!(
        c.contains("ACC:alice (123456)"),
        "Current account 应映射为 ACC:alice (123456): {}",
        c
    );
    assert!(c.contains("MGR:gh"), "Manager 应映射为 MGR:gh: {}", c);
    assert!(
        c.contains("NODE:v20.0.0"),
        "Node 应映射为 NODE:v20.0.0: {}",
        c
    );
}

// ============================================================================
// Case 156: pr merge — 命令锚点 + MRG/D 动作映射
// ============================================================================
/// 测试：gh pr merge 样例（case 156）映射 MRG 与删除分支。
#[test]
fn test_pr_merge_case_156() {
    let c = compact_gh_log_for_ai(&read_case("case_156_gh_pr_merge"));
    assert!(c.starts_with("gh pr merge"), "命令锚点丢失: {}", c);
    assert!(!c.contains('✓'), "应清除 ✓ 符号: {}", c);
    assert!(c.contains("MRG:#150"), "合并应映射为 MRG:#150: {}", c);
    assert!(
        c.contains("D:feature-auth"),
        "删除分支应映射为 D:feature-auth: {}",
        c
    );
}

// ============================================================================
// Case 162: run view — 命令锚点 + KV 扁平化 + 缩进任务行消除
// ============================================================================
/// 测试：gh run view 样例（case 162）KV 映射与缩进任务行消除。
#[test]
fn test_run_view_case_162() {
    let c = compact_gh_log_for_ai(&read_case("case_162_gh_run_view"));
    assert!(c.starts_with("gh run view"), "命令锚点丢失: {}", c);
    assert!(c.contains("WF:CI"), "Workflow 应映射为 WF:CI: {}", c);
    assert!(
        c.contains("RN:#12345 - Build and Test"),
        "Run 应映射为 RN:#12345: {}",
        c
    );
    assert!(c.contains("BR:main"), "Branch 应映射为 BR:main: {}", c);
    assert!(c.contains("JB:2 jobs"), "Jobs 应映射为 JB:2 jobs: {}", c);
    assert!(!c.contains("(3m24s)"), "缩进任务行应被消除: {}", c);
}

// ============================================================================
// Case 197: run view(带 ✓) — 缩进任务行带勾号也应消除
// ============================================================================
/// 测试：gh run view 样例（case 197）勾号任务行被消除。
#[test]
fn test_run_view_case_197() {
    let c = compact_gh_log_for_ai(&read_case("case_197_gh_run_view"));
    assert!(c.starts_with("gh run view"), "命令锚点丢失: {}", c);
    assert!(!c.contains('✓'), "应清除 ✓ 符号: {}", c);
    assert!(c.contains("WF:CI"), "Workflow 应映射为 WF:CI: {}", c);
    assert!(
        c.contains("ST:Success"),
        "Status 应映射为 ST:Success: {}",
        c
    );
    assert!(c.contains("JB:2 jobs"), "Jobs 应映射为 JB:2 jobs: {}", c);
    assert!(!c.contains("(3m24s)"), "勾号任务行应被消除: {}", c);
}

// ============================================================================
// Case 198: repo list — 命令锚点 + 表头消除
// ============================================================================
/// 测试：gh repo list 样例（case 198）表头消除、行保留。
#[test]
fn test_repo_list_case_198() {
    let c = compact_gh_log_for_ai(&read_case("case_198_gh_repo_list"));
    assert!(c.starts_with("gh repo list"), "命令锚点丢失: {}", c);
    assert!(!c.contains("visibility"), "表头应消除: {}", c);
    assert!(
        c.contains("my-repo Main repository private 2026-04-08"),
        "数据行应保留: {}",
        c
    );
    assert!(
        c.contains("another-repo Another project public 2026-04-05"),
        "第二数据行应保留: {}",
        c
    );
}

// ============================================================================
// Case 199: repo view — 命令锚点 + K-V 符号化（VIS/DB/LC）
// ============================================================================
/// 测试：gh repo view 样例（case 199）元数据 K-V 符号化。
#[test]
fn test_repo_view_case_199() {
    let c = compact_gh_log_for_ai(&read_case("case_199_gh_repo_view"));
    assert!(
        c.starts_with("gh repo view owner/my-repo"),
        "命令锚点丢失: {}",
        c
    );
    assert!(c.contains("owner/my-repo"), "仓库路径丢失: {}", c);
    assert!(
        c.contains("VIS:private"),
        "Visibility 应映射为 VIS:private: {}",
        c
    );
    assert!(
        c.contains("DB:main"),
        "Default branch 应映射为 DB:main: {}",
        c
    );
    assert!(c.contains("LC:MIT"), "License 应映射为 LC:MIT: {}", c);
}

// ============================================================================
// Case 200: gist list — 命令锚点 + 表头/勾号消除
// ============================================================================
/// 测试：gh gist list 样例（case 200）表头与勾号被消除。
#[test]
fn test_gist_list_case_200() {
    let c = compact_gh_log_for_ai(&read_case("case_200_gh_gist_list"));
    assert!(c.starts_with("gh gist list"), "命令锚点丢失: {}", c);
    assert!(!c.contains('✓'), "应清除 ✓ 符号: {}", c);
    assert!(!c.contains("DESCRIPTION"), "表头应消除: {}", c);
    assert!(
        c.contains("Add utility functions 2 files 2026-04-05"),
        "第一数据行应保留: {}",
        c
    );
    assert!(
        c.contains("Fix bug in auth 1 file 2026-04-01"),
        "第二数据行应保留: {}",
        c
    );
}

// ============================================================================
// Case 201: gist view — 命令锚点 + K-V 扁平化 + URL 噪音消除
// ============================================================================
/// 测试：gh gist view 样例（case 201）字段符号化。
#[test]
fn test_gist_view_case_201() {
    let c = compact_gh_log_for_ai(&read_case("case_201_gh_gist_view"));
    assert!(c.starts_with("gh gist view abc1234"), "命令锚点丢失: {}", c);
    assert!(c.contains("gist:abc1234"), "gist 标识丢失: {}", c);
    assert!(
        c.contains("files:utils.rs, helpers.rs"),
        "Files 字段丢失: {}",
        c
    );
    assert!(c.contains("CR:2026-04-05"), "Created 应映射为 CR: {}", c);
    assert!(!c.contains("gist.github.com"), "URL 行应被消除: {}", c);
}

// ============================================================================
// Case 202: actions list — 命令锚点 + 状态/触发列解析
// ============================================================================
/// 测试：gh actions list 样例（case 202）表头消除、行保留。
#[test]
fn test_actions_list_case_202() {
    let c = compact_gh_log_for_ai(&read_case("case_202_gh_actions_list"));
    assert!(c.starts_with("gh actions list"), "命令锚点丢失: {}", c);
    assert!(!c.contains("WORKFLOW"), "表头应消除: {}", c);
    assert!(c.contains("CI Pass 10m push"), "CI 行丢失: {}", c);
    assert!(
        c.contains("Deploy Pending - workflow_dispatch"),
        "Deploy 行丢失: {}",
        c
    );
}

// ============================================================================
// Case 203: actions view — 命令锚点 + KV 扁平化
// ============================================================================
/// 测试：gh actions view 样例（case 203）元数据 K-V 符号化。
#[test]
fn test_actions_view_case_203() {
    let c = compact_gh_log_for_ai(&read_case("case_203_gh_actions_view"));
    assert!(
        c.starts_with("gh actions view 12345"),
        "命令锚点丢失: {}",
        c
    );
    assert!(c.contains("WF:CI"), "Workflow 应映射为 WF:CI: {}", c);
    assert!(c.contains("RN:#12345"), "Run 应映射为 RN:#12345: {}", c);
    assert!(
        c.contains("ST:Success"),
        "Status 应映射为 ST:Success: {}",
        c
    );
    assert!(
        c.contains("CR:2026-04-08 10:00:00"),
        "Created 应映射为 CR:2026-04-08 10:00:00: {}",
        c
    );
}

// ============================================================================
// Case 204: secret list — 命令锚点 + ✓ Set 列规整
// ============================================================================
/// 测试：gh secret list 样例（case 204）勾号消除、密钥行保留。
#[test]
fn test_secret_list_case_204() {
    let c = compact_gh_log_for_ai(&read_case("case_204_gh_secret_list"));
    assert!(c.starts_with("gh secret list"), "命令锚点丢失: {}", c);
    assert!(!c.contains('✓'), "应清除 ✓ 符号: {}", c);
    assert!(c.contains("ACTIONS_DEPLOY_KEY Set"), "密钥行丢失: {}", c);
    assert!(
        c.contains("DATABASE_URL Set (encrypted)"),
        "加密密钥行丢失: {}",
        c
    );
}

// ============================================================================
// Case 205: deploy list — 命令锚点 + 环境部署行保留
// ============================================================================
/// 测试：gh deploy list 样例（case 205）表头消除、环境行保留。
#[test]
fn test_deploy_list_case_205() {
    let c = compact_gh_log_for_ai(&read_case("case_205_gh_deploy_list"));
    assert!(c.starts_with("gh deploy list"), "命令锚点丢失: {}", c);
    assert!(!c.contains("ENVIRONMENT"), "表头应消除: {}", c);
    assert!(
        c.contains("production https://prod.example.com 2026-04-08"),
        "production 行丢失: {}",
        c
    );
    assert!(
        c.contains("development https://dev.example.com 2026-04-05"),
        "development 行丢失: {}",
        c
    );
}
