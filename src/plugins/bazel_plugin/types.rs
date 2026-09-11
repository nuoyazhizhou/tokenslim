//! Bazel 插件类型定义。

/// Bazel 构建/测试日志压缩插件主体：识别 `bazel build/test`、版本、query 目标与构建状态信号。
pub struct BazelPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
