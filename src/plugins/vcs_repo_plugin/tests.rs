use super::methods::*;
use std::path::{Path, PathBuf};

// ============================================================================
// 样板辅助
// ============================================================================
fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join("vcs_repo_plugin")
}

fn read_case(case_name: &str) -> String {
    let file_path = sample_dir().join(format!("{case_name}.log"));
    std::fs::read_to_string(&file_path)
        .unwrap_or_else(|err| panic!("读取样本失败 {}: {err}", file_path.display()))
}

// ============================================================================
// Case 100: repo sync — 命令锚点 + 进度噪音消除 + 项目哈希映射
// ============================================================================
#[test]
fn test_sync_case_100() {
    let raw = read_case("case_100_repo_sync");
    let compacted = compact_repo_status_for_ai(&raw);

    // 必须保留命令锚点
    assert!(compacted.starts_with("repo sync"), "必须保留命令锚点");

    // 进度噪音应被彻底消除
    assert!(
        !compacted.contains("Downloading"),
        "应消除 Downloading 进度条"
    );
    assert!(!compacted.contains("Syncing:"), "应消除 Syncing: 进度");
    assert!(!compacted.contains("Syncing done."), "应消除完成回显");

    // 项目与哈希映射：PRJ:<path> @<hash>
    assert!(
        compacted.contains("PRJ:platform/frameworks/base @abc123def"),
        "应映射项目路径与哈希"
    );
    assert!(
        compacted.contains("PRJ:platform/packages/apps/Camera @def456ghi"),
        "应映射第二个项目路径与哈希"
    );
}

// ============================================================================
// Case 116: repo status — 命令锚点 + 扁平化项目状态 + 文件修改映射
// ============================================================================
#[test]
fn test_status_case_116() {
    let raw = read_case("case_116_repo_status");
    let compacted = compact_repo_status_for_ai(&raw);

    // 必须保留命令锚点
    assert!(compacted.starts_with("repo status"), "必须保留命令锚点");

    // 项目与分支状态：PRJ:<path> BR:<branch> (<state>)
    assert!(
        compacted.contains("PRJ:platform/build/make BR:master (clean)"),
        "应输出 clean 状态的项目"
    );
    assert!(
        compacted.contains("PRJ:platform/frameworks/base BR:feature-xyz (clean)"),
        "应输出第二个 clean 项目"
    );
    assert!(
        compacted.contains("PRJ:platform/packages/apps/Settings BR:feature-xyz (dirty)"),
        "应输出 dirty 状态的项目"
    );
    assert!(
        compacted.contains("PRJ:vendor/partner/products/MyApp BR:main (clean)"),
        "应输出第四个项目"
    );

    // 压缩协议 V1 文件状态码映射：Modified→M, Added→A
    assert!(
        compacted.contains("M:src/SettingsActivity.java"),
        "Modified 应映射为 M"
    );
    assert!(
        compacted.contains("A:res/values/strings.xml"),
        "Added 应映射为 A"
    );
}

// ============================================================================
// Case 124: repo upload — 命令锚点 + SSH URL 消除 + 推送映射
// ============================================================================
#[test]
fn test_upload_case_124() {
    let raw = read_case("case_124_repo_upload");
    let compacted = compact_repo_status_for_ai(&raw);

    // 必须保留命令锚点
    assert!(compacted.starts_with("repo upload"), "必须保留命令锚点");

    // SSH URL 噪音应被彻底消除
    assert!(!compacted.contains("ssh://"), "应消除 SSH URL 噪音");

    // 推送映射：PRJ:<path>: HEAD -> refs/changes/...
    assert!(
        compacted.contains("PRJ:platform/frameworks/base: HEAD -> refs/changes/123/456/1"),
        "应输出第一个项目的推送映射"
    );
    assert!(
        compacted.contains("PRJ:platform/packages/apps/Settings: HEAD -> refs/changes/124/457/1"),
        "应输出第二个项目的推送映射"
    );

    // 汇总行应被消除
    assert!(!compacted.contains("projects uploaded"), "应消除推送汇总行");
}

// ============================================================================
// 短输入回退
// ============================================================================
#[test]
fn test_short_input_fallback() {
    let raw = "repo help";
    let compacted = compact_repo_status_for_ai(raw);
    assert_eq!(compacted, raw, "过短输入应直接返回原始文本");
}

// ============================================================================
// 噪音检测
// ============================================================================
#[test]
fn test_repo_noise_detection() {
    // 这些是 repo 专属噪音
    assert!(super::methods::is_repo_noise(
        "Downloading platform/frameworks/base: 45%"
    ));
    assert!(super::methods::is_repo_noise("Syncing: 100/120 projects"));
    assert!(super::methods::is_repo_noise("Syncing done."));
    assert!(super::methods::is_repo_noise("Listing projects ..."));
    assert!(super::methods::is_repo_noise("Staged changes in:"));
    assert!(super::methods::is_repo_noise(
        "Upload project: platform/frameworks/base/"
    ));
    assert!(super::methods::is_repo_noise(
        "repo initialized in /home/user/android"
    ));
    assert!(super::methods::is_repo_noise(
        "Your identity is: alice <alice@example.com>"
    ));
    assert!(super::methods::is_repo_noise(
        "will use a mirror located at /home/user/android/mirror"
    ));
    assert!(super::methods::is_repo_noise("repo: syncing..."));

    // 这些不是噪音
    assert!(!super::methods::is_repo_noise(
        "project platform/frameworks/base/ branch master"
    ));
    assert!(!super::methods::is_repo_noise(
        "platform/frameworks/base: abc1234 Initial commit"
    ));
    assert!(!super::methods::is_repo_noise("* master"));
    assert!(!super::methods::is_repo_noise(
        "Starting branch: feature-new"
    ));
    assert!(!super::methods::is_repo_noise(
        "Switched to branch 'feature-xyz' in platform/frameworks/base"
    ));
}

#[test]
fn test_start_case_119_preserves_success_confirmation() {
    let raw = read_case("case_119_repo_start");
    let compacted = compact_repo_status_for_ai(&raw);

    assert!(
        compacted.contains("repo start feature-new --platform/frameworks/base"),
        "repo start command must be retained: {compacted}"
    );
    assert!(
        compacted.contains("Starting branch: feature-new"),
        "repo start branch creation confirmation must be retained: {compacted}"
    );
    assert!(
        compacted.contains("Switched to branch 'feature-new' in platform/frameworks/base"),
        "repo start branch switch confirmation must be retained: {compacted}"
    );
}

// ============================================================================
// 异常映射
// ============================================================================
#[test]
fn test_repo_alert_mapping() {
    assert!(super::methods::map_repo_alert("CONFLICT: merge conflict").is_some());
    assert!(super::methods::map_repo_alert("error: something went wrong").is_some());
    assert!(super::methods::map_repo_alert("Push failed").is_some());
    assert!(super::methods::map_repo_alert("Push rejected").is_some());
    assert!(super::methods::map_repo_alert("master -> master").is_none());
}
