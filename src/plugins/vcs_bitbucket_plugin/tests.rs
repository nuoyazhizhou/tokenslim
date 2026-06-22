use super::methods::*;
use std::path::{Path, PathBuf};

// ============================================================================
// 样板辅助
// ============================================================================
fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join("vcs_bitbucket_plugin")
}

fn read_case(case_name: &str) -> String {
    let file_path = sample_dir().join(format!("{case_name}.log"));
    std::fs::read_to_string(&file_path)
        .unwrap_or_else(|err| panic!("读取样本失败 {}: {err}", file_path.display()))
}

// ============================================================================
// Case 113: pr list — 命令锚点 + 表头消除 + 行列符号化
// ============================================================================
#[test]
fn test_pr_list_case_113() {
    let raw = read_case("case_113_bitbucket_pr_list");
    let compacted = compact_bitbucket_log_for_ai(&raw);

    // 必须保留命令锚点
    assert!(
        compacted.starts_with("bitbucket pr list"),
        "必须保留命令锚点"
    );

    // 表头应被彻底消除
    assert!(!compacted.contains("TYPE"), "应消除表头 TYPE 列");
    assert!(!compacted.contains("AUTHOR"), "应消除表头 AUTHOR 列");

    // 分隔线应被彻底消除
    assert!(!compacted.contains("---"), "应消除分隔线");

    // 数据行符号化：#<ID> ST:<STATE> OW:@<author> <title>
    assert!(
        compacted.contains("#123 ST:OPEN OW:@alice Fix authentication bug"),
        "第一行应包含 #123 ST:OPEN OW:@alice 和标题"
    );
    assert!(
        compacted.contains("#124 ST:OPEN OW:@bob Add new dashboard"),
        "第二行应包含 #124 ST:OPEN OW:@bob 和标题"
    );
    assert!(
        compacted.contains("#122 ST:MERGED OW:@charlie Update dependencies"),
        "第三行应包含 #122 ST:MERGED OW:@charlie 和标题"
    );
}

// ============================================================================
// Case 114: pr view — 命令锚点 + K-V 扁平化 + DESC 保留
// ============================================================================
#[test]
fn test_pr_view_case_114() {
    let raw = read_case("case_114_bitbucket_pr_view");
    let compacted = compact_bitbucket_log_for_ai(&raw);

    // 必须保留命令锚点
    assert!(
        compacted.starts_with("bitbucket pr view 125"),
        "必须保留命令锚点"
    );

    // 分隔线和冗余标题应被消除
    assert!(!compacted.contains("==="), "应消除分隔线");
    assert!(!compacted.contains("Pull request #"), "应消除冗余标题行");

    // 元数据符号化：ST:OPEN OW:@alice RV:bob,charlie BR:feature-refactor->main
    assert!(compacted.contains("ST:OPEN"), "State 应映射为 ST:OPEN");
    assert!(compacted.contains("OW:@alice"), "Author 应映射为 OW:@alice");
    assert!(
        compacted.contains("RV:bob,charlie"),
        "Reviewers 应映射为 RV:bob,charlie（去除 (approved) 后缀）"
    );
    assert!(
        compacted.contains("BR:feature-refactor->main"),
        "Source 应映射为 BR:feature-refactor->main"
    );

    // Description 必须保留完整文本
    assert!(
        compacted.contains("DESC: This PR refactors the codebase to improve maintainability."),
        "DESC 必须保留完整描述且不被篡改"
    );

    // 噪音行应被消除
    assert!(!compacted.contains("Created:"), "应消除 Created 行");
    assert!(
        !compacted.contains("Participants:"),
        "应消除 Participants 行"
    );
}

// ============================================================================
// Case 207: pr create — 命令锚点 + URL 消除 + SRC 映射
// ============================================================================
#[test]
fn test_pr_create_case_207() {
    let raw = read_case("case_207_bitbucket_pr_create");
    let compacted = compact_bitbucket_log_for_ai(&raw);

    // 必须保留命令锚点
    assert!(
        compacted.starts_with("bitbucket pr create"),
        "必须保留命令锚点"
    );

    // URL 应被彻底消除
    assert!(!compacted.contains("bitbucket.org"), "应消除 Bitbucket URL");

    // Source 映射
    assert!(
        compacted.contains("SRC:feature-auth->main"),
        "Source 应映射为 SRC:feature-auth->main"
    );

    // 创建结果
    assert!(
        compacted.contains("Created PR #125"),
        "应保留创建结果并去除 ✓ 前缀"
    );
}

// ============================================================================
// Case 208: issue list — 命令锚点 + 表头消除 + 行列符号化
// ============================================================================
#[test]
fn test_issue_list_case_208() {
    let raw = read_case("case_208_bitbucket_issue_list");
    let compacted = compact_bitbucket_log_for_ai(&raw);

    // 必须保留命令锚点
    assert!(
        compacted.starts_with("bitbucket issue list"),
        "必须保留命令锚点"
    );

    // 表头应被彻底消除
    assert!(!compacted.contains("ASSIGNEE"), "应消除表头 ASSIGNEE 列");
    assert!(!compacted.contains("PRIORITY"), "应消除表头 PRIORITY 列");

    // 数据行符号化：#<ID> ST:<STATUS> OW:@<assignee> <title> PRI:<priority>
    assert!(
        compacted.contains("#1 ST:OPEN OW:@alice Fix login bug PRI:High"),
        "第一个 issue 应包含 #1 ST:OPEN OW:@alice 和标题及优先级"
    );
    assert!(
        compacted.contains("#2 ST:OPEN OW:@bob Add new feature PRI:Medium"),
        "第二个 issue 应包含正确的状态和优先级"
    );
    assert!(
        compacted.contains("#3 ST:CLOSED OW:@charlie Update documentation PRI:Low"),
        "第三个 issue 应包含 CLOSED 状态"
    );
}

// ============================================================================
// Case 213: pr list(spacing) — 大量空格与长标题仍可稳定压缩
// ============================================================================
#[test]
fn test_pr_list_case_213_spacing() {
    let raw = read_case("case_213_bitbucket_pr_list_spacing");
    let compacted = compact_bitbucket_log_for_ai(&raw);

    assert!(
        compacted.starts_with("bitbucket pr list --state OPEN"),
        "必须保留命令锚点"
    );
    assert!(
        compacted.contains("#221 ST:OPEN OW:@dev_a Fix token cache race condition"),
        "应正确提取第一条 PR 行"
    );
    assert!(
        compacted.contains("#220 ST:DECLINED OW:@dev_b Remove legacy endpoint"),
        "应正确提取第二条 PR 行"
    );
}

// ============================================================================
// Case 214: pr view(multiline desc) — 多行描述应合并为单行 DESC
// ============================================================================
#[test]
fn test_pr_view_case_214_multiline_desc() {
    let raw = read_case("case_214_bitbucket_pr_view_multiline_desc");
    let compacted = compact_bitbucket_log_for_ai(&raw);

    assert!(
        compacted.starts_with("bitbucket pr view 221"),
        "必须保留命令锚点"
    );
    assert!(compacted.contains("ST:OPEN"), "State 应映射为 ST");
    assert!(compacted.contains("OW:@dev_a"), "Author 应映射为 OW");
    assert!(
        compacted.contains("RV:lead1,qa2"),
        "Reviewers 应去掉 approved 后缀并逗号合并"
    );
    assert!(
        compacted.contains("BR:feature/token-cache->main"),
        "Source 应映射为 BR:src->dst"
    );
    assert!(
        compacted.contains("DESC: This change refactors token cache locking. It also improves retry backoff visibility."),
        "多行 Description 应合并到一行"
    );
}

// ============================================================================
// Case 215: issue list(resolved) — RESOLVED 状态应被识别
// ============================================================================
#[test]
fn test_issue_list_case_215_resolved() {
    let raw = read_case("case_215_bitbucket_issue_list_resolved");
    let compacted = compact_bitbucket_log_for_ai(&raw);

    assert!(
        compacted.starts_with("bitbucket issue list --status all"),
        "必须保留命令锚点"
    );
    assert!(
        compacted.contains("#11 ST:RESOLVED OW:@alice Rotate expired API key PRI:High"),
        "RESOLVED issue 应保持状态语义"
    );
    assert!(
        compacted.contains("#12 ST:CLOSED OW:@bob Remove stale webhook PRI:Low"),
        "CLOSED issue 应保持状态语义"
    );
}

// ============================================================================
// Case 216: generic(alert) — 通用路径应保留错误并移除 URL 噪音
// ============================================================================
#[test]
fn test_generic_case_216_alert() {
    let raw = read_case("case_216_bitbucket_generic_alert");
    let compacted = compact_bitbucket_log_for_ai(&raw);

    assert!(
        compacted.starts_with("bitbucket repo sync"),
        "必须保留命令锚点"
    );
    assert!(
        compacted.contains("!CONFLICT: failed to update refs"),
        "冲突/失败应映射为 ! 前缀"
    );
    assert!(
        !compacted.contains("https://bitbucket.org/"),
        "URL 噪音应移除"
    );
}

// ============================================================================
// Case 222: pr list(relative time) — CREATED 非绝对日期也应可解析
// ============================================================================
#[test]
fn test_pr_list_case_222_relative_time() {
    let raw = read_case("case_222_bitbucket_pr_list_relative_time");
    let compacted = compact_bitbucket_log_for_ai(&raw);

    assert!(
        compacted.starts_with("bitbucket pr list --state all"),
        "必须保留命令锚点"
    );
    assert!(
        compacted.contains("#310 ST:OPEN OW:@alice Fix stale cache invalidation"),
        "相对时间场景应仍可提取 PR 行"
    );
    assert!(
        compacted.contains("#309 ST:MERGED OW:@bob Add cloud retry guard"),
        "相对时间场景应仍可提取第二行"
    );
}

// ============================================================================
// 短输入回退
// ============================================================================
#[test]
fn test_short_input_fallback() {
    let raw = "bitbucket help";
    let compacted = compact_bitbucket_log_for_ai(raw);
    assert_eq!(compacted, raw, "过短输入应直接返回原始文本");
}

// ============================================================================
// 噪音检测
// ============================================================================
#[test]
fn test_bb_noise_detection() {
    // 这些是 bitbucket 通用噪音
    assert!(super::methods::is_bb_noise("Description:"));
    assert!(super::methods::is_bb_noise("Changes:"));
    assert!(super::methods::is_bb_noise(
        "Pull request #125: Refactor codebase"
    ));
    assert!(super::methods::is_bb_noise("Created: 2026-04-05"));
    assert!(super::methods::is_bb_noise("Updated: 2026-04-06"));

    // 这些不是噪音
    assert!(!super::methods::is_bb_noise("State: OPEN"));
    assert!(!super::methods::is_bb_noise("Author: alice"));
    assert!(!super::methods::is_bb_noise("Reviewers: bob, charlie"));
}

// ============================================================================
// 异常映射
// ============================================================================
#[test]
fn test_bb_alert_mapping() {
    assert!(super::methods::map_bb_alert("CONFLICT: merge conflict").is_some());
    assert!(super::methods::map_bb_alert("error: something went wrong").is_some());
    assert!(super::methods::map_bb_alert("Push failed").is_some());
    assert!(super::methods::map_bb_alert("Push rejected").is_some());
    assert!(super::methods::map_bb_alert("OPEN 123 Fix auth bug").is_none());
}
