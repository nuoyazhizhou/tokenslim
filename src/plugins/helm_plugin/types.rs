//! Helm 插件类型定义。

pub struct HelmPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
