//! Android/Gradle 插件样本驱动测试。

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::android_gradle_plugin::AndroidGradlePlugin;
    use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_log};

    #[test]
    fn detects_android_gradle_case() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log("android_gradle_plugin", "case_001_gradle_build");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    #[test]
    fn compresses_without_expansion() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log("android_gradle_plugin", "case_001_gradle_build");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.len() <= raw.len());
    }

    #[test]
    fn compresses_generic_gradle_tasks() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log("android_gradle_plugin", "case_013_gradle_generic_build");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("[GRADLE] tasks="));
        assert!(out.contains("BUILD SUCCESSFUL"));
        assert!(out.len() <= raw.len());
    }

    #[test]
    fn compresses_gradle_dependency_downloads() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log(
            "android_gradle_plugin",
            "case_014_gradle_dependency_download",
        );
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("[GRADLE] downloads="));
        assert!(out.len() <= raw.len());
    }

    #[test]
    fn preserves_gradle_failure_signal() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log("android_gradle_plugin", "case_015_gradle_daemon_failure");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("BUILD FAILED"));
        assert!(out.contains("FAILED"));
        assert!(out.len() <= raw.len());
    }

    #[test]
    fn detects_ci_gradle_wrappers() {
        let plugin = AndroidGradlePlugin::new();
        for case_id in [
            "case_016_github_actions_gradle_test_failure",
            "case_017_gitlab_gradle_wrapper_failure",
            "case_019_gradle_connected_android_test_failure",
        ] {
            let raw = read_sample_log("android_gradle_plugin", case_id);
            assert!(plugin.detect(&make_log_slice(&raw)).is_some());
        }
    }

    #[test]
    fn preserves_ci_gradle_test_failure_signals() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log(
            "android_gradle_plugin",
            "case_016_github_actions_gradle_test_failure",
        );
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("[GRADLE] tasks="));
        assert!(out.contains("testDebugUnitTest"));
        assert!(out.contains("There were failing tests."));
        assert!(out.contains("BUILD FAILED"));
        assert!(out.contains("Process completed with exit code 1"));
        assert!(out.len() <= raw.len());
    }

    #[test]
    fn preserves_ci_anchor_before_gradle_summary() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log(
            "android_gradle_plugin",
            "case_020_gradle_kotlin_compile_failure",
        );
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert_eq!(
            out.lines().next(),
            Some("Buildkite agent 3.67.0 starting android pipeline step")
        );
        assert!(out.contains("[GRADLE] tasks="));
        assert!(out.len() <= raw.len());
    }

    #[test]
    fn preserves_gradle_task_names_without_unresolved_macros() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log("android_gradle_plugin", "case_004_gradle_long_line");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("> Task :app:processDebugManifest"));
        assert!(!out.contains("$M"));
        assert!(out.len() <= raw.len());
    }

    #[test]
    fn preserves_connected_android_test_failure_signal() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log(
            "android_gradle_plugin",
            "case_019_gradle_connected_android_test_failure",
        );
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("connectedDebugAndroidTest"));
        assert!(out.contains("Finished 6 tests"));
        assert!(out.contains("BUILD FAILED"));
        assert!(out.len() <= raw.len());
    }
}
