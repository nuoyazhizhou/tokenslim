//! Terraform 插件类型定义。

/// Terraform 基础设施即代码（IaC）执行输出压缩插件，提取资源增删改与错误。
pub struct TerraformPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
