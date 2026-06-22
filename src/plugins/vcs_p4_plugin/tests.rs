use super::methods::*;
use std::path::{Path, PathBuf};

// ============================================================================
// 测试基础设施 — 文件驱动
// ============================================================================

fn sample_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join("vcs_p4_plugin")
}

/// 从 samples/vcs_p4_plugin/<name>.log 读取测试用例
fn read_case(name: &str) -> String {
    let p = sample_dir().join(format!("{}.log", name));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取样本失败 {}: {}", p.display(), e))
}

/// 【红线断言 1】输出的第一行必须等于输入的第一行（命令锚点保护）
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

/// 从压缩输出行中提取指定字段的值，如从 "CH:12345 CR:2026/04/03" 中提取 "CR:2026/04/03"
fn extract_field(line: &str, prefix: &str) -> String {
    line.split_whitespace()
        .find(|token| token.starts_with(prefix))
        .map(|t| t[prefix.len()..].to_string())
        .unwrap_or_default()
}

// ============================================================================
// Case 15: opened — edit/add 状态映射正确性 + 版本号剔除
// ============================================================================
#[test]
fn test_case_15_p4_opened() {
    let output = compact_p4_opened_for_ai(&read_case("case_15_p4_opened"));
    assert_anchor_preserved("case_15_p4_opened", &output);
    // edit → M:
    assert!(
        output.contains("M://depot/main/src/core/doctor_workspace/methods.rs"),
        "edit 应映射为 M:"
    );
    // add → A:
    assert!(
        output.contains("A://depot/main/src/plugins/vcs_plugin/parser.rs"),
        "add 应映射为 A:"
    );
    // 版本号 #N 已剔除
    assert!(!output.contains("#8"), "版本号 #N 应被剔除");
    assert!(!output.contains("#3"), "版本号 #N 应被剔除");
    // 废话已清除
    assert!(
        !output.contains("default change"),
        "default change 废话应被清除"
    );
    assert!(!output.contains("(text)"), "文件类型注释应被清除");
}

// ============================================================================
// Case 90: p4 add — 【红线断言】输出包含 A: 而非 M:
// ============================================================================
#[test]
fn test_case_90_p4_add_maps_to_a_not_m() {
    let output = compact_p4_add_for_ai(&read_case("case_90_p4_add"));
    assert_anchor_preserved("case_90_p4_add", &output);
    assert!(
        output.contains("A://depot/main/src/new_file.rs"),
        "p4 add 应映射为 A: 而非 M:, 实际输出: {}",
        output
    );
    assert!(
        output.contains("A://depot/main/config/new_config.json"),
        "第二个文件也应映射为 A:"
    );
    // 确认不存在 M: 误标
    let m_count = output.matches("M:").count();
    assert_eq!(m_count, 0, "p4 add 输出不应出现任何 M: 标记");
}

// ============================================================================
// Case 91: p4 delete — 【红线断言】输出包含 D: 而非 M:
// ============================================================================
#[test]
fn test_case_91_p4_delete_maps_to_d_not_m() {
    let output = compact_p4_delete_for_ai(&read_case("case_91_p4_delete"));
    assert_anchor_preserved("case_91_p4_delete", &output);
    assert!(
        output.contains("D://depot/main/src/old_file.rs"),
        "p4 delete 应映射为 D: 而非 M:, 实际输出: {}",
        output
    );
    assert!(
        output.contains("D://depot/main/src/deprecated/module.rs"),
        "第二个文件也应映射为 D:"
    );
    let m_count = output.matches("M:").count();
    assert_eq!(m_count, 0, "p4 delete 输出不应出现任何 M: 标记");
}

// ============================================================================
// Case 89: p4 edit — edit → M:
// ============================================================================
#[test]
fn test_case_89_p4_edit() {
    let output = compact_p4_edit_for_ai(&read_case("case_89_p4_edit"));
    assert_anchor_preserved("case_89_p4_edit", &output);
    assert!(!output.contains("opened for edit"), "叙述废话应被清除");
    assert!(
        output.contains("M://depot/main/src/main.rs"),
        "edit 应映射为 M:"
    );
    assert!(output.contains("M://depot/main/src/utils/helper.rs"));
    assert!(output.contains("M://depot/main/config/app.json"));
}

// ============================================================================
// Case 17: changes — 标准符号化 CH: CR: OW: ST: CM:
// ============================================================================
#[test]
fn test_case_17_p4_changes() {
    let output = compact_p4_changes_for_ai(&read_case("case_17_p4_changes"));
    assert_anchor_preserved("case_17_p4_changes", &output);
    assert!(
        output.contains("CH:12345 CR:20260403 OW:@alice.chen ST:pending CM:feat(vcs): Add SVN/HG/P4 parser support"),
        "第一行应完整符号化: {}",
        output
    );
    assert!(
        output.contains("CH:12344 CR:20260402 OW:@bob.wang ST:pending CM:fix: Resolve path dictionary collision with email addresses"),
        "第二行应完整符号化"
    );
    assert!(
        output.contains("CH:12338 CR:20260327 OW:@bob.wang ST:pending CM:fix: Handle Windows path separators in path compressor"),
        "最后一行应完整符号化"
    );
}

// ============================================================================
// Case 236: changes -m — 【红线断言】CR: 字段不为空
// ============================================================================
#[test]
fn test_case_236_changes_max_cr_not_empty() {
    let output = compact_p4_changes_for_ai(&read_case("case_236_p4_changes_max"));
    assert_anchor_preserved("case_236_p4_changes_max", &output);
    // 逐行检查 CR: 字段非空
    for (i, line) in output.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue; // 跳过锚点行和空行
        }
        let cr_val = extract_field(line, "CR:");
        assert!(
            !cr_val.is_empty(),
            "Case 236: CR: 字段不得为空, line {i}: {line}"
        );
    }
    // 验证具体行内容
    assert!(
        output.contains("CH:12349 CR:20260403 OW:@alice.chen ST:"),
        "应正确解析 'by X on DATE' 格式: {}",
        output
    );
    assert!(
        output.contains("CH:12345 CR:20260403 OW:@alice.chen ST:"),
        "最后一行也应正确解析"
    );
}

// ============================================================================
// Case 308: changes -l — 【红线断言】描述信息中不含换行符
// ============================================================================
#[test]
fn test_case_308_changes_l_no_newlines_in_cm() {
    let output = compact_p4_changes_for_ai(&read_case("case_308_p4_changes_l"));
    assert_anchor_preserved("case_308_p4_changes_l", &output);
    // 每条 Change 记录独占一行，CM: 内容内不得含换行
    for (i, line) in output.lines().enumerate() {
        if i == 0 {
            continue;
        }
        assert!(
            !line.contains('\n'),
            "Case 308: 每行 Change 记录不得含换行, line {i}: {line}"
        );
    }
    // 多行续接已拼入同一 CM:
    let line1 = output.lines().nth(1).unwrap();
    assert!(
        line1.contains("when connecting to the LDAP server"),
        "续行 'when connecting to...' 应拼入 CM:, 实际: {}",
        line1
    );
    assert!(
        line1.contains("adds retry logic"),
        "续行 'adds retry logic' 应拼入 CM:（不换行）"
    );
    // 验证第二条记录的续行也已拼入
    assert!(
        output.contains(
            "Validates email format, password strength, and username uniqueness before submission"
        ),
        "第二条续行应拼入 CM:"
    );
}

// ============================================================================
// Case 234: opened long — 锚点保护 + 长格式处理
// ============================================================================
#[test]
fn test_case_234_p4_opened_long_anchor_preserved() {
    let output = compact_p4_opened_for_ai(&read_case("case_234_p4_opened_long"));
    assert_anchor_preserved("case_234_p4_opened_long", &output);
    // 长格式路径也压缩为 M: + 路径
    assert!(
        output.contains("M://depot/main/src/core/doctor_workspace/methods.rs"),
        "长格式 opened 应正确压缩: {}",
        output
    );
    assert!(
        output.contains("M://depot/main/src/plugins/vcs_plugin/parser.rs"),
        "长格式 opened 第二条也应正确压缩"
    );
    // 版本号已剔除
    assert!(!output.contains("#2"), "版本号应被剔除");
    assert!(!output.contains("#3"), "版本号应被剔除");
    // 噪音已清除
    assert!(!output.contains("on chrome"), "changlist 名称应被清除");
    assert!(!output.contains("alice.chen@client"), "用户信息应被清除");
}

// ============================================================================
// Case 83: sync — 命令锚点 + 状态映射 + 噪音消除
// ============================================================================
#[test]
fn test_case_83_p4_sync() {
    let output = compact_p4_sync_for_ai(&read_case("case_83_p4_sync"));
    assert_anchor_preserved("case_83_p4_sync", &output);
    assert!(
        !output.contains("Sync completed"),
        "Sync completed 废话应被消除"
    );
    assert!(output.contains("M:src/main.rs"), "updated 应映射为 M:");
    assert!(output.contains("A:config/app.json"), "added 应映射为 A:");
}

// ============================================================================
// Case 84: submit — 命令锚点 + Change X created/submitted 废话消除
// ============================================================================
#[test]
fn test_case_84_p4_submit() {
    let output = compact_p4_submit_for_ai(&read_case("case_84_p4_submit"));
    assert_anchor_preserved("case_84_p4_submit", &output);
    assert!(!output.contains("created."), "Change created 废话应被消除");
    assert!(
        !output.contains("submitted."),
        "Change submitted 废话应被消除"
    );
    assert!(
        output.contains("//depot/main/src/main.rs"),
        "应保留文件路径"
    );
    assert!(
        output.contains("//depot/main/src/utils/helper.rs"),
        "应保留文件路径"
    );
}

// ============================================================================
// Case 85: shelve — 命令锚点 + Shelve change 废话消除
// ============================================================================
#[test]
fn test_case_85_p4_shelve() {
    let output = compact_p4_shelve_for_ai(&read_case("case_85_p4_shelve"));
    assert_anchor_preserved("case_85_p4_shelve", &output);
    assert!(
        !output.contains("Shelve change"),
        "Shelve change 废话应被消除"
    );
    assert!(
        output.contains("//depot/main/src/main.rs"),
        "应保留文件路径"
    );
}

// ============================================================================
// Case 86: unshelve — 命令锚点
// ============================================================================
#[test]
fn test_case_86_p4_unshelve() {
    let output = compact_p4_unshelve_for_ai(&read_case("case_86_p4_unshelve"));
    assert_anchor_preserved("case_86_p4_unshelve", &output);
    assert!(
        !output.contains("Unshelved change"),
        "Unshelved change 废话应被消除"
    );
    assert!(
        !output.contains("files restored"),
        "files restored 废话应被消除"
    );
    assert!(
        output.contains("//depot/main/src/main.rs"),
        "应保留文件路径"
    );
}

// ============================================================================
// Case 87: resolve — 命令锚点
// ============================================================================
#[test]
fn test_case_87_p4_resolve() {
    let output = compact_p4_resolve_for_ai(&read_case("case_87_p4_resolve"));
    assert_anchor_preserved("case_87_p4_resolve", &output);
    assert!(!output.contains("files resolved"), "冲突摘要应被清除");
    assert!(
        output.contains("//depot/main/src/main.rs"),
        "应保留文件路径"
    );
}

// ============================================================================
// Case 88: revert — 命令锚点
// ============================================================================
#[test]
fn test_case_88_p4_revert() {
    let output = compact_p4_revert_for_ai(&read_case("case_88_p4_revert"));
    assert_anchor_preserved("case_88_p4_revert", &output);
    assert!(!output.contains("files reverted."), "摘要应被清除");
    assert!(
        output.contains("//depot/main/src/main.rs"),
        "应保留文件路径"
    );
}

// ============================================================================
// Case 307: diff — DIFF: 头部压缩
// ============================================================================
#[test]
fn test_case_307_p4_diff() {
    let output = compact_p4_other_for_ai(&read_case("case_307_p4_diff"));
    assert_anchor_preserved("case_307_p4_diff", &output);
    assert!(
        output.contains("DIFF://depot/main/src/main.rs"),
        "==== 头部应压缩为 DIFF:"
    );
    assert!(
        output.contains("DIFF://depot/main/src/lib/utils.rs"),
        "第二个 ==== 头部应压缩"
    );
}

// ============================================================================
// 短输入回退测试
// ============================================================================
#[test]
fn test_short_input_no_crash() {
    // 极短输入 → 回退到原文
    let output = compact_p4_opened_for_ai("p4 help");
    assert!(output.starts_with("p4 help"), "短输入应原样返回");
}

#[test]
fn test_p4_command_line_detection() {
    // 验证 p4 命令行识别
    assert!(is_p4_command_line("p4 opened"));
    assert!(is_p4_command_line("p4 changes -m 5 //depot/..."));
    assert!(is_p4_command_line("p4 submit"));
    assert!(!is_p4_command_line("//depot/main/src/main.rs"));
    assert!(!is_p4_command_line("Change 12345 on 2026/04/03 by alice"));
}

// ============================================================================
// Case 235: describe -s — 锚点 + author 保留
// ============================================================================
#[test]
fn test_case_235_p4_describe_short() {
    let output = compact_p4_describe_for_ai(&read_case("case_235_p4_describe_short"));
    assert_anchor_preserved("case_235_p4_describe_short", &output);
    assert!(output.contains("alice.chen"), "author 应保留");
    assert!(output.contains("VCS parsers"), "描述信息应保留");
    assert!(
        !output.contains("Updated files:"),
        "Updated files 标题应被清除"
    );
}

// ============================================================================
// Case 20: info — 锚点 + 键值压缩
// ============================================================================
#[test]
fn test_case_20_p4_info() {
    let output = compact_p4_info_for_ai(&read_case("case_20_p4_info"));
    assert_anchor_preserved("case_20_p4_info", &output);
    assert!(
        output.contains("User: alice.chen"),
        "User name 应压缩为 User:"
    );
    assert!(
        output.contains("Client: alice-workstation"),
        "Client name 应压缩为 Client:"
    );
    assert!(
        output.contains("Server: perforce.example.com:1666"),
        "Server address 应压缩为 Server:"
    );
}

// ============================================================================
// Case 22: dirs — 前缀提取
// ============================================================================
#[test]
fn test_case_22_p4_dirs() {
    let output = compact_p4_dirs_for_ai(&read_case("case_22_p4_dirs"));
    assert_anchor_preserved("case_22_p4_dirs", &output);
    assert!(output.contains("root: //depot/main/"), "应提取公共根路径");
    assert!(output.contains("src/plugins/vcs_plugin/"), "子路径应保留");
}

// ============================================================================
// Case 310: sync -n — 预览模式
// ============================================================================
#[test]
fn test_case_310_p4_sync_preview() {
    let output = compact_p4_sync_for_ai(&read_case("case_310_p4_sync_n"));
    assert_anchor_preserved("case_310_p4_sync_n", &output);
    assert!(!output.contains("files would be updated"), "摘要应被清除");
    assert!(
        output.contains("//depot/main/src/main.rs"),
        "应保留文件路径"
    );
}

// ============================================================================
// Case 16: describe — 单行化 + 无 commit 噪音 + 状态前缀
// ============================================================================
#[test]
fn test_case_16_p4_describe_flattened() {
    let output = compact_p4_describe_for_ai(&read_case("case_16_p4_describe"));
    assert_anchor_preserved("case_16_p4_describe", &output);
    // 不应出现 "commit 12345" 噪音行
    assert!(
        !output.contains("commit "),
        "describe 输出不应含 commit 噪音"
    );
    // 多行描述应拍扁为单行（CM: 内容不含换行）
    for line in output.lines().skip(1) {
        assert!(!line.contains('\n'), "CM: 描述不得换行: {}", line);
    }
    // 文件列表应有状态前缀: A://path, M://path
    assert!(
        output.contains("A://depot/main/src/plugins/vcs_plugin/parser.rs"),
        "add 应映射为 A:"
    );
    assert!(
        output.contains("M://depot/main/src/plugins/vcs_plugin/methods.rs"),
        "edit 应映射为 M:"
    );
    // DIFF 头部符号化
    // DIFF 头部符号化
    assert!(
        output.contains("DIFF://depot/main/src/plugins/vcs_plugin/parser.rs"),
        "==== 应转为 DIFF:"
    );
}

// ============================================================================
// Case 18: fstat — 键名缩写 + 单行拍扁（法则 P4-2）
// ============================================================================
#[test]
fn test_case_18_p4_fstat_abbreviated() {
    let output = compact_p4_fstat_for_ai(&read_case("case_18_p4_fstat"));
    assert_anchor_preserved("case_18_p4_fstat", &output);
    // 冗长键名已缩写且拍扁到单行
    assert!(!output.contains("depotFile:"), "depotFile 应缩写为 DF");
    assert!(!output.contains("headAction:"), "headAction 应缩写为 ACT");
    assert!(!output.contains("headRev:"), "headRev 应缩写为 REV");
    // action: <val> 行必须被删除
    assert!(!output.contains("action:"), "块末尾 action: 行应被删除");
    // clientFile 行必须被删除
    assert!(!output.contains("clientFile"), "clientFile 行应被删除");
    // 第一行文件拍扁: DF://depot/... REV:8 ACT:edit TYPE:text CHANGE:12340 HAVE:7
    let first_line = output.lines().nth(1).unwrap();
    assert!(
        first_line.starts_with("DF://depot/main/src/core/doctor_workspace/methods.rs "),
        "第一行应以 DF: 开头并包含路径, 实际: {}",
        first_line
    );
    assert!(
        first_line.contains("REV:8")
            && first_line.contains("ACT:edit")
            && first_line.contains("TYPE:text"),
        "第一行应包含 REV:8 ACT:edit TYPE:text, 实际: {}",
        first_line
    );
    // 多个文件各占一行
    assert!(
        output.lines().count() >= 6,
        "至少应有 6 行（1 锚点 + 5 文件）"
    );
    // 不含冗长键名
    assert!(!output.contains("depotFile"), "正文不应出现冗长键名");
    assert!(!output.contains("headType"), "正文不应出现冗长键名");
}

// ============================================================================
// Case 312: fstat -T — 单行拍扁（法则 P4-2）
// ============================================================================
#[test]
fn test_case_312_p4_fstat_t() {
    let output = compact_p4_fstat_for_ai(&read_case("case_312_p4_fstat_T"));
    assert_anchor_preserved("case_312_p4_fstat_T", &output);
    // -T 格式拍扁为单行: DF://depot/main/src/main.rs TYPE:text REV:5
    let body: Vec<&str> = output.lines().skip(1).collect();
    let body_str = body.join("\n");
    assert!(
        body_str.contains("DF://depot/main/src/main.rs"),
        "-T 格式 DF: 应缩写: {}",
        body_str
    );
    assert!(body_str.contains("TYPE:text"), "-T 格式 TYPE: 应缩写");
    assert!(body_str.contains("REV:5"), "-T 格式 REV: 应缩写");
    // 正文部分不应出现冗长键名
    assert!(
        !body_str.contains("depotFile"),
        "正文部分冗长键名应被消除: {}",
        body_str
    );
    assert!(!body_str.contains("headType"), "正文部分冗长键名应被消除");
    // 所有属性应在同一行
    assert_eq!(body.len(), 1, "-T 格式应输出单行, 实际: {}", body_str);
}

// ============================================================================
// Case 184: p4 files — #N - action → A:/M:/D: + 路径 符号化（法则 P4-3）
// ============================================================================
#[test]
fn test_case_184_p4_files_symbolized() {
    let output = compact_p4_files_for_ai(&read_case("case_184_p4_files"));
    assert_anchor_preserved("case_184_p4_files", &output);
    assert!(
        output.contains("A://depot/main/src/main.rs"),
        "add 应映射为 A:, 实际: {}",
        output
    );
    assert!(
        output.contains("M://depot/main/src/main.rs"),
        "edit 应映射为 M:"
    );
    assert!(!output.contains("#1"), "版本号应被剔除");
    assert!(!output.contains("#2"), "版本号应被剔除");
    assert!(!output.contains(" - add"), "add 噪音应被消除");
    assert!(!output.contains(" - edit"), "edit 噪音应被消除");
}

// ============================================================================
// Case 142: p4 move — moved from → R:dest <- source 符号化（法则 P4-3）
// ============================================================================
#[test]
fn test_case_142_p4_move_symbolized() {
    let output = compact_p4_move_for_ai(&read_case("case_142_p4_move"));
    assert_anchor_preserved("case_142_p4_move", &output);
    assert!(
        output.contains("R://depot/main/dest.txt <- //depot/main/source.txt"),
        "move 应映射为 R:dest <- source, 实际: {}",
        output
    );
    assert!(!output.contains("#1"), "版本号应被剔除");
    assert!(!output.contains("moved from"), "moved from 噪音应被消除");
}

// ============================================================================
// Case 143: p4 copy — -> → C:src -> dest 符号化（法则 P4-3）
// ============================================================================
#[test]
fn test_case_143_p4_copy_symbolized() {
    let output = compact_p4_copy_for_ai(&read_case("case_143_p4_copy"));
    assert_anchor_preserved("case_143_p4_copy", &output);
    assert!(
        output.contains("C://depot/main/file1.txt -> //depot/backup/file1.txt"),
        "copy 应映射为 C:src -> dest, 实际: {}",
        output
    );
    assert!(
        output.contains("C://depot/main/file2.txt -> //depot/backup/file2.txt"),
        "第二个文件也应压缩"
    );
}

// ============================================================================
// Case 144: p4 integrate — -> → C:src -> dest 符号化（法则 P4-3）
// ============================================================================
#[test]
fn test_case_144_p4_integrate_symbolized() {
    let output = compact_p4_integrate_for_ai(&read_case("case_144_p4_integrate"));
    assert_anchor_preserved("case_144_p4_integrate", &output);
    assert!(
        output.contains("C://depot/main/src/main.rs -> //depot/feature/src/main.rs"),
        "integrate 应映射为 C:src -> dest, 实际: {}",
        output
    );
    assert!(
        output.contains("C://depot/main/src/lib.rs -> //depot/feature/src/lib.rs"),
        "第二个文件也应压缩"
    );
}

// ============================================================================
// Case 185: filelog — 深度粉碎 #N|CH:X|M|date|@user
// ============================================================================
#[test]
fn test_case_185_p4_filelog_symbold() {
    let output = compact_p4_filelog_for_ai(&read_case("case_185_p4_filelog"));
    assert_anchor_preserved("case_185_p4_filelog", &output);
    // 深度粉碎格式: #2|CH:45678|M|2026-04-08|@alice
    assert!(
        output.contains("#2|CH:45678|M|2026-04-08|@alice"),
        "filelog 应深度粉碎: {}",
        output
    );
    assert!(
        output.contains("#1|CH:12345|A|2026-01-01|@bob"),
        "第二条也应深度粉碎"
    );
    // 日期中 / 已替换为 -
    assert!(!output.contains("2026/04"), "日期 / 已替换为 -");
    // 不应保留 "change " 前缀
    assert!(!output.contains(" change "), "filelog 不应保留 change 前缀");
}

// ============================================================================
// Case 236: changes -m — ST: 默认 submitted
// ============================================================================
#[test]
fn test_case_236_st_default_submitted() {
    let output = compact_p4_changes_for_ai(&read_case("case_236_p4_changes_max"));
    // 每行 ST: 字段必须为 submitted
    for (i, line) in output.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue;
        }
        assert!(
            line.contains("ST:submitted"),
            "ST: 应默认 submitted, line {i}: {}",
            line
        );
    }
}
