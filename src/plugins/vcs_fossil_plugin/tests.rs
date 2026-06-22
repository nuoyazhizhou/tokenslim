use super::methods::*;
use std::path::{Path, PathBuf};

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join("vcs_fossil_plugin")
}
fn read_case(c: &str) -> String {
    let p = sample_dir().join(format!("{c}.log"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取样本失败 {}: {e}", p.display()))
}

// ============================================================================
// Case 29: status — 命令锚点 + 状态码 M: 映射
// ============================================================================
#[test]
fn test_status_case_29() {
    let c = compact_fossil_status_for_ai(&read_case("case_29_fossil_status"));
    assert!(c.starts_with("fossil status"), "必须保留命令锚点");
    assert!(!c.contains("repository:"), "应消除 repository 元数据");
    assert!(!c.contains("local-root:"), "应消除 local-root 元数据");
    assert!(!c.contains("checkout:"), "应消除 checkout 元数据");
    assert!(
        c.contains("ST:M src/plugins/vcs_plugin/methods.rs"),
        "EDITED 应映射为 ST:M"
    );
    assert!(
        c.contains("ST:M src/plugins/vcs_plugin/parser.rs"),
        "第二个 EDITED 应映射为 ST:M"
    );
    assert!(
        c.contains("ST:A samples/vcs/case_29_fossil_status.log"),
        "ADDED 应映射为 ST:A"
    );
}

// ============================================================================
// Case 152: changes — 状态码 M/A/D 映射 + 锚点
// ============================================================================
#[test]
fn test_changes_case_152() {
    let c = compact_fossil_status_for_ai(&read_case("case_152_fossil_changes"));
    assert!(c.starts_with("fossil changes"), "必须保留命令锚点");
    assert!(c.contains("ST:A src/newfile.py"), "ADDED 应映射为 ST:A");
    assert!(c.contains("ST:M src/existing.py"), "EDITED 应映射为 ST:M");
    assert!(c.contains("ST:D src/oldfile.txt"), "DELETED 应映射为 ST:D");
}

// ============================================================================
// Case 104: timeline — 作者/哈希符号化
// ============================================================================
#[test]
fn test_timeline_case_104() {
    let c = compact_fossil_log_for_ai(&read_case("case_104_fossil_timeline"));
    assert!(c.starts_with("fossil timeline"), "必须保留命令锚点");
    assert!(c.contains("@1abc123"), "哈希应带 @ 前缀");
    assert!(c.contains("OW:@alice"), "作者应带 OW:@ 前缀");
    assert!(c.contains("Add new feature"), "提交描述不可丢失");
    assert!(c.contains("@2def456"), "第二个哈希应带 @ 前缀");
    assert!(c.contains("OW:@bob"), "第二个作者应带 OW:@ 前缀");
}

// ============================================================================
// Case 153: undo — 抹除废话，REVERT:
// ============================================================================
#[test]
fn test_undo_case_153() {
    let c = compact_fossil_log_for_ai(&read_case("case_153_fossil_undo"));
    assert!(c.starts_with("fossil undo"), "必须保留命令锚点");
    assert!(!c.contains("Undo successful"), "叙述性废话应被清除");
    assert!(c.contains("REVERT:@abc123"), "应输出 REVERT:@abc123");
}

// ============================================================================
// Case 196: sync — 抹除 Done. 和 Sync with...
// ============================================================================
#[test]
fn test_sync_case_196() {
    let c = compact_fossil_log_for_ai(&read_case("case_196_fossil_sync"));
    assert!(c.starts_with("fossil sync"), "必须保留命令锚点");
    assert!(!c.contains("Done."), "Done. 废话应被清除");
    assert!(!c.contains("Sync with"), "Sync with 废话应被清除");
    assert!(c.contains("Pull:"), "应保留 Pull: 信息");
    assert!(c.contains("Push:"), "应保留 Push: 信息");
}

// ============================================================================
// 短输入回退 + 报警检测
// ============================================================================
#[test]
fn test_short_input_fallback() {
    let c = compact_fossil_log_for_ai("fossil help");
    assert_eq!(c, "fossil help");
}

#[test]
fn test_fossil_alert_mapping() {
    assert!(super::methods::map_fossil_alert("CONFLICT src/file.rs").is_some());
    assert!(super::methods::map_fossil_alert("error: something wrong").is_some());
    assert!(super::methods::map_fossil_alert("EDITED src/file.rs").is_none());
}
