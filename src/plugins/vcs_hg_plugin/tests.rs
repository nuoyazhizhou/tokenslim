#[cfg(test)]
mod tests {
    use super::super::methods::*;
    use super::super::parser::*;
    use std::path::{Path, PathBuf};

    // ============================================================================
    // 统一测试基础设施 — 文件驱动（法则：严禁 Hardcode）
    // ============================================================================
    fn sample_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("vcs_hg_plugin")
    }

    /// 从 samples/vcs_hg_plugin/<name>.log 读取测试用例
    fn read_case(name: &str) -> String {
        let p = sample_dir().join(format!("{}.log", name));
        std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("读取样本失败 {}: {}", p.display(), e))
    }

    // ============================================================================
    // Status 族：hg status — 状态码映射
    // ============================================================================
    #[test]
    fn test_case_08_hg_status() {
        let c = compact_hg_status_for_ai(&read_case("case_08_hg_status"));
        assert!(c.starts_with("hg status"), "必须保留命令锚点");
        assert!(
            c.contains("M src/plugins/vcs_plugin/methods.rs"),
            "M 应保留"
        );
        assert!(c.contains("? config/vcs_plugin.local.json"), "? 应保留");
    }

    #[test]
    fn test_case_232_hg_status_short() {
        let c = compact_hg_status_for_ai(&read_case("case_232_hg_status_S"));
        assert!(c.starts_with("hg status"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_233_hg_status_c() {
        let c = compact_hg_status_for_ai(&read_case("case_233_hg_status_c"));
        assert!(c.starts_with("hg status"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_274_hg_st_short() {
        let c = compact_hg_status_for_ai(&read_case("case_274_hg_st"));
        assert!(c.starts_with("hg st"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_275_hg_status_quiet() {
        let c = compact_hg_status_for_ai(&read_case("case_275_hg_status_quiet"));
        assert!(c.starts_with("hg status"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_276_hg_status_untracked() {
        let c = compact_hg_status_for_ai(&read_case("case_276_hg_status_untracked"));
        assert!(c.contains("?"));
    }

    #[test]
    fn test_case_277_hg_status_path() {
        let c = compact_hg_status_for_ai(&read_case("case_277_hg_status_path"));
        // 带路径参数的 status 不应产生幽灵行
        assert!(!c.lines().any(|l| l.starts_with("h ")), "不应出现幽灵行");
        assert!(c.contains("M"));
    }

    // ============================================================================
    // Diff 族：hg diff
    // ============================================================================
    #[test]
    fn test_case_09_hg_diff() {
        let c = compact_hg_diff_for_ai(&read_case("case_09_hg_diff"));
        assert!(c.starts_with("hg diff"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_301_hg_diff_git() {
        let c = compact_hg_diff_for_ai(&read_case("case_301_hg_diff_git"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_302_hg_diff_stat() {
        let c = compact_hg_diff_for_ai(&read_case("case_302_hg_diff_stat"));
        assert!(!c.is_empty());
    }

    // ============================================================================
    // Log 族：hg log
    // ============================================================================
    #[test]
    fn test_case_10_hg_log() {
        let c = compact_hg_log_for_ai(&read_case("case_10_hg_log"));
        assert!(c.starts_with("hg log"));
        // 法则 D: 输出不为空，包含关键字段
        assert!(!c.is_empty());
        assert!(
            c.contains("18:eeff3344aa55") || c.contains("CH:18:eeff3344aa55"),
            "should contain changeset hash"
        );
    }

    #[test]
    fn test_case_278_hg_log_limit() {
        let c = compact_hg_log_for_ai(&read_case("case_278_hg_log_limit"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_279_hg_log_verbose() {
        let c = compact_hg_log_for_ai(&read_case("case_279_hg_log_verbose"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_280_hg_log_style_compact() {
        let c = compact_hg_log_for_ai(&read_case("case_280_hg_log_style_compact"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_281_hg_log_tip() {
        let c = compact_hg_log_for_ai(&read_case("case_281_hg_log_tip"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_303_hg_log_follow() {
        let c = compact_hg_log_for_ai(&read_case("case_303_hg_log_follow"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_304_hg_log_patch() {
        let c = compact_hg_log_for_ai(&read_case("case_304_hg_log_patch"));
        assert!(!c.is_empty());
    }

    // ============================================================================
    // 远程族：clone / pull / push
    // ============================================================================
    #[test]
    fn test_case_58_hg_clone() {
        let c = process_parser(&HgCloneParser, &read_case("case_58_hg_clone"));
        assert!(c.starts_with("hg clone"));
        assert!(c.contains("dest:"), "应包含 dest: 信息");
        assert!(!c.contains("requesting all changes"), "进度噪音应过滤");
    }

    #[test]
    fn test_case_59_hg_pull() {
        let c = process_parser(&HgPullParser, &read_case("case_59_hg_pull"));
        assert!(c.starts_with("hg pull"));
        assert!(
            c.contains("pull:") || c.contains("no changes"),
            "pull 信息应保留"
        );
    }

    #[test]
    fn test_case_60_hg_push() {
        let c = process_parser(&HgPushParser, &read_case("case_60_hg_push"));
        assert!(c.starts_with("hg push"));
        assert!(
            c.contains("push:") || c.contains("no changes"),
            "push 信息应保留"
        );
    }

    // ============================================================================
    // 提交/更新族
    // ============================================================================
    #[test]
    fn test_case_61_hg_update() {
        let c = process_parser(&HgUpdateParser, &read_case("case_61_hg_update"));
        assert!(c.starts_with("hg update"));
        assert!(
            c.contains("update") || c.contains("wd"),
            "update 信息应保留"
        );
    }

    #[test]
    fn test_case_305_hg_update_c() {
        let c = process_parser(&HgUpdateParser, &read_case("case_305_hg_update_C"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_62_hg_commit() {
        let c = process_parser(&HgCommitParser, &read_case("case_62_hg_commit"));
        assert!(c.starts_with("hg commit"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_306_hg_commit_amend() {
        let c = process_parser(&HgCommitParser, &read_case("case_306_hg_commit_amend"));
        assert!(!c.is_empty());
    }

    // ============================================================================
    // 分支/书签/标签族
    // ============================================================================
    #[test]
    fn test_case_63_hg_branches() {
        let c = process_parser(&HgBranchesParser, &read_case("case_63_hg_branches"));
        assert!(c.starts_with("hg branches"));
        assert!(c.contains("~"), "inactive 应压缩为 ~");
    }

    #[test]
    fn test_case_64_hg_merge() {
        let c = process_parser(&HgMergeParser, &read_case("case_64_hg_merge"));
        assert!(c.starts_with("hg merge"));
        assert!(c.contains("merge"), "merge 信息应保留");
    }

    #[test]
    fn test_case_11_hg_heads() {
        let c = compact_hg_heads_for_ai(&read_case("case_11_hg_heads"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_12_hg_outgoing() {
        let c = compact_hg_outgoing_for_ai(&read_case("case_12_hg_outgoing"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_13_hg_incoming() {
        let c = compact_hg_incoming_for_ai(&read_case("case_13_hg_incoming"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_14_hg_parents() {
        let c = compact_hg_parents_for_ai(&read_case("case_14_hg_parents"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_69_hg_bookmarks() {
        let c = process_parser(&HgBookmarksParser, &read_case("case_69_hg_bookmarks"));
        assert!(c.starts_with("hg bookmarks"));
        assert!(!c.is_empty());
    }

    // ============================================================================
    // 其他操作族
    // ============================================================================
    #[test]
    fn test_case_65_hg_rollback() {
        let c = process_parser(&HgRollbackParser, &read_case("case_65_hg_rollback"));
        assert!(c.starts_with("hg rollback"));
        assert!(c.contains("rollback"), "rollback 信息应保留");
    }

    #[test]
    fn test_case_66_hg_backout() {
        let c = process_parser(&HgBackoutParser, &read_case("case_66_hg_backout"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_67_hg_shelve() {
        let c = process_parser(&HgShelveParser, &read_case("case_67_hg_shelve"));
        assert!(c.starts_with("hg shelve"));
        assert!(c.contains("shelved:"), "shelved 信息应保留");
    }

    #[test]
    fn test_case_68_hg_phase() {
        let c = process_parser(&HgPhaseParser, &read_case("case_68_hg_phase"));
        assert!(c.starts_with("hg phase"));
        assert!(c.contains("draft") || c.contains("public"), "phase 应保留");
    }

    #[test]
    fn test_case_82_hg_tag() {
        let c = process_parser(&HgTagParser, &read_case("case_82_hg_tag"));
        assert!(c.starts_with("hg tag"));
        assert!(c.contains("tag:"), "tag 信息应保留");
    }

    #[test]
    fn test_case_132_hg_copy() {
        let c = process_parser(&HgCopyParser, &read_case("case_132_hg_copy"));
        assert!(c.starts_with("hg copy"));
        assert!(c.contains("copy") && c.contains("->"), "应使用 -> 表示流向");
    }

    #[test]
    fn test_case_133_hg_move() {
        let c = process_parser(&HgMoveParser, &read_case("case_133_hg_move"));
        assert!(c.starts_with("hg move"));
        assert!(c.contains("move") && c.contains("->"), "应使用 -> 表示流向");
    }

    #[test]
    fn test_case_134_hg_purge() {
        let c = process_parser(&HgPurgeParser, &read_case("case_134_hg_purge"));
        assert!(c.starts_with("hg purge"));
        assert!(c.contains("D "), "purge 应映射为 D");
    }

    #[test]
    fn test_case_135_hg_archive() {
        let c = process_parser(&HgArchiveParser, &read_case("case_135_hg_archive"));
        assert!(c.starts_with("hg archive"));
        assert!(c.contains("archive:"), "archive 路径应保留");
    }

    #[test]
    fn test_case_136_hg_verify() {
        let c = process_parser(&HgVerifyParser, &read_case("case_136_hg_verify"));
        assert!(c.starts_with("hg verify"));
        assert!(!c.contains("checking"), "checking 噪音应过滤");
    }

    #[test]
    fn test_case_171_hg_identify() {
        let c = process_parser(&HgIdentifyParser, &read_case("case_171_hg_identify"));
        assert!(!c.is_empty());
    }

    #[test]
    fn test_case_172_hg_paths() {
        let c = process_parser(&HgPathsParser, &read_case("case_172_hg_paths"));
        assert!(c.contains("default="), "paths 应包含 default=");
    }

    #[test]
    fn test_case_173_hg_config() {
        let c = process_parser(&HgConfigParser, &read_case("case_173_hg_config"));
        assert!(c.contains("[ui]"), "config 段落应保留");
    }

    #[test]
    fn test_case_174_hg_summarize() {
        let c = process_parser(&HgSummarizeParser, &read_case("case_174_hg_summarize"));
        assert!(c.contains("BR:main"), "分支应映射为 BR:");
    }

    #[test]
    fn test_case_175_hg_transplant() {
        let c = process_parser(&HgTransplantParser, &read_case("case_175_hg_transplant"));
        assert!(c.contains("transplant"), "transplant 信息应保留");
    }

    // ============================================================================
    // Phase 2 P2 增强功能测试 — 新增 3 个功能
    // ============================================================================

    /// 测试 hg shelve --list 压缩：折叠冗长的 shelve 列表
    #[test]
    fn test_case_085_hg_shelve_list_compresses() {
        let raw = read_case("case_085_shelve_list");
        let c = compact_hg_shelve_enhanced(&raw);

        // 验证命令锚点保留
        assert!(c.starts_with("hg shelve --list"), "必须保留命令锚点");

        // 验证摘要行存在
        assert!(c.contains("[SHELVE]"), "应包含 [SHELVE] 摘要");
        assert!(c.contains("10 shelves"), "应显示总数");
        assert!(c.contains("first 5 shown"), "应说明显示前 5 个");
        assert!(c.contains("5 omitted"), "应说明省略 5 个");

        // 验证前 5 个 shelve 保留
        assert!(c.contains("default"), "应保留第 1 个 shelve");
        assert!(c.contains("feature-auth"), "应保留第 2 个 shelve");

        // 验证后 5 个 shelve 被省略
        assert!(!c.contains("optimize-perf"), "不应包含最后一个 shelve");
    }

    /// 测试 hg graft 输出压缩：折叠详细的 graft 信息
    #[test]
    fn test_case_086_hg_graft_compresses() {
        let raw = read_case("case_086_graft");
        let c = compact_hg_graft_enhanced(&raw);

        // 验证命令锚点保留
        assert!(c.starts_with("hg graft"), "必须保留命令锚点");

        // 验证摘要行存在
        assert!(c.contains("[GRAFT]"), "应包含 [GRAFT] 摘要");
        assert!(c.contains("5 changesets grafted"), "应显示 graft 数量");

        // 验证 changeset 信息保留
        assert!(c.contains("123:abc1234"), "应保留 changeset 标识");
        assert!(
            c.contains("feat: Add login feature"),
            "应保留 commit message"
        );

        // 验证 merging 行被过滤
        assert!(
            !c.contains("merging src/auth/login.py"),
            "不应包含 merging 详细信息"
        );
    }

    /// 测试 hg histedit 输出压缩：折叠注释行，只保留命令
    #[test]
    fn test_case_087_hg_histedit_compresses() {
        let raw = read_case("case_087_histedit");
        let c = compact_hg_histedit_enhanced(&raw);

        // 验证命令锚点保留
        assert!(c.starts_with("hg histedit"), "必须保留命令锚点");

        // 验证摘要行存在
        assert!(c.contains("[HISTEDIT]"), "应包含 [HISTEDIT] 摘要");
        assert!(c.contains("20 changesets"), "应显示 changeset 数量");
        assert!(c.contains("interactive mode"), "应说明交互模式");

        // 验证命令行保留
        assert!(c.contains("pick abc1234"), "应保留 pick 命令");
        assert!(
            c.contains("feat: Add login feature"),
            "应保留 commit message"
        );

        // 验证注释行被过滤
        assert!(!c.contains("# Edit history between"), "不应包含注释行");
        assert!(!c.contains("# Commands:"), "不应包含注释行");
        assert!(!c.contains("# p, pick = use commit"), "不应包含注释行");
    }

    /// ROI 测试：shelve list 压缩不应导致扩展
    #[test]
    fn test_case_085_shelve_list_compression_does_not_expand() {
        let raw = read_case("case_085_shelve_list");
        let c = compact_hg_shelve_enhanced(&raw);

        // ROI 门控：压缩后不应比原始输入更长
        assert!(
            c.len() <= raw.len(),
            "压缩后长度 {} 不应超过原始长度 {}",
            c.len(),
            raw.len()
        );
    }

    /// ROI 测试：graft 压缩不应导致扩展
    #[test]
    fn test_case_086_graft_compression_does_not_expand() {
        let raw = read_case("case_086_graft");
        let c = compact_hg_graft_enhanced(&raw);

        // ROI 门控：压缩后不应比原始输入更长
        assert!(
            c.len() <= raw.len(),
            "压缩后长度 {} 不应超过原始长度 {}",
            c.len(),
            raw.len()
        );
    }

    /// ROI 测试：histedit 压缩不应导致扩展
    #[test]
    fn test_case_087_histedit_compression_does_not_expand() {
        let raw = read_case("case_087_histedit");
        let c = compact_hg_histedit_enhanced(&raw);

        // ROI 门控：压缩后不应比原始输入更长
        assert!(
            c.len() <= raw.len(),
            "压缩后长度 {} 不应超过原始长度 {}",
            c.len(),
            raw.len()
        );
    }

    /// 边界测试：shelve list 少于阈值时不应折叠
    #[test]
    fn test_shelve_list_below_threshold_not_folded() {
        let raw =
            "hg shelve --list\ndefault (10 files, 2 days ago)\nfeature (5 files, 1 week ago)\n";
        let c = compact_hg_shelve_enhanced(raw);

        // 少于 5 个 shelve，不应折叠
        assert!(!c.contains("[SHELVE]"), "少于阈值时不应添加摘要");
        assert!(c.contains("default"), "应保留所有 shelve");
        assert!(c.contains("feature"), "应保留所有 shelve");
    }

    /// 边界测试：空 graft 输出应保持原样
    #[test]
    fn test_empty_graft_output_unchanged() {
        let raw = "hg graft 123\n";
        let c = compact_hg_graft_enhanced(raw);

        // 空输出应保持原样
        assert_eq!(c, raw, "空输出应保持不变");
    }

    // ============================================================================
    // 单元测试 — 邮箱提取 & 短输入回退（这些是函数级别测试，不需 sample 文件）
    // ============================================================================
    #[test]
    fn extract_hg_author_with_email() {
        assert_eq!(extract_hg_author("developer <dev@example.com>"), "@dev");
    }

    #[test]
    fn extract_hg_author_without_email() {
        assert_eq!(extract_hg_author("alice.chen"), "alice.chen");
    }

    #[test]
    fn extract_hg_author_direct_email() {
        assert_eq!(extract_hg_author("user.name@domain.com"), "@user.name");
    }

    #[test]
    fn test_short_input_no_crash() {
        let output = compact_hg_status_for_ai("hg help");
        assert_eq!(output, "hg help");
    }
}
