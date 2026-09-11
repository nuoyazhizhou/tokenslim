use super::methods::*;
use std::path::PathBuf;


fn sample_dir() -> PathBuf {
    crate::plugins::test_utils::vcs_sample_dir("vcs_az_plugin")
}
fn read_case(case_name: &str) -> String {
    crate::plugins::test_utils::vcs_read_case("vcs_az_plugin", case_name)
}

// ============================================================================
// Case 96: show — 命令锚点 + K-V 扁平化
// ============================================================================
/// 测试：az repos show 样例（case 96）被压缩为锚点+K-V。
#[test]
fn test_show_case_96() {
    let raw = read_case("case_96_az_repos_show");
    let compacted = compact_az_log_for_ai(&raw);

    assert!(compacted.starts_with("az repos show"), "必须保留命令锚点");

    assert!(
        compacted.contains("PRJ:my-project"),
        "项目名应映射为 PRJ:my-project"
    );
    assert!(
        compacted.contains("BR:main"),
        "DefaultBranch 应映射为 BR:main"
    );
    assert!(
        compacted.contains("URL:az:myorg/my-project"),
        "RemoteUrl 应缩写为 az:myorg/my-project"
    );
    assert!(
        compacted.contains("SS:up to date"),
        "SyncStatus 应映射为 SS:up to date"
    );
}

// ============================================================================
// Case 112: list — 命令锚点 + JSON 平面化提取
// ============================================================================
/// 测试：az repos list 样例（case 112）提取项目/仓库/ID/分支字段。
#[test]
fn test_list_case_112() {
    let raw = read_case("case_112_az_repos_list");
    let compacted = compact_az_log_for_ai(&raw);

    assert!(compacted.starts_with("az repos list"), "必须保留命令锚点");

    // JSON 括号应被消除
    assert!(!compacted.contains('['), "JSON 数组括号应被消除");
    assert!(!compacted.contains(']'), "JSON 数组括号应被消除");

    assert!(
        compacted.contains("PRJ:MyProject"),
        "嵌套 project.name 应映射为 PRJ:MyProject"
    );
    assert!(
        compacted.contains("REPO:my-repo"),
        "name 应映射为 REPO:my-repo"
    );
    assert!(
        compacted.contains("ID:abc123-def456-ghi789"),
        "id 应映射为 ID:abc123-def456-ghi789"
    );
    assert!(
        compacted.contains("BR:main"),
        "defaultBranch 应映射为 BR:main"
    );
}

// ============================================================================
// Case 160: create — 命令锚点 + A: 映射 + URL 缩写 + ✓ 清除
// ============================================================================
/// 测试：az repos create 样例（case 160）映射 A: 行并缩写 URL。
#[test]
fn test_create_case_160() {
    let raw = read_case("case_160_az_repos_create");
    let compacted = compact_az_log_for_ai(&raw);

    assert!(compacted.starts_with("az repos create"), "必须保留命令锚点");

    // ✓ 应被彻底清除
    assert!(!compacted.contains('✓'), "勾号应被消除");

    assert!(
        compacted.contains("A:my-new-repo"),
        "创建应映射为 A:my-new-repo"
    );
    assert!(
        compacted.contains("URL:az:org/MyProject/_git/my-new-repo"),
        "URL 应缩写为 az:..."
    );
}

// ============================================================================
// Case 161: delete — 命令锚点 + D: 映射 + ✓ 清除
// ============================================================================
/// 测试：az repos delete 样例（case 161）映射 D: 行。
#[test]
fn test_delete_case_161() {
    let raw = read_case("case_161_az_repos_delete");
    let compacted = compact_az_log_for_ai(&raw);

    assert!(compacted.starts_with("az repos delete"), "必须保留命令锚点");

    assert!(!compacted.contains('✓'), "勾号应被消除");

    assert!(compacted.contains("D:my-repo"), "删除应映射为 D:my-repo");
}

// ============================================================================
// Case 209: create(no-url) — 无 URL 也要提取 A:
// ============================================================================
/// 测试：无 URL 的 create 样例（case 209）仅映射 A: 行。
#[test]
fn test_create_case_209_no_url() {
    let raw = read_case("case_209_az_repos_create_no_url");
    let compacted = compact_az_log_for_ai(&raw);

    assert!(compacted.starts_with("az repos create"), "必须保留命令锚点");
    assert!(
        compacted.contains("A:infra-tools"),
        "无 URL 场景也应提取 A:repo"
    );
    assert!(!compacted.contains('✓'), "勾号应被消除");
}

// ============================================================================
// Case 210: delete(confirm) — 删除确认信息应映射为 D:
// ============================================================================
/// 测试：delete 确认样例（case 210）被正确处理。
#[test]
fn test_delete_case_210_confirm() {
    let raw = read_case("case_210_az_repos_delete_confirm");
    let compacted = compact_az_log_for_ai(&raw);

    assert!(compacted.starts_with("az repos delete"), "必须保留命令锚点");
    assert!(
        compacted.contains("D:legacy-repo"),
        "删除确认应映射为 D:legacy-repo"
    );
    assert!(!compacted.contains('✓'), "勾号应被消除");
}

// ============================================================================
// Case 211: generic(error) — generic 路径需保留 URL 与错误信号
// ============================================================================
/// 测试：通用错误样例（case 211）保留错误信号与 URL。
#[test]
fn test_generic_case_211_error_and_url() {
    let raw = read_case("case_211_az_repos_generic_error");
    let compacted = compact_az_log_for_ai(&raw);

    assert!(
        compacted.starts_with("az repos policy list"),
        "必须保留命令锚点"
    );
    assert!(
        compacted.contains("URL:az:myorg/myproj/_git/temp-repo"),
        "RemoteUrl 应缩写为 az: 前缀"
    );
    assert!(
        compacted.contains("!error: unauthorized to read policy"),
        "错误信号应映射为 ! 前缀"
    );
    assert!(
        !compacted.contains("Repository created:"),
        "generic 路径应去除 repository created 噪音"
    );
}

// ============================================================================
// Case 212: update(kv) — generic K-V 应映射为 REPO/BR/URL/SS
// ============================================================================
/// 测试：K-V 映射样例（case 212）被扁平化。
#[test]
fn test_generic_case_212_kv_mapping() {
    let raw = read_case("case_212_az_repos_update_kv");
    let compacted = compact_az_log_for_ai(&raw);

    assert!(compacted.starts_with("az repos update"), "必须保留命令锚点");
    assert!(compacted.contains("REPO:app-repo"), "Name 应映射为 REPO");
    assert!(compacted.contains("BR:main"), "DefaultBranch 应映射为 BR");
    assert!(
        compacted.contains("URL:az:myorg/app-project/_git/app-repo"),
        "WebUrl 应缩写为 az: 前缀"
    );
    assert!(
        compacted.contains("SS:ahead by 2"),
        "SyncStatus 应映射为 SS"
    );
}

// ============================================================================
// Case 221: show(ansi+error) — ANSI 应剥离，错误信号不丢失
// ============================================================================
/// 测试：含 ANSI 的 show 样例（case 221）被正确清理。
#[test]
fn test_show_case_221_ansi_error() {
    let raw = read_case("case_221_az_repos_show_ansi_error").replace("\\u001b", "\u{1b}");
    let compacted = compact_az_log_for_ai(&raw);

    assert!(compacted.starts_with("az repos show"), "必须保留命令锚点");
    assert!(compacted.contains("BR:main"), "ANSI 包裹的分支字段应可解析");
    assert!(
        compacted.contains("URL:az:myorg/my-project"),
        "ANSI 包裹的 URL 应缩写"
    );
    assert!(
        compacted.contains("!error: failed to query repo policy"),
        "show 路径中的错误信号应保留"
    );
    assert!(!compacted.contains('\u{1b}'), "输出不应包含 ANSI 转义");
}

// ============================================================================
// 短输入回退
// ============================================================================
/// 测试：短输入直接返回原文（不压缩）。
#[test]
fn test_short_input_fallback() {
    let raw = "az help";
    let compacted = compact_az_log_for_ai(raw);
    assert_eq!(compacted, raw, "过短输入应直接返回原始文本");
}

// ============================================================================
// 噪音与异常映射
// ============================================================================
/// 测试：az 噪音行检测。
#[test]
fn test_az_noise_detection() {
    assert!(super::methods::is_az_noise("Repository created: my-repo"));
    assert!(super::methods::is_az_noise("Repository deleted: my-repo"));
    assert!(!super::methods::is_az_noise("DefaultBranch: main"));
}

/// 测试：az 警报行映射。
#[test]
fn test_az_alert_mapping() {
    assert!(super::methods::map_az_alert("CONFLICT: merge conflict").is_some());
    assert!(super::methods::map_az_alert("error: something wrong").is_some());
    assert!(super::methods::map_az_alert("Push failed").is_some());
    assert!(super::methods::map_az_alert("az repos show").is_none());
}
