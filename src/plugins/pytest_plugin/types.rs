//! pytest 插件类型定义。

pub struct PytestPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
