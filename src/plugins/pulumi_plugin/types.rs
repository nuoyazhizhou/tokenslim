//! Pulumi 插件类型定义。

pub struct PulumiPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
