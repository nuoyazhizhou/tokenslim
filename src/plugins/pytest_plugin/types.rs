//! pytest 插件类型定义。

/// pytest 测试运行输出压缩插件，提取测试通过/失败/跳过统计与关键失败行。
pub struct PytestPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
