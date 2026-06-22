use super::methods::*;
use std::path::{Path, PathBuf};

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join("vcs_cvs_plugin")
}
fn read_case(c: &str) -> String {
    let p = sample_dir().join(format!("{c}.log"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取样本失败 {}: {e}", p.display()))
}

// ============================================================================
// Case 101: update — 命令锚点 + Updating 废话消除 + U/A/R 映射
// ============================================================================
#[test]
fn test_update_case_101() {
    let c = compact_cvs_status_for_ai(&read_case("case_101_cvs_update"));
    assert!(c.starts_with("cvs update"), "必须保留命令锚点");
    assert!(!c.contains("Updating"), "Updating 废话应被消除");
    assert!(!c.contains("files updated"), "files updated 废话应被消除");
    assert!(c.contains("U src/main.rs"), "U 应映射为 U");
    assert!(c.contains("A new_file.py"), "A 应映射为 A");
    assert!(c.contains("R old_file.txt"), "R 应映射为 R");
}

// ============================================================================
// Case 314: update -d — 多层 Updating 消除
// ============================================================================
#[test]
fn test_update_d_case_314() {
    let c = compact_cvs_status_for_ai(&read_case("case_314_cvs_update_d"));
    assert!(c.starts_with("cvs update"), "必须保留命令锚点");
    assert!(!c.contains("Updating"), "所有 Updating 废话应被消除");
    assert!(!c.contains("New directory"), "New directory 应被消除");
    assert!(c.contains("U src/main.rs"), "应保留状态文件");
    assert!(c.contains("? docs/api_reference.md"), "? 应映射为 ?");
}

// ============================================================================
// Case 315: status -v — 命令锚点 + 分隔线消除 + KV 扁平化
// ============================================================================
#[test]
fn test_status_v_case_315() {
    let c = compact_cvs_status_for_ai(&read_case("case_315_cvs_status_v"));
    assert!(c.starts_with("cvs status"), "必须保留命令锚点");
    assert!(!c.contains("========"), "分隔线应被消除");
    assert!(c.contains("File: main.rs"), "应保留文件信息");
    assert!(c.contains("Status: M"), "应保留状态信息");
    assert!(c.contains("WR: 1.6"), "WR 应压缩 Working revision");
}

#[test]
fn test_status_case_36_roi_guard() {
    let raw = read_case("case_36_cvs_status");
    let c = compact_cvs_status_for_ai(&raw);
    assert!(c.starts_with("cvs status"));
    assert!(
        c.len() <= raw.len(),
        "ROI guard must prevent short status expansion"
    );
    assert!(c.contains("src/plugins/vcs_plugin/methods.rs"));
}

// ============================================================================
// Case 102: commit — 命令锚点 + CM: 映射
// ============================================================================
#[test]
fn test_commit_case_102() {
    let c = compact_cvs_log_family_for_ai(&read_case("case_102_cvs_commit"));
    assert!(c.starts_with("cvs commit"), "必须保留命令锚点");
    assert!(!c.contains("Checking in"), "Checking in 废话应被消除");
    assert!(
        c.contains("CM:src/main.rs@1.2 initial version"),
        "提交应映射为 CM:file@rev message"
    );
    assert!(
        c.contains("CM:config/app.json@1.3 Add new feature"),
        "第二个提交应保留"
    );
}

// ============================================================================
// Case 146: tag — 命令锚点 + T: 映射
// ============================================================================
#[test]
fn test_tag_case_146() {
    let c = compact_cvs_log_family_for_ai(&read_case("case_146_cvs_tag"));
    assert!(c.starts_with("cvs tag"), "必须保留命令锚点");
    assert!(!c.contains("Tagging"), "Tagging 废话应被消除");
    assert!(c.contains("ST:T src/main.java"), "T 应映射为 ST:T");
    assert!(c.contains("ST:T src/utils.java"), "第二个 T 应保留");
}

#[test]
fn test_history_case_190_roi_guard() {
    let raw = read_case("case_190_cvs_history");
    let c = compact_cvs_log_family_for_ai(&raw);
    assert!(c.starts_with("cvs history"));
    assert!(
        c.len() <= raw.len(),
        "ROI guard must prevent short history expansion"
    );
    assert!(c.contains("Resync point"));
}

// ============================================================================
// 短输入 + 报警
// ============================================================================
#[test]
fn test_short_input_fallback() {
    let c = compact_cvs_log_for_ai("cvs help");
    assert_eq!(c, "cvs help");
}

#[test]
fn test_cvs_alert_mapping() {
    assert!(super::methods::map_cvs_alert("CONFLICT src/main.c").is_some());
    assert!(super::methods::map_cvs_alert("error: something wrong").is_some());
    assert!(super::methods::map_cvs_alert("U src/main.c").is_none());
}
