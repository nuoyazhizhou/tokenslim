use super::methods::*;
use std::path::{Path, PathBuf};

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join("vcs_glab_plugin")
}

fn read_case(case_name: &str) -> String {
    let file_path = sample_dir().join(format!("{case_name}.log"));
    std::fs::read_to_string(&file_path)
        .unwrap_or_else(|err| panic!("读取样本失败 {}: {err}", file_path.display()))
}

// ============================================================================
// Case 95: mr list — 命令锚点 + 列解析
// ============================================================================
#[test]
fn test_mr_list_case_95() {
    let raw = read_case("case_95_glab_mr_list");
    let compacted = compact_glab_log_for_ai(&raw);

    assert!(compacted.starts_with("glab mr list"), "必须保留命令锚点");
    assert!(
        compacted.contains("!123 ST:open OW:@alice Add user authentication flow"),
        "MR 行应包含 !123 ST:open OW:@alice 和标题"
    );
    assert!(
        compacted.contains("!122 ST:merged OW:@bob Refactor database queries"),
        "第二行应包含 !122 ST:merged OW:@bob"
    );
    assert!(
        compacted.contains("!121 ST:closed OW:@alice Fix memory leak"),
        "第三行应包含 !121 ST:closed OW:@alice"
    );
}

// ============================================================================
// Case 108: mr view — 命令锚点 + K-V 扁平化 + DESC
// ============================================================================
#[test]
fn test_mr_view_case_108() {
    let raw = read_case("case_108_glab_mr_view");
    let compacted = compact_glab_log_for_ai(&raw);

    assert!(
        compacted.starts_with("glab mr view 123"),
        "必须保留命令锚点"
    );
    assert!(!compacted.contains("==="), "应消除分隔线");
    assert!(compacted.contains("ST:Open"), "Status 应映射为 ST:Open");
    assert!(compacted.contains("OW:@alice"), "Author 应映射为 OW:@alice");
    assert!(
        compacted.contains("RV:charlie"),
        "Reviewers 应映射为 RV:charlie（去除 (1) 后缀）"
    );
    assert!(
        compacted.contains("BR:feature-branch->main"),
        "Source 应映射为 BR:feature-branch->main"
    );
    assert!(
        compacted.contains("URL:gl:owner/repo/-/merge_requests/123"),
        "Web URL 应缩写为 gl:..."
    );
    assert!(
        compacted.contains("DESC: This merge request adds a new feature"),
        "DESC 必须保留完整描述文本"
    );
    assert!(!compacted.contains("Changes:"), "应消除 Changes 行");
}

// ============================================================================
// Case 109: mr create — 命令锚点 + A: 映射 + URL 消除
// ============================================================================
#[test]
fn test_mr_create_case_109() {
    let raw = read_case("case_109_glab_mr_create");
    let compacted = compact_glab_log_for_ai(&raw);

    assert!(compacted.starts_with("glab mr create"), "必须保留命令锚点");
    assert!(
        !compacted.contains("Creating merge request"),
        "应消除叙述性进度文本"
    );
    assert!(!compacted.contains("gitlab.com"), "应消除 URL 行");
    assert!(compacted.contains("A:!456"), "创建结果应映射为 A:!456");
}

// ============================================================================
// Case 159: mr create (variant) — 命令锚点 + ✓ 去除 + A: 映射
// ============================================================================
#[test]
fn test_mr_create_case_159() {
    let raw = read_case("case_159_glab_mr_create");
    let compacted = compact_glab_log_for_ai(&raw);

    assert!(compacted.starts_with("glab mr create"), "必须保留命令锚点");
    assert!(!compacted.contains('✓'), "应消除 ✓ 勾号");
    assert!(!compacted.contains("gitlab.com"), "应消除 URL 行");
    assert!(compacted.contains("A:!200"), "创建结果应映射为 A:!200");
}

// ============================================================================
// Case 110: issue list — 命令锚点 + 表头清除 + 列解析
// ============================================================================
#[test]
fn test_issue_list_case_110() {
    let raw = read_case("case_110_glab_issue_list");
    let compacted = compact_glab_log_for_ai(&raw);

    assert!(compacted.starts_with("glab issue list"), "必须保留命令锚点");
    assert!(!compacted.contains("----"), "应消除分隔线");
    assert!(!compacted.contains("Assignee"), "应消除表头 Assignee 列");
    assert!(!compacted.contains("Author"), "应消除表头 Author 列");

    assert!(
        compacted.contains("#1 ST:Open OW:@alice Fix login bug LB:bug"),
        "第一个 issue 应包含 #1 ST:Open OW:@alice 和标签"
    );
    assert!(
        compacted.contains("#2 ST:Open OW:@charlie Add new feature LB:enhancement"),
        "第二个 issue 应包含正确的作者和标签"
    );
    assert!(
        compacted.contains("#3 ST:Closed OW:@alice Update documentation LB:documentation"),
        "第三个 issue 应包含 Closed 状态"
    );
}

// ============================================================================
// Case 111: issue view — 命令锚点 + K-V 扁平化 + DESC
// ============================================================================
#[test]
fn test_issue_view_case_111() {
    let raw = read_case("case_111_glab_issue_view");
    let compacted = compact_glab_log_for_ai(&raw);

    assert!(
        compacted.starts_with("glab issue view 5"),
        "必须保留命令锚点"
    );
    assert!(
        !compacted.contains("Steps to reproduce"),
        "应消除 Steps to reproduce"
    );
    assert!(!compacted.contains("Expected:"), "应消除 Expected 行");
    assert!(
        compacted.contains("Actual: Error occurs"),
        "Error 行必须保留"
    );
    assert!(compacted.to_ascii_lowercase().contains("error"));

    assert!(
        compacted.contains("DESC: This is a critical bug that needs to be fixed urgently."),
        "DESC 必须保留完整描述文本"
    );
}

// ============================================================================
// Case 206: issue create — 命令锚点 + ✓ 去除 + A: 映射
// ============================================================================
#[test]
fn test_issue_create_case_206() {
    let raw = read_case("case_206_glab_issue_create");
    let compacted = compact_glab_log_for_ai(&raw);

    assert!(
        compacted.starts_with("glab issue create"),
        "必须保留命令锚点"
    );
    assert!(!compacted.contains('✓'), "应消除 ✓ 勾号");
    assert!(!compacted.contains("gitlab.com"), "应消除 URL 行");
    assert!(compacted.contains("A:!50"), "创建结果应映射为 A:!50");
}

// ============================================================================
// 短输入回退 + 噪音检测 + 异常映射
// ============================================================================
#[test]
fn test_short_input_fallback() {
    let raw = "glab help";
    let compacted = compact_glab_log_for_ai(raw);
    assert_eq!(compacted, raw, "过短输入应直接返回原始文本");
}

#[test]
fn test_glab_noise_detection() {
    assert!(super::methods::is_glab_noise_line(
        "Creating merge request for branch"
    ));
    assert!(super::methods::is_glab_noise_line(
        "Merge request created: !456"
    ));
    assert!(super::methods::is_glab_noise_line(
        "Created merge request !200"
    ));
    assert!(!super::methods::is_glab_noise_line("Status: Open"));
}

#[test]
fn test_glab_alert_mapping() {
    assert!(super::methods::map_glab_alert("CONFLICT: merge conflict").is_some());
    assert!(super::methods::map_glab_alert("error: something wrong").is_some());
    assert!(super::methods::map_glab_alert("!123 Add feature").is_none());
}
