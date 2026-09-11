//! dotnet_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::dotnet_plugin::DotNetPlugin;
    use crate::plugins::test_utils::*;
    /// 验证真实 .NET 多行堆栈样本能被 `detect` 命中且置信度 ≥ 0.5。
    #[test]
    fn detects_real_stack_trace_sample() {
        let plugin = DotNetPlugin::new();
        let raw = read_sample_log("dotnet_plugin", "case_003_stack_trace");
        let score = plugin.detect(&make_log_slice(&raw));
        assert!(score.is_some(), "真实 .NET 多行堆栈应命中 detect");
        assert!(score.unwrap() >= 0.5);
    }

    /// 验证 .NET 堆栈样本压缩后不得显著扩张（允许 +4 字节内）。
    #[test]
    fn compresses_stack_trace_sample_without_expansion() {
        let plugin = DotNetPlugin::new();
        let raw = read_sample_log("dotnet_plugin", "case_003_stack_trace");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len() + 4,
            "dotnet 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }

    /// 验证 msbuild 样本压缩后不得显著扩张（允许 +4 字节内）。
    #[test]
    fn compresses_msbuild_sample_without_expansion() {
        let plugin = DotNetPlugin::new();
        let raw = read_sample_log("dotnet_plugin", "case_004_msbuild");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len() + 4,
            "dotnet 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }
}
