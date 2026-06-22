use super::methods::*;
use std::path::{Path, PathBuf};

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join("vcs_darcs_plugin")
}
fn read_case(c: &str) -> String {
    let p = sample_dir().join(format!("{c}.log"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取样本失败 {}: {e}", p.display()))
}

// ============================================================================
// Case 35: log — 命令锚点 + 提交信息未被丢弃
// ============================================================================
#[test]
fn test_log_case_35() {
    let c = compact_darcs_log_for_ai(&read_case("case_35_darcs_log"));
    assert!(c.starts_with("darcs log"));
    assert!(c.contains("darcs changes"));
    assert!(c.contains("OW:@alice.chen"), "应包含作者信息");
    assert!(
        c.contains("CM:feat(darcs): Support compact log normalization"),
        "提交描述不可丢弃"
    );
    assert!(c.contains("OW:@bob.wang"), "应包含第二个补丁的作者");
    assert!(c.contains("CR:2026-04-05"), "应包含日期信息");
}

// ============================================================================
// Case 42: status — 命令锚点 + A/M/R 状态码
// ============================================================================
#[test]
fn test_status_case_42() {
    let c = compact_darcs_status_for_ai(&read_case("case_42_darcs_status"));
    assert!(!c.is_empty());
    assert!(c.contains("A") || c.contains("M") || c.contains("R"));
}

// ============================================================================
// Case 210: obliterate — 命令锚点 + 交互式对话消除
// ============================================================================
#[test]
fn test_obliterate_case_210() {
    let c = compact_darcs_log_family_for_ai(&read_case("case_210_darcs_obliterate"));
    assert!(c.starts_with("darcs obliterate"), "必须保留命令锚点");
    assert!(!c.contains("(yes/no)"), "交互式对话 (yes/no) 必须被消除");
    assert!(!c.contains("About to delete"), "叙述性文本必须被消除");
    assert!(!c.contains("Really delete"), "确认对话必须被消除");
    assert!(
        c.contains("D:abc1234: Add feature A") || c.contains("D:  abc1234: Add feature A"),
        "保留删除的补丁记录"
    );
}

// ============================================================================
// Case 154: amend — 命令锚点 + 流向箭头
// ============================================================================
#[test]
fn test_amend_case_154() {
    let c = compact_darcs_log_family_for_ai(&read_case("case_154_darcs_amend"));
    assert!(c.starts_with("darcs amend"), "必须保留命令锚点");
    assert!(!c.contains("Amending patch"), "叙述性文本必须被消除");
    assert!(
        c.contains("AMEND:Add login feature->Add login with OAuth support")
            || c.contains("AMEND: Add login feature -> Add login with OAuth support"),
        "应使用 -> 表示 old->new 流向"
    );
}

// ============================================================================
// Case 282: rebase — 命令锚点 + 流向箭头
// ============================================================================
#[test]
fn test_rebase_case_282() {
    let c = compact_darcs_log_family_for_ai(&read_case("case_282_darcs_rebase"));
    assert!(c.starts_with("darcs rebase"), "必须保留命令锚点");
    assert!(!c.contains("Rebase in progress"), "进度描述必须被消除");
    assert!(
        c.contains("REBASE:patch-1->patch-4"),
        "应使用 -> 表示 rebase 流向"
    );
}

// ============================================================================
// Case 321: log summary — 补丁描述保留
// ============================================================================
#[test]
fn test_log_summary_case_321() {
    let c = compact_darcs_log_family_for_ai(&read_case("case_321_darcs_log_summary"));
    assert!(c.starts_with("darcs log"));
    assert!(
        c.contains("OW:@Alice Chen") || c.contains("OW:@alice"),
        "应保留作者"
    );
    assert!(c.contains("Refactor main entry point"), "提交描述不可丢弃");
    assert!(
        c.contains("Add configuration loader"),
        "第二个补丁描述不可丢弃"
    );
    assert!(
        c.contains("Initial project structure"),
        "第三个补丁描述不可丢弃"
    );
}

// ============================================================================
// 短输入 + 噪音检测
// ============================================================================
#[test]
fn test_short_input_fallback() {
    let c = compact_darcs_log_for_ai("darcs help");
    assert_eq!(c, "darcs help");
}

#[test]
fn test_darcs_noise_detection() {
    assert!(super::methods::is_darcs_noise("About to delete 3 patches"));
    assert!(super::methods::is_darcs_noise(
        "Really delete these patches? (yes/no): yes"
    ));
    assert!(super::methods::is_darcs_noise(
        "Recording patch \"Add feature\""
    ));
    assert!(super::methods::is_darcs_noise(
        "Patch applied successfully."
    ));
    assert!(super::methods::is_darcs_noise(
        "Rebase in progress: 3 patches suspended"
    ));
    assert!(super::methods::is_darcs_noise(
        "Amending patch for: feature-x"
    ));
    assert!(!super::methods::is_darcs_noise("Author: alice"));
}

#[test]
fn test_darcs_alert_mapping() {
    assert!(super::methods::map_darcs_alert("CONFLICT: merge conflict").is_some());
    assert!(super::methods::map_darcs_alert("error: something wrong").is_some());
    assert!(super::methods::map_darcs_alert("Author: alice").is_none());
}
