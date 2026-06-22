//! CloudFormation 插件类型定义。

pub struct CloudFormationPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
