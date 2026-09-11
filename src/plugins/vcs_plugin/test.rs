//! vcs_plugin test module — 中央调度器核心测试
//!
//! 各工具的具体测试已迁移到独立微插件，此处仅保留调度器基础设施测试。

#[cfg(test)]
mod tests {
    use crate::core::compression::Token;
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::{Slice, SliceType};
    use crate::plugins::vcs_plugin::methods::{run_with_vcs_ai_context, VcsAiProfile};
    use crate::plugins::vcs_plugin::types::VcsPlugin;
    use once_cell::sync::Lazy;
    use std::borrow::Cow;
    use std::collections::HashMap;
    use std::sync::Mutex;

    static VCS_MODE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    /// 测试辅助：以干净的 VCS 模式运行闭包。
    fn with_clean_vcs_mode<T>(f: impl FnOnce() -> T) -> T {
        let _guard = VCS_MODE_LOCK.lock().expect("vcs mode lock poisoned");
        run_with_vcs_ai_context(false, VcsAiProfile::None, f)
    }

    /// 测试辅助：从文本构造 Slice。
    fn slice_from_text<'a>(text: &'a str) -> Slice<'a> {
        Slice {
            id: 1,
            text: Cow::Borrowed(text),
            slice_type: SliceType::LogBlock,
            offset: 0,
            line_start: 1,
            line_end: text.lines().count().max(1),
            file_metadata: None,
            flags: Default::default(),
        }
    }

    /// 测试：npm help 不被误分类为 VCS。
    #[test]
    fn detect_should_not_misclassify_npm_help_as_vcs() {
        with_clean_vcs_mode(|| {
            let plugin = VcsPlugin::new();
            let text = r#"npm <command>

Usage:

npm install        install all the dependencies in your project
npm run <foo>      run the script named <foo>
All commands:
    config, dedupe, deprecate, diff, docs, doctor,
    update, version, view
"#;
            let slice = slice_from_text(text);
            assert_eq!(plugin.detect(&slice), None);
        });
    }

    /// 测试：svn status 信号被保留。
    #[test]
    fn detect_should_keep_svn_status_signal() {
        with_clean_vcs_mode(|| {
            let plugin = VcsPlugin::new();
            let text = r#"svn status
M       src/main.rs
?       tmp/cache.txt
"#;
            let slice = slice_from_text(text);
            assert!(plugin.detect(&slice).is_some());
        });
    }

    /// 测试：非 VCS 命令头被短路。
    #[test]
    fn detect_should_short_circuit_non_vcs_command_head() {
        with_clean_vcs_mode(|| {
            let plugin = VcsPlugin::new();
            let text = r#"npm install
git log
svn status
"#;
            let slice = slice_from_text(text);
            assert_eq!(plugin.detect(&slice), None);
        });
    }

    /// 测试：显式云 VCS 命令头优先。
    #[test]
    fn detect_should_prefer_explicit_cloud_vcs_command_head() {
        with_clean_vcs_mode(|| {
            let plugin = VcsPlugin::new();
            let text = r#"gh pr list
No pull requests found for this repository.
"#;
            let slice = slice_from_text(text);
            assert!(plugin.detect(&slice).is_some());
        });
    }

    /// 测试：含 updated 词的 git log 路由到 log 模式。
    #[test]
    fn git_log_with_updated_word_should_route_to_log_mode() {
        let plugin = VcsPlugin::new();
        let text = r#"git log -n 90
commit c6c55247ba1dc1e76d9462895511a2decf46a532
Author: nuoyazhizhou <nuoyazhizhou@example.com>
Date:   Wed May 6 22:53:54 2026 +0800

    optimize parser

commit 56ad0f4c4a4a95ea566b5632f6e89fcec9a853bc
Author: nuoyazhizhou <nuoyazhizhou@example.com>
Date:   Wed May 6 22:53:28 2026 +0800

    Updated route config for vcs; svn log fixed
"#;
        let slice = slice_from_text(text);
        let mut dict = DictionaryEngine::new();
        let mut dedup = DedupEngine::new(DedupConfig::default());
        let arena = bumpalo::Bump::new();
        let result = run_with_vcs_ai_context(true, VcsAiProfile::Log, || {
            plugin.compress(&slice, &mut dict, &mut dedup, &arena)
        });
        let mode = result
            .metadata
            .as_ref()
            .and_then(|m| m.get("mode"))
            .cloned()
            .unwrap_or_default();
        assert!(
            mode.contains("log"),
            "git log should use log mode, got mode={mode}"
        );
    }

    /// 测试：git log 长短 flag 路由到相同 log 模式。
    #[test]
    fn git_log_short_and_long_flags_route_to_same_log_mode() {
        let plugin = VcsPlugin::new();
        let long_text = r#"git log -n 90
commit c6c55247ba1dc1e76d9462895511a2decf46a532
Author: nuoyazhizhou <nuoyazhizhou@example.com>
Date:   Wed May 6 22:53:54 2026 +0800

    Updated route config with svn log mention
"#;
        let short_text = r#"git log -n 2
commit c6c55247ba1dc1e76d9462895511a2decf46a532
Author: nuoyazhizhou <nuoyazhizhou@example.com>
Date:   Wed May 6 22:53:54 2026 +0800

    Updated route config with svn log mention
"#;

        let run_one = |text: &str| {
            let slice = slice_from_text(text);
            let mut dict = DictionaryEngine::new();
            let mut dedup = DedupEngine::new(DedupConfig::default());
            let arena = bumpalo::Bump::new();
            let result = run_with_vcs_ai_context(true, VcsAiProfile::Log, || {
                plugin.compress(&slice, &mut dict, &mut dedup, &arena)
            });
            result
                .metadata
                .as_ref()
                .and_then(|m| m.get("mode"))
                .cloned()
                .unwrap_or_default()
        };

        let long_mode = run_one(long_text);
        let short_mode = run_one(short_text);
        assert!(long_mode.contains("log"), "long_mode={long_mode}");
        assert!(short_mode.contains("log"), "short_mode={short_mode}");
    }

    // ============================================================================
    // 端到端集成测试：意图解析 → 分派表 → 专用压缩器 → 压缩输出的完整链路
    // 断言经 Plugin::compress 真实管线（slice 文本、infer tool、explicit intent、
    // compact_*_for_tool 分派）后的 metadata mode、命令锚点与语义特征产物。
    // ============================================================================

    /// 测试辅助：读取指定插件的物理样本 case（禁止手写 mock 字符串）。
    fn read_e2e_sample(plugin: &str, case: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join(format!("vcs_{plugin}_plugin"))
            .join(format!("{case}.log"));
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("读取样本 {} 失败: {e}", path.display()))
    }

    /// 测试辅助：在 AI 紧凑模式下跑完整 compress 管线，返回输出文本与 metadata。
    /// profile=None 以验证「意图解析独立驱动分派」，不依赖调用方预设 profile。
    /// 注意：调用方（with_clean_vcs_mode 闭包）已持有 VCS_MODE_LOCK，此处不得重复取锁。
    fn run_e2e_compress(plugin: &VcsPlugin, text: &str) -> (String, HashMap<String, String>) {
        let slice = slice_from_text(text);
        let mut dict = DictionaryEngine::new();
        let mut dedup = DedupEngine::new(DedupConfig::default());
        let arena = bumpalo::Bump::new();
        let result = run_with_vcs_ai_context(true, VcsAiProfile::None, || {
            plugin.compress(&slice, &mut dict, &mut dedup, &arena)
        });
        let out = result
            .tokens
            .iter()
            .map(|t| match t {
                Token::Text(c) => c.to_string(),
                _ => String::new(),
            })
            .collect::<String>();
        let meta = result.metadata.unwrap_or_default();
        (out, meta)
    }

    /// 测试辅助：取文本首个非空行作为命令锚点预期值。
    fn e2e_anchor(text: &str) -> &str {
        text.lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim())
            .unwrap_or_default()
    }

    /// 端到端测试：4 个云端 CLI 变体（other 意图）从意图解析直达专用压缩输出。
    #[test]
    fn e2e_cloud_cli_other_intent_reaches_dedicated_compressed_output() {
        with_clean_vcs_mode(|| {
            let plugin = VcsPlugin::new();
            let cases: &[(&str, &str, &str, &str)] = &[
                // (插件, case, 期望 mode, 语义特征 token)
                (
                    "gh",
                    "case_92_gh_pr_list",
                    "ai-compact-other",
                    "#22 ST:open OW:@alice",
                ),
                (
                    "glab",
                    "case_95_glab_mr_list",
                    "ai-compact-other",
                    "!123 ST:open OW:@alice",
                ),
                (
                    "az",
                    "case_112_az_repos_list",
                    "ai-compact-other",
                    "PRJ:MyProject",
                ),
                (
                    "bitbucket",
                    "case_113_bitbucket_pr_list",
                    "ai-compact-other",
                    "#123 ST:OPEN OW:@alice",
                ),
            ];
            for (plugin_name, case, exp_mode, feature) in cases {
                let raw = read_e2e_sample(plugin_name, case);
                let (out, meta) = run_e2e_compress(&plugin, &raw);
                assert_eq!(
                    meta.get("tool").map(String::as_str),
                    Some("git"),
                    "{plugin_name} {case} 应归类 Git 家族"
                );
                assert_eq!(
                    meta.get("mode").map(String::as_str),
                    Some(*exp_mode),
                    "{plugin_name} {case} 意图分派错误，输出={out:?}"
                );
                assert!(
                    out.starts_with(e2e_anchor(&raw)),
                    "{plugin_name} {case} 命令锚点丢失（法则 0），输出={out:?}"
                );
                assert!(
                    out.contains(feature),
                    "{plugin_name} {case} 应含专用压缩语义特征 {feature}，输出={out:?}"
                );
            }
        });
    }

    /// 端到端测试：repo/gerrit 的 status/log 意图各自直达专用压缩输出。
    #[test]
    fn e2e_repo_and_gerrit_intent_reaches_dedicated_compressed_output() {
        with_clean_vcs_mode(|| {
            let plugin = VcsPlugin::new();
            let cases: &[(&str, &str, &str, &str)] = &[
                // repo status → status 分支（mode=ai-compact）
                (
                    "repo",
                    "case_116_repo_status",
                    "ai-compact",
                    "BR:master (clean)",
                ),
                // repo sync → log 分支（mode=ai-compact-log）
                ("repo", "case_100_repo_sync", "ai-compact-log", "@abc123def"),
                // gerrit query → log 分支（mode=ai-compact-log）
                (
                    "gerrit",
                    "case_97_gerrit_query",
                    "ai-compact-log",
                    "CHG @Iabc123def456789",
                ),
                // gerrit checkout → status 分支（mode=ai-compact）
                (
                    "gerrit",
                    "case_127_gerrit_checkout",
                    "ai-compact",
                    "(up-to-date)",
                ),
            ];
            for (plugin_name, case, exp_mode, feature) in cases {
                let raw = read_e2e_sample(plugin_name, case);
                let (out, meta) = run_e2e_compress(&plugin, &raw);
                assert_eq!(
                    meta.get("tool").map(String::as_str),
                    Some("git"),
                    "{plugin_name} {case} 应归类 Git 家族"
                );
                assert_eq!(
                    meta.get("mode").map(String::as_str),
                    Some(*exp_mode),
                    "{plugin_name} {case} 意图分派错误，输出={out:?}"
                );
                assert!(
                    out.starts_with(e2e_anchor(&raw)),
                    "{plugin_name} {case} 命令锚点丢失（法则 0），输出={out:?}"
                );
                assert!(
                    out.contains(feature),
                    "{plugin_name} {case} 应含专用压缩语义特征 {feature}，输出={out:?}"
                );
            }
        });
    }

    /// 端到端测试：repo/gerrit 的 other 意图分支各自直达专用压缩输出。
    #[test]
    fn e2e_repo_and_gerrit_other_intent_reaches_dedicated_compressed_output() {
        with_clean_vcs_mode(|| {
            let plugin = VcsPlugin::new();
            let cases: &[(&str, &str, &str, &str)] = &[
                // repo list → other 分支（mode=ai-compact-other）；项目根路径经路径
                // 字典重映射后仍保留语义（断言 [paths] 根映射片段）。
                (
                    "repo",
                    "case_115_repo_list",
                    "ai-compact-other",
                    "$P1=platform/build; $P2=platform/frameworks",
                ),
                // gerrit review → other 分支（mode=ai-compact-other），标签缩写并入锚点行
                (
                    "gerrit",
                    "case_125_gerrit_review",
                    "ai-compact-other",
                    "CR+2@alice",
                ),
            ];
            for (plugin_name, case, exp_mode, feature) in cases {
                let raw = read_e2e_sample(plugin_name, case);
                let (out, meta) = run_e2e_compress(&plugin, &raw);
                assert_eq!(
                    meta.get("tool").map(String::as_str),
                    Some("git"),
                    "{plugin_name} {case} 应归类 Git 家族"
                );
                assert_eq!(
                    meta.get("mode").map(String::as_str),
                    Some(*exp_mode),
                    "{plugin_name} {case} 意图分派错误，输出={out:?}"
                );
                assert!(
                    out.starts_with(e2e_anchor(&raw)),
                    "{plugin_name} {case} 命令锚点丢失（法则 0），输出={out:?}"
                );
                assert!(
                    out.contains(feature),
                    "{plugin_name} {case} 应含专用压缩语义特征 {feature}，输出={out:?}"
                );
            }
        });
    }

    /// 端到端测试：repo diff 经 diff 意图分支直达专用压缩输出。
    #[test]
    fn e2e_repo_diff_intent_reaches_dedicated_compressed_output() {
        with_clean_vcs_mode(|| {
            let plugin = VcsPlugin::new();
            let raw = read_e2e_sample("repo", "case_118_repo_diff");
            let (out, meta) = run_e2e_compress(&plugin, &raw);
            assert_eq!(
                meta.get("tool").map(String::as_str),
                Some("git"),
                "repo diff 应归类 Git 家族"
            );
            assert_eq!(
                meta.get("mode").map(String::as_str),
                Some("ai-compact-diff"),
                "repo diff 意图应走 diff 分支，输出={out:?}"
            );
            assert!(
                out.starts_with(e2e_anchor(&raw)),
                "repo diff 命令锚点丢失（法则 0），输出={out:?}"
            );
            assert!(!out.trim().is_empty(), "repo diff 输出不应为空");
        });
    }

    /// 测试：非 AI 模式下 6 个云端 CLI 命令头全部获得高分路由（方案 A 接线）。
    #[test]
    fn detect_scores_cloud_cli_command_heads_without_ai_mode() {
        with_clean_vcs_mode(|| {
            let plugin = VcsPlugin::new();
            for text in [
                "gh pr list\n#12 open Fix build\n",
                "glab mr list\n!34 draft UI polish\n",
                "az repos list\nName  Project\n",
                "bitbucket pr list\n#5 fix login\n",
                "repo status\nproject foo/bar ...\n",
                "gerrit query change:123\nchange 123\n",
            ] {
                let slice = slice_from_text(text);
                assert_eq!(
                    plugin.detect(&slice),
                    Some(0.95),
                    "云端 CLI 命令头应得 0.95 高分路由：{text:?}"
                );
            }
        });
    }

    /// 测试：命令头解析失败时，云端 CLI 块检测兜底归类 Git（方案 A 接线）。
    #[test]
    fn classify_tool_falls_back_to_cloud_cli_block_detection() {
        with_clean_vcs_mode(|| {
            let plugin = VcsPlugin::new();
            // 未闭合引号令 parse_command_line_tokens 失败，验证 is_gh_log_block 兜底。
            let text = "gh \"pr list\n#12 open Fix build\n";
            let slice = slice_from_text(text);
            assert!(plugin.detect(&slice).is_some());
        });
    }
}
