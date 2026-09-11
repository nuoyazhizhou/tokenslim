use super::methods::*;
use std::path::{Path, PathBuf};

// ============================================================================
// 样板辅助
// ============================================================================
/// 测试辅助：返回样例目录路径。
fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join("vcs_repo_plugin")
}

/// 测试辅助：读取指定 repo 样例文件。
fn read_case(case_name: &str) -> String {
    let file_path = sample_dir().join(format!("{case_name}.log"));
    std::fs::read_to_string(&file_path)
        .unwrap_or_else(|err| panic!("读取样本失败 {}: {err}", file_path.display()))
}

// ============================================================================
// Case 100: repo sync — 命令锚点 + 进度噪音消除 + 项目哈希映射
// ============================================================================
/// 测试：repo sync 样例（case 100）项目与哈希映射。
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
/// 测试：repo status 样例（case 116）项目状态扁平化。
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
/// 测试：repo upload 样例（case 124）推送映射。
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
/// 测试：短输入直接返回原文（不压缩）。
#[test]
fn test_short_input_fallback() {
    let raw = "repo help";
    let compacted = compact_repo_status_for_ai(raw);
    assert_eq!(compacted, raw, "过短输入应直接返回原始文本");
}

// ============================================================================
// 噪音检测
// ============================================================================
/// 测试：repo 噪音行检测。
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

/// 测试：repo start 样例（case 119）保留成功确认。
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
/// 测试：repo 警报行映射。
#[test]
fn test_repo_alert_mapping() {
    assert!(super::methods::map_repo_alert("CONFLICT: merge conflict").is_some());
    assert!(super::methods::map_repo_alert("error: something went wrong").is_some());
    assert!(super::methods::map_repo_alert("Push failed").is_some());
    assert!(super::methods::map_repo_alert("Push rejected").is_some());
    assert!(super::methods::map_repo_alert("master -> master").is_none());
}

// ============================================================================
// generic fallback 族（list/branches/diff/checkout/forall/stage/init）
// 这些子命令经 compact_repo_other_for_ai → compact_repo_log_for_ai 落到
// compact_repo_generic，统一走"锚点 + 噪音过滤 + diff 压缩 + 警报映射"。
// ============================================================================
/// 测试：repo list 样例（case 115）消除 "Listing projects" 叙述噪音。
#[test]
fn test_list_case_115() {
    let raw = read_case("case_115_repo_list");
    let compacted = compact_repo_other_for_ai(&raw);

    // 必须保留命令锚点
    assert!(compacted.starts_with("repo list"), "必须保留命令锚点");

    // 叙述性噪音应被消除
    assert!(
        !compacted.contains("Listing projects"),
        "应消除列表叙述噪音"
    );

    // 项目清单应原样保留
    assert!(
        compacted.contains("platform/build/make: platform/build/make"),
        "应保留第一个项目"
    );
    assert!(
        compacted.contains("platform/frameworks/base: platform/frameworks/base"),
        "应保留第二个项目"
    );
    assert!(
        compacted.contains("vendor/partner/products/MyApp: vendor/partner/products/MyApp"),
        "应保留第四个项目"
    );
}

/// 测试：repo branches 样例（case 117）本地/远端分支列表保留。
#[test]
fn test_branches_case_117() {
    let raw = read_case("case_117_repo_branches");
    let compacted = compact_repo_other_for_ai(&raw);

    // 必须保留命令锚点
    assert!(compacted.starts_with("repo branches"), "必须保留命令锚点");

    // 本地分支与选中标记保留
    assert!(compacted.contains("* master"), "应保留当前分支标记");
    assert!(compacted.contains("feature-auth"), "应保留本地分支");
    assert!(compacted.contains("feature-api"), "应保留第三个本地分支");

    // 远端跟踪分支保留
    assert!(
        compacted.contains("remotes/origin/HEAD -> origin/master"),
        "应保留远端 HEAD 映射"
    );
    assert!(
        compacted.contains("remotes/origin/feature-ui"),
        "应保留远端分支"
    );
}

/// 测试：repo diff 样例（case 118）diff 与 hunk 头压缩、上下文行映射。
#[test]
fn test_diff_case_118() {
    let raw = read_case("case_118_repo_diff");
    let compacted = compact_repo_other_for_ai(&raw);

    // 必须保留命令锚点
    assert!(compacted.starts_with("repo diff"), "必须保留命令锚点");

    // diff --git → D:file
    assert!(
        compacted.contains("D:src/Activity.java"),
        "diff --git 头应压缩为 D:file"
    );

    // hunk 头 → @@a->b@@
    assert!(
        compacted.contains("@@-10,7->+10,7@@"),
        "hunk 头应压缩为 @@a->b@@"
    );

    // index / --- / +++ 元数据行应被消除
    assert!(!compacted.contains("index 1234567"), "应消除 index 行");
    assert!(!compacted.contains("--- a/"), "应消除 --- 行");
    assert!(!compacted.contains("+++ b/"), "应消除 +++ 行");

    // 增删行应保留语义、去除行内前导空白
    assert!(
        compacted.contains(r#"-Log.d(TAG, "old log");"#),
        "删除行应保留"
    );
    assert!(
        compacted.contains(r#"+Log.d(TAG, "new log");"#),
        "新增行应保留"
    );
}

/// 测试：repo checkout 样例（case 120）保留切换分支成功确认。
#[test]
fn test_checkout_case_120() {
    let raw = read_case("case_120_repo_checkout");
    let compacted = compact_repo_other_for_ai(&raw);

    // 必须保留命令锚点
    assert!(compacted.starts_with("repo checkout"), "必须保留命令锚点");
    assert!(
        compacted
            .contains("Switched to branch 'feature-existing' in platform/packages/apps/Settings"),
        "应保留分支切换成功确认"
    );
}

/// 测试：repo forall 样例（case 121）逐项目命令回显保留。
#[test]
fn test_forall_case_121() {
    let raw = read_case("case_121_repo_forall");
    let compacted = compact_repo_other_for_ai(&raw);

    // 必须保留命令锚点
    assert!(compacted.starts_with("repo forall"), "必须保留命令锚点");

    // 各项目回显保留：path: hash subject
    assert!(
        compacted.contains("platform/build/make: abc1234 Initial commit"),
        "应保留第一个项目回显"
    );
    assert!(
        compacted.contains("platform/frameworks/base: def5678 Add core framework"),
        "应保留第二个项目回显"
    );
    assert!(
        compacted.contains("platform/packages/apps/Settings: ghi9012 Update settings UI"),
        "应保留第三个项目回显"
    );
}

/// 测试：repo stage 样例（case 122）消除 "Staged changes in:" 叙述噪音。
#[test]
fn test_stage_case_122() {
    let raw = read_case("case_122_repo_stage");
    let compacted = compact_repo_other_for_ai(&raw);

    // 必须保留命令锚点
    assert!(compacted.starts_with("repo stage"), "必须保留命令锚点");

    // 叙述噪音消除
    assert!(
        !compacted.contains("Staged changes in"),
        "应消除暂存叙述噪音"
    );

    // 各项目文件计数保留
    assert!(
        compacted.contains("platform/frameworks/base: 3 files"),
        "应保留项目计数"
    );
    assert!(
        compacted.contains("platform/packages/apps/Settings: 2 files"),
        "应保留第二个项目计数"
    );
}

/// 测试：repo init 样例（case 123）消除 repo: 环境叙述噪音，保留初始化 URL。
#[test]
fn test_init_case_123() {
    let raw = read_case("case_123_repo_init");
    let compacted = compact_repo_other_for_ai(&raw);

    // 必须保留命令锚点
    assert!(compacted.starts_with("repo init"), "必须保留命令锚点");

    // repo: 前缀环境叙述应被消除
    assert!(!compacted.contains("repo initialized"), "应消除初始化回显");
    assert!(!compacted.contains("Your identity"), "应消除身份叙述");
    assert!(!compacted.contains("will use a mirror"), "应消除镜像叙述");
    assert!(!compacted.contains("syncing"), "应消除同步叙述");

    // manifest URL 参数行保留
    assert!(
        compacted
            .contains("repo init -u https://android.googlesource.com/platform/manifest -b main"),
        "应保留 manifest 初始化参数"
    );
}
