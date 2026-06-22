//! unity_unreal_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::test_utils::*;
    use crate::plugins::unity_unreal_plugin::UnityUnrealPlugin;
    #[test]
    fn compresses_unity_log_sample_without_expansion() {
        let plugin = UnityUnrealPlugin::new();
        let raw = read_sample_log("unity_unreal_plugin", "case_001_unity_log");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len() + 4,
            "unity_unreal 插件压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn compresses_unreal_log_sample_without_expansion() {
        let plugin = UnityUnrealPlugin::new();
        let raw = read_sample_log("unity_unreal_plugin", "case_002_unreal_log");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len() + 4,
            "unity_unreal 插件压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    #[test]
    fn compresses_performance_sample_without_expansion() {
        let plugin = UnityUnrealPlugin::new();
        let raw = read_sample_log("unity_unreal_plugin", "case_010_performance");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len() + 4,
            "unity_unreal 插件压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 真实 Unreal Engine 日志（含 `LogUObject`/`LogHAL`/`LogLinker` 等内置 log category）。
    #[test]
    fn detects_real_unreal_logs_sample() {
        let plugin = UnityUnrealPlugin::new();
        let raw = read_sample_log("unity_unreal_plugin", "case_013_real_unreal_logs");
        let score = plugin.detect(&make_log_slice(&raw));
        assert!(score.is_some(), "真实 Unreal 日志应命中 detect");
        assert!(score.unwrap() >= 0.8);
    }

    /// 真实 Unity 日志（含 `Unloading `/`Building AssetBundle`/`Shader compilation`）。
    #[test]
    fn detects_real_unity_logs_sample() {
        let plugin = UnityUnrealPlugin::new();
        let raw = read_sample_log("unity_unreal_plugin", "case_014_real_unity_logs");
        let score = plugin.detect(&make_log_slice(&raw));
        assert!(score.is_some(), "真实 Unity 日志应命中 detect");
        assert!(score.unwrap() >= 0.8);
    }

    /// syslog 格式的日志里可能出现 "Loading"/"game" 等近似词，但缺少 `Log<UObject/HAL/...>` 等
    /// Unity/Unreal 专属关键字，也没有 `.uasset/.prefab/.mat` 后缀，应 detect=None。
    #[test]
    fn does_not_detect_syslog_as_unity_unreal() {
        let plugin = UnityUnrealPlugin::new();
        let raw = read_sample_log("unity_unreal_plugin", "case_015_looks_unreal_but_syslog");
        assert!(
            plugin.detect(&make_log_slice(&raw)).is_none(),
            "unity_unreal 不应把 syslog 误识别为游戏引擎日志"
        );
    }
}
