//! vcs_plugin test module — 中央调度器核心测试
//!
//! 各工具的具体测试已迁移到独立微插件，此处仅保留调度器基础设施测试。

#[cfg(test)]
mod tests {
    use crate::core::dedup_engine::{DedupConfig, DedupEngine};
    use crate::core::dictionary_engine::DictionaryEngine;
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::{Slice, SliceType};
    use crate::plugins::vcs_plugin::methods::{run_with_vcs_ai_context, VcsAiProfile};
    use crate::plugins::vcs_plugin::types::VcsPlugin;
    use once_cell::sync::Lazy;
    use std::borrow::Cow;
    use std::sync::Mutex;

    static VCS_MODE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn with_clean_vcs_mode<T>(f: impl FnOnce() -> T) -> T {
        let _guard = VCS_MODE_LOCK.lock().expect("vcs mode lock poisoned");
        run_with_vcs_ai_context(false, VcsAiProfile::None, f)
    }

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
}
