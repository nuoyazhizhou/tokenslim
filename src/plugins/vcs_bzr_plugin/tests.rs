use super::methods::*;
use std::path::{Path, PathBuf};

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join("vcs_bzr_plugin")
}
fn read_case(c: &str) -> String {
    let p = sample_dir().join(format!("{c}.log"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取样本失败 {}: {e}", p.display()))
}

// ============================================================================
// Case 38: status — 命令锚点 + M:/A:/D:/?: 映射
// ============================================================================
#[test]
fn test_status_case_38() {
    let c = compact_bzr_status_for_ai(&read_case("case_38_bzr_status"));
    assert!(c.starts_with("bzr status"), "必须保留命令锚点");
    assert!(
        c.contains("M src/plugins/vcs_plugin/methods.rs"),
        "modified 应映射为 M"
    );
    assert!(
        c.contains("A samples/vcs/case_33_bzr_status.log"),
        "added 应映射为 A"
    );
    assert!(
        c.contains("D src/legacy/old_bzr_adapter.rs"),
        "removed 应映射为 D"
    );
    assert!(c.contains("? tmp/bzr-note.txt"), "unknown 应映射为 ?");
}

// ============================================================================
// Case 39: log — 命令锚点 + CM: 提交信息不可丢弃
// ============================================================================
#[test]
fn test_log_case_39() {
    let c = compact_bzr_log_for_ai(&read_case("case_39_bzr_log"));
    assert!(c.starts_with("bzr log"), "必须保留命令锚点");
    assert!(c.contains("r203"), "应包含 revision r203");
    assert!(c.contains("OW:@alice.chen"), "作者应映射为 OW:@alice.chen");
    assert!(
        c.contains("CM:feat(bzr): add status/log/diff showcase coverage"),
        "提交信息不可丢弃且应带 CM: 前缀"
    );
    assert!(c.contains("r202"), "应包含第二个 revision");
    assert!(c.contains("OW:@bob.wang"), "第二个作者应带 OW:@");
    assert!(
        c.contains("CM:fix(bzr): normalize timestamp"),
        "第二个提交信息不可丢弃"
    );
}

// ============================================================================
// Case 148: push — 命令锚点 + 帮助废话消除
// ============================================================================
#[test]
fn test_push_case_148() {
    let c = compact_bzr_log_family_for_ai(&read_case("case_148_bzr_push"));
    assert!(c.starts_with("bzr push"), "必须保留命令锚点");
    assert!(!c.contains("To push to a branch"), "帮助废话应被消除");
    assert!(c.contains("No remote branch"), "核心信息应保留");
}

// ============================================================================
// Case 149: merge — 命令锚点 + 成功提示消除
// ============================================================================
#[test]
fn test_merge_case_149() {
    let c = compact_bzr_log_family_for_ai(&read_case("case_149_bzr_merge"));
    assert!(c.starts_with("bzr merge"), "必须保留命令锚点");
    assert!(!c.contains("All changes applied"), "成功提示废话应被清除");
    assert!(c.contains("Merging from:"), "核心 merge 信息应保留");
}

// ============================================================================
// Case 192: revert — 命令锚点 + REVERT: 映射
// ============================================================================
#[test]
fn test_revert_case_192() {
    let c = compact_bzr_status_for_ai(&read_case("case_192_bzr_revert"));
    assert!(c.starts_with("bzr revert"), "必须保留命令锚点");
    assert!(c.contains("ST:R src/main.java"), "reverted 应映射为 ST:R");
}

// ============================================================================
// Case 319: status short — 短格式 M:/A: 映射
// ============================================================================
#[test]
fn test_status_short_case_319() {
    let c = compact_bzr_status_for_ai(&read_case("case_319_bzr_status_short"));
    assert!(c.starts_with("bzr status"), "必须保留命令锚点");
    assert!(c.contains("src/main.rs"), "M 路径应保留");
    assert!(c.contains("src/auth/login.rs"), "+A 路径应保留");
    assert!(c.contains("tests/smoke_test.rs"), "? 路径应保留");
}

// ============================================================================
// 短输入回退 + 报警检测
// ============================================================================
#[test]
fn test_short_input_fallback() {
    let c = compact_bzr_log_for_ai("bzr help");
    assert_eq!(c, "bzr help");
}

#[test]
fn test_bzr_alert_mapping() {
    assert!(super::methods::map_bzr_alert("CONFLICT: merge conflict").is_some());
    assert!(super::methods::map_bzr_alert("error: something wrong").is_some());
    assert!(super::methods::map_bzr_alert("modified: src/file.rs").is_none());
}
