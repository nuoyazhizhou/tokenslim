use super::methods::*;
use std::path::{Path, PathBuf};

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join("vcs_gh_plugin")
}
fn read_case(c: &str) -> String {
    let p = sample_dir().join(format!("{c}.log"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取样本失败 {}: {e}", p.display()))
}

// ============================================================================
// Case 92: pr list — 命令锚点 + 列解析
// ============================================================================
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
#[test]
fn test_issue_create_case_157() {
    let c = compact_gh_log_for_ai(&read_case("case_157_gh_issue_create"));
    assert!(c.starts_with("gh issue create"));
    assert!(!c.contains('✓'));
    assert!(c.contains("A:#42"));
    assert!(c.contains("LB:bug"));
}

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
#[test]
fn test_short_input_fallback() {
    let c = compact_gh_log_for_ai("gh help");
    assert_eq!(c, "gh help");
}

#[test]
fn test_gh_alert_mapping() {
    assert!(super::methods::map_gh_alert("CONFLICT: merge conflict").is_some());
    assert!(super::methods::map_gh_alert("error: something wrong").is_some());
    assert!(super::methods::map_gh_alert("#22 Add feature").is_none());
}
