//! Pulumi 插件类型定义。

/// Pulumi 基础设施即代码（IaC）部署输出压缩插件，提取资源增删改与错误信息。
pub struct PulumiPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
