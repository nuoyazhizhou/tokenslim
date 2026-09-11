//! Android/Gradle 插件样本驱动测试。

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::android_gradle_plugin::AndroidGradlePlugin;
    use crate::plugins::test_utils::{compress_to_string, make_log_slice, read_sample_log};

    /// 验证含 Gradle 构建的样本能被 `detect` 命中。
    #[test]
    fn detects_android_gradle_case() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log("android_gradle_plugin", "case_001_gradle_build");
        assert!(plugin.detect(&make_log_slice(&raw)).is_some());
    }

    /// 验证 Gradle 构建样本压缩后不扩张（≤ 原文长度）。
    #[test]
    fn compresses_without_expansion() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log("android_gradle_plugin", "case_001_gradle_build");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.len() <= raw.len());
    }

    /// 验证通用 Gradle 任务样本压缩后含 `[GRADLE] tasks=` 与 BUILD SUCCESSFUL 且不扩张。
    #[test]
    fn compresses_generic_gradle_tasks() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log("android_gradle_plugin", "case_013_gradle_generic_build");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("[GRADLE] tasks="));
        assert!(out.contains("BUILD SUCCESSFUL"));
        assert!(out.len() <= raw.len());
    }

    /// 验证依赖下载样本压缩后含 `[GRADLE] downloads=` 且不扩张。
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

    /// 验证 daemon 失败样本压缩后保留 BUILD FAILED 与 FAILED 信号且不扩张。
    #[test]
    fn preserves_gradle_failure_signal() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log("android_gradle_plugin", "case_015_gradle_daemon_failure");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("BUILD FAILED"));
        assert!(out.contains("FAILED"));
        assert!(out.len() <= raw.len());
    }

    /// 验证 D8 dex 重复类冲突样本保留 `D8: Program type already present` 诊断行、BUILD SUCCESSFUL 且不扩张。
    #[test]
    fn preserves_d8_program_type_conflict() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log("android_gradle_plugin", "case_009_gradle_d8");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("D8: Program type already present"));
        assert!(out.contains("BUILD SUCCESSFUL"));
        assert!(out.len() <= raw.len());
    }

    /// 验证 GitHub Actions/GitLab/ConnectedAndroidTest 等 CI Gradle 样本均能被 `detect` 命中。
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

    /// 验证 CI Gradle 测试失败样本保留 `[GRADLE] tasks=`、测试名、失败标记与退出码且不扩张。
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

    /// 验证 Gradle 摘要前的 CI 锚点行（如 Buildkite agent 起始行）被保留在输出首行。
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

    /// 验证长行 Gradle 任务样本保留任务名（如 `:app:processDebugManifest`）且不被错误字典化为 `$M`。
    #[test]
    fn preserves_gradle_task_names_without_unresolved_macros() {
        let plugin = AndroidGradlePlugin::new();
        let raw = read_sample_log("android_gradle_plugin", "case_004_gradle_long_line");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("> Task :app:processDebugManifest"));
        assert!(!out.contains("$M"));
        assert!(out.len() <= raw.len());
    }

    /// 验证 ConnectedAndroidTest 失败样本保留测试名、测试计数、BUILD FAILED 且不扩张。
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
