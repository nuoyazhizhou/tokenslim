//! Bazel 插件类型定义。

pub struct BazelPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
