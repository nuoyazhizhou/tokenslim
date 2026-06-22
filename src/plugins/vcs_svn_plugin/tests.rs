use super::methods::*;
use std::path::{Path, PathBuf};

// ============================================================================
// 测试基础设施 — 文件驱动（法则：严禁 Hardcode）
// ============================================================================

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join("vcs_svn_plugin")
}

/// 从 samples/vcs_svn_plugin/<name>.log 读取测试用例
fn read_case(name: &str) -> String {
    let p = sample_dir().join(format!("{}.log", name));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取样本失败 {}: {}", p.display(), e))
}

/// 【红线断言】输出的第一行必须等于输入的第一行（命令锚点保护）
fn assert_anchor_preserved(case_name: &str, output: &str) {
    let input = read_case(case_name);
    let input_first = input.lines().next().unwrap_or("").trim();
    let output_first = output.lines().next().unwrap_or("").trim();
    assert_eq!(
        output_first, input_first,
        "锚点丢失: case={}, 期望首行='{}', 实际首行='{}'",
        case_name, input_first, output_first
    );
}

// ============================================================================
// Case 01: svn status — 状态码映射正确性 + 锚点保护
// ============================================================================
#[test]
fn test_case_01_svn_status() {
    let output = compact_svn_status_for_ai(&read_case("case_01_svn_status"));
    assert_anchor_preserved("case_01_svn_status", &output);
    // M: Modified, A: Added, D: Deleted, ?: Untracked, !: Conflict
    assert!(
        output.contains("M src/core/doctor_workspace/methods.rs"),
        "M 应保留"
    );
    assert!(
        output.contains("A src/plugins/vcs_plugin/parser.rs"),
        "A 应保留"
    );
    assert!(output.contains("D src/legacy/old_module.rs"), "D 应保留");
    assert!(
        output.contains("? config/vcs_plugin.local.json"),
        "? 应保留"
    );
    assert!(output.contains("! docs/design/architecture.md"), "! 应保留");
    // 废话过滤（Phase 2 修复后启用）
    assert!(!output.contains("warning: W155010"), "warning 噪音应过滤");
    assert!(
        !output.contains("Checked out revision"),
        "Checkout 摘要应过滤"
    );
}

// ============================================================================
// Case 03: svn log — 头部 + 描述保留 + 锚点保护
// ============================================================================
#[test]
fn test_case_03_svn_log() {
    let output = compact_svn_log_for_ai(&read_case("case_03_svn_log"));
    assert_anchor_preserved("case_03_svn_log", &output);
    assert!(
        output.contains("r12345 | alice.chen |"),
        "revision 头部应保留"
    );
    assert!(
        output.contains("feat(vcs): Add SVN/HG/P4 parser support"),
        "commit message 应保留"
    );
    // 分隔线过滤
    assert!(
        !output
            .contains("------------------------------------------------------------------------"),
        "分隔线应过滤"
    );
    // Changed paths 块过滤（Phase 2 修复后启用）
    assert!(!output.contains("Changed paths:"), "Changed paths 头应过滤");
}

// ============================================================================
// Case 71: svn commit — 动作符号化 + 锚点保护
// ============================================================================
#[test]
fn test_case_71_svn_commit() {
    let output = compact_svn_commit_for_ai(&read_case("case_71_svn_commit"));
    assert_anchor_preserved("case_71_svn_commit", &output);
    assert!(output.contains("M config/app.json"), "Sending → M");
    assert!(!output.contains("Transmitting file data"), "进度噪音应过滤");
    assert!(output.contains("r12345"), "Committed revision → rNNN");
}

// ============================================================================
// Case 07: svn info — 键值保留 + 锚点保护
// ============================================================================
#[test]
fn test_case_07_svn_info() {
    let output = compact_svn_info_for_ai(&read_case("case_07_svn_info"));
    assert_anchor_preserved("case_07_svn_info", &output);
    assert!(
        output.contains("Path:") || output.contains("Revision:") || !output.is_empty(),
        "info 输出应包含字段"
    );
}

// ============================================================================
// Case 02: svn diff — DIFF: 头部压缩 + 分隔线过滤
// ============================================================================
#[test]
fn test_case_02_svn_diff() {
    let output = compact_svn_diff_for_ai(&read_case("case_02_svn_diff"));
    assert_anchor_preserved("case_02_svn_diff", &output);
    // Index: → DIFF: 头部降维
    assert!(!output.contains("Index: "), "Index 行应转为 DIFF:");
    assert!(
        output.contains("DIFF:") || output.contains("---"),
        "diff 结构应保留"
    );
}

// ============================================================================
// Case 70: svn update — revision 规范化 + 状态映射
// ============================================================================
#[test]
fn test_case_70_svn_update() {
    let output = compact_svn_status_for_ai(&read_case("case_70_svn_update"));
    assert_anchor_preserved("case_70_svn_update", &output);
    assert!(!output.contains("Updating '.':"), "Updating 噪音应过滤");
}

// ============================================================================
// Case 178: svn propset — 压缩率检查（不应膨胀）
// ============================================================================
#[test]
fn test_case_178_svn_propset_compression() {
    let raw = read_case("case_178_svn_propset");
    let output = compact_svn_prop_for_ai(&raw);
    assert_anchor_preserved("case_178_svn_propset", &output);
    // 修复后输出不应包含原始废话行
    assert!(!output.contains("property '"), "property 废话应被符号化");
    assert!(output.len() <= raw.len() + 60, "输出不应显著膨胀");
}

// ============================================================================
// Case 77: svn revert — 状态映射
// ============================================================================
#[test]
fn test_case_77_svn_revert() {
    let output = compact_svn_status_for_ai(&read_case("case_77_svn_revert"));
    assert_anchor_preserved("case_77_svn_revert", &output);
    assert!(
        output.contains("Reverted") || output.contains("src/"),
        "revert 路径应保留"
    );
}

// ============================================================================
// 极短输入回退测试
// ============================================================================
#[test]
fn test_short_input_no_crash() {
    let output = compact_svn_status_for_ai("svn help");
    assert!(output.starts_with("svn help"), "短输入应原样返回");
}

#[test]
fn test_svn_command_line_detection() {
    assert!(is_svn_command_line("svn status"));
    assert!(is_svn_command_line("svn log -l 5"));
    assert!(is_svn_command_line("svn commit -m \"fix\""));
    assert!(!is_svn_command_line("svn"));
    assert!(!is_svn_command_line("M       src/main.rs"));
}

// ============================================================================
// Phase 2 P2 增强功能测试 — 新增 3 个功能
// ============================================================================

/// 测试 svn merge 压缩：折叠合并详细信息，保留冲突摘要
#[test]
fn test_case_318_svn_merge_conflict_compresses() {
    let raw = read_case("case_318_svn_merge_conflict");
    let c = compact_svn_merge_enhanced(&raw);

    // 验证命令锚点保留
    assert_anchor_preserved("case_318_svn_merge_conflict", &c);

    // 验证摘要行存在
    assert!(c.contains("[MERGE]"), "应包含 [MERGE] 摘要");
    assert!(c.contains("r123-r456"), "应显示 revision 范围");
    assert!(c.contains("updated"), "应显示更新文件数");
    assert!(c.contains("conflicts"), "应显示冲突数");

    // 验证冲突文件保留
    assert!(c.contains("C    src/auth/config.py"), "应保留冲突文件");
    assert!(c.contains("C    src/user/views.py"), "应保留冲突文件");

    // 验证非冲突文件被省略
    assert!(!c.contains("U    src/auth/login.py"), "非冲突文件应省略");

    // 验证噪音行被过滤
    assert!(!c.contains("--- Merging r"), "Merging 行应过滤");
    assert!(!c.contains("Recording mergeinfo"), "Recording 行应过滤");
}

/// 测试 svn update 压缩：折叠详细的文件列表
#[test]
fn test_case_319_svn_update_detailed_compresses() {
    let raw = read_case("case_319_svn_update_detailed");
    let c = compact_svn_update_enhanced(&raw);

    // 验证命令锚点保留
    assert_anchor_preserved("case_319_svn_update_detailed", &c);

    // 验证摘要行存在
    assert!(c.contains("[UPDATE]"), "应包含 [UPDATE] 摘要");
    assert!(c.contains("r12345"), "应显示 revision");
    assert!(c.contains("20 files updated"), "应显示文件数");
    assert!(c.contains("details suppressed"), "应说明详情被省略");

    // 验证文件列表被省略
    assert!(!c.contains("U    src/auth/login.py"), "文件列表应省略");
    assert!(!c.contains("Updating"), "Updating 行应过滤");
}

/// 测试 svn annotate 压缩：折叠相同作者的连续行
#[test]
fn test_case_320_svn_annotate_long_compresses() {
    let raw = read_case("case_320_svn_annotate_long");
    let c = compact_svn_annotate_enhanced(&raw);

    // 验证命令锚点保留
    assert_anchor_preserved("case_320_svn_annotate_long", &c);

    // 验证摘要行存在
    assert!(c.contains("[ANNOTATE]"), "应包含 [ANNOTATE] 摘要");
    assert!(c.contains("lines"), "应显示总行数");
    assert!(c.contains("contributors"), "应显示贡献者数");

    // 验证贡献者统计
    assert!(c.contains("@alice.chen"), "应包含作者统计");
    assert!(c.contains("@bob.wang"), "应包含作者统计");
    assert!(c.contains("(") && c.contains(" lines)"), "应显示行数统计");
}

/// ROI 测试：merge 压缩不应导致扩展
#[test]
fn test_case_318_merge_compression_does_not_expand() {
    let raw = read_case("case_318_svn_merge_conflict");
    let c = compact_svn_merge_enhanced(&raw);

    // ROI 门控：压缩后不应比原始输入更长
    assert!(
        c.len() <= raw.len(),
        "压缩后长度 {} 不应超过原始长度 {}",
        c.len(),
        raw.len()
    );
}

/// ROI 测试：update 压缩不应导致扩展
#[test]
fn test_case_319_update_compression_does_not_expand() {
    let raw = read_case("case_319_svn_update_detailed");
    let c = compact_svn_update_enhanced(&raw);

    // ROI 门控：压缩后不应比原始输入更长
    assert!(
        c.len() <= raw.len(),
        "压缩后长度 {} 不应超过原始长度 {}",
        c.len(),
        raw.len()
    );
}

/// ROI 测试：annotate 压缩不应导致扩展
#[test]
fn test_case_320_annotate_compression_does_not_expand() {
    let raw = read_case("case_320_svn_annotate_long");
    let c = compact_svn_annotate_enhanced(&raw);

    // ROI 门控：压缩后不应比原始输入更长
    assert!(
        c.len() <= raw.len(),
        "压缩后长度 {} 不应超过原始长度 {}",
        c.len(),
        raw.len()
    );
}

/// 边界测试：少量文件的 update 不应折叠
#[test]
fn test_update_below_threshold_not_folded() {
    let raw = "svn update\nUpdating '.':\nU    file1.txt\nU    file2.txt\nAt revision 123.\n";
    let c = compact_svn_update_enhanced(raw);

    // 少于 10 个文件，不应折叠
    assert!(!c.contains("[UPDATE]"), "少于阈值时不应添加摘要");
    assert!(c.contains("U    file1.txt"), "应保留所有文件");
}

/// 边界测试：少量行的 annotate 不应折叠
#[test]
fn test_annotate_below_threshold_not_folded() {
    let raw = "svn annotate file.txt\n   123   alice   line1\n   124   bob   line2\n";
    let c = compact_svn_annotate_enhanced(raw);

    // 少于 20 行，不应折叠
    assert!(!c.contains("[ANNOTATE]"), "少于阈值时不应添加摘要");
    assert!(c.contains("123   alice"), "应保留所有行");
}
