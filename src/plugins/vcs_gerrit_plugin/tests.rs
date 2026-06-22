use super::methods::*;
use std::path::{Path, PathBuf};

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join("vcs_gerrit_plugin")
}

fn read_case(case_name: &str) -> String {
    let file_path = sample_dir().join(format!("{case_name}.log"));
    std::fs::read_to_string(&file_path)
        .unwrap_or_else(|err| panic!("读取样本失败 {}: {err}", file_path.display()))
}

#[test]
fn test_query_case_97() {
    let raw = read_case("case_97_gerrit_query");
    let compacted = compact_gerrit_log_for_ai(&raw);

    assert!(compacted.starts_with("gerrit query"), "必须保留命令锚点");

    let lines: Vec<&str> = compacted.lines().collect();
    assert!(
        lines[1].contains("CHG @Iabc123def456789"),
        "change ID 应带 @ 前缀"
    );
    assert!(
        lines[1].contains("PRJ:platform/frameworks/base"),
        "project 应符号化为 PRJ"
    );
    assert!(lines[1].contains("BR:master"), "branch 应符号化为 BR");
    assert!(lines[1].contains("ST:NEW"), "status 应符号化为 ST");
    assert!(
        lines[1].contains("OW:alice@company.com"),
        "owner 应符号化为 OW"
    );
    assert!(
        lines[1].contains("RV:bob@company.com,carol@company.com"),
        "reviewers 应符号化为 RV"
    );
    assert!(
        lines[2].contains("CHG @Idef789ghi012345"),
        "第二个 change ID"
    );
    assert!(lines[2].contains("ST:MERGED"), "第二个 status");
}

#[test]
fn test_review_case_125() {
    let raw = read_case("case_125_gerrit_review");
    let compacted = compact_gerrit_log_for_ai(&raw);

    assert!(compacted.starts_with("gerrit review"), "必须保留命令锚点");
    assert_eq!(compacted.lines().count(), 1, "标签应合并至命令锚点同一行");
    assert!(
        compacted.contains("CR+2@alice"),
        "Code-Review 应缩写为 CR 并带 @user"
    );
    assert!(
        compacted.contains("V+1@alice"),
        "Verified 应缩写为 V 并带 @user"
    );
    assert!(
        !compacted.contains("Labels are now set"),
        "应消除叙述性噪音"
    );
}

#[test]
fn test_push_case_126() {
    let raw = read_case("case_126_gerrit_push");
    let compacted = compact_gerrit_log_for_ai(&raw);

    assert!(compacted.starts_with("gerrit push"), "必须保留命令锚点");
    assert!(
        !compacted.contains("ssh://review.example.com"),
        "应移除长 URL 噪音"
    );
    assert!(compacted.contains("master"), "应保留 master 引用");
    assert!(compacted.contains("feature-xyz"), "应保留 feature-xyz 引用");
    assert!(compacted.contains("@123/456/1"), "changes 引用应带 @ 前缀");
    assert!(compacted.contains("Pushed 3 refs"), "应保留推送计数");
}

#[test]
fn test_checkout_case_127() {
    let raw = read_case("case_127_gerrit_checkout");
    let compacted = compact_gerrit_log_for_ai(&raw);

    assert!(compacted.starts_with("gerrit checkout"), "必须保留命令锚点");
    assert_eq!(
        compacted.lines().count(),
        1,
        "分支状态应合并至命令锚点同一行"
    );
    assert!(compacted.contains("*feature-xyz"), "分支应带 * 前缀");
    assert!(compacted.contains("(up-to-date)"), "应包含 up-to-date 状态");
}

#[test]
fn test_query_case_217_subject_topic() {
    let raw = read_case("case_217_gerrit_query_subject_topic");
    let compacted = compact_gerrit_log_for_ai(&raw);

    assert!(
        compacted.starts_with("gerrit query --format text"),
        "必须保留命令锚点"
    );
    assert!(
        compacted.contains("CHG @I111aaa222bbb333"),
        "应保留 change id"
    );
    assert!(
        compacted.contains("SJ:Refactor auth session handling"),
        "subject 应映射为 SJ"
    );
    assert!(compacted.contains("TP:auth-refactor"), "topic 应映射为 TP");
}

#[test]
fn test_review_case_218_submit_label() {
    let raw = read_case("case_218_gerrit_review_submit");
    let compacted = compact_gerrit_log_for_ai(&raw);

    assert!(
        compacted.starts_with("gerrit review 12345,2"),
        "必须保留命令锚点"
    );
    assert_eq!(compacted.lines().count(), 1, "标签应并入同一行");
    assert!(compacted.contains("CR+1@alice"), "Code-Review 应缩写为 CR");
    assert!(compacted.contains("V-1@ci-bot"), "Verified 应缩写为 V");
    assert!(
        compacted.contains("Submit+1@release-bot"),
        "未知标签也应保留并追加 @user"
    );
}

#[test]
fn test_push_case_219_changes_refs() {
    let raw = read_case("case_219_gerrit_push_changes_refs");
    let compacted = compact_gerrit_log_for_ai(&raw);

    assert!(
        compacted.starts_with("git push origin HEAD"),
        "必须保留命令锚点"
    );
    assert!(compacted.contains("main"), "heads 同名应折叠为分支名");
    assert!(
        compacted.contains("@34/1234/5"),
        "changes 同名引用应映射为 @路径"
    );
    assert!(compacted.contains("feature-x->main"), "分支流向应保留");
    assert!(compacted.contains("Pushed 3 refs"), "推送计数应保留");
}

#[test]
fn test_generic_case_220_alert_and_url() {
    let raw = read_case("case_220_gerrit_generic_alert_url");
    let compacted = compact_gerrit_log_for_ai(&raw);

    assert!(
        compacted.starts_with("gerrit ls-projects --show-description"),
        "必须保留命令锚点"
    );
    assert!(
        compacted.contains("URL:gr:c/team/repo/+/12345"),
        "Gerrit URL 应缩写为 gr: 前缀"
    );
    assert!(
        compacted.contains("!error: permission denied"),
        "错误应映射为 ! 前缀"
    );
}

#[test]
fn test_generic_case_223_remote_error_ansi() {
    let raw = read_case("case_223_gerrit_remote_error_ansi").replace("\\u001b", "\u{1b}");
    let compacted = compact_gerrit_log_for_ai(&raw);

    assert!(
        compacted.starts_with("gerrit ls-projects --show-description"),
        "必须保留命令锚点"
    );
    assert!(
        compacted.contains("!remote: error: permission denied"),
        "remote 错误不应被噪音规则吞掉"
    );
    assert!(!compacted.contains('\u{1b}'), "输出不应包含 ANSI 转义");
}

#[test]
fn test_short_input_fallback() {
    let raw = "gerrit status";
    let compacted = compact_gerrit_log_for_ai(raw);
    assert_eq!(compacted, raw, "过短输入应直接返回原始文本");
}

#[test]
fn test_alert_prefix_mapping() {
    assert_eq!(
        map_alert_line("CONFLICT (content): Merge conflict in src/main.rs"),
        Some("!CONFLICT (content): Merge conflict in src/main.rs".to_string())
    );
    assert_eq!(
        map_alert_line("error: failed to push some refs"),
        Some("!error: failed to push some refs".to_string())
    );
    assert_eq!(
        map_alert_line("Push rejected"),
        Some("!Push rejected".to_string())
    );
    assert_eq!(
        map_alert_line("!CONFLICT already annotated"),
        Some("!CONFLICT already annotated".to_string())
    );
    assert_eq!(map_alert_line("master -> master"), None);
}

#[test]
fn test_file_status_mapping() {
    assert_eq!(map_file_status("Modified"), "M");
    assert_eq!(map_file_status("modified"), "M");
    assert_eq!(map_file_status("M"), "M");
    assert_eq!(map_file_status("Added"), "A");
    assert_eq!(map_file_status("added"), "A");
    assert_eq!(map_file_status("A"), "A");
    assert_eq!(map_file_status("Deleted"), "D");
    assert_eq!(map_file_status("deleted"), "D");
    assert_eq!(map_file_status("D"), "D");
    assert_eq!(map_file_status("Renamed"), "R");
    assert_eq!(map_file_status("renamed"), "R");
    assert_eq!(map_file_status("R"), "R");
    assert_eq!(map_file_status("Unknown"), "Unknown");
}
