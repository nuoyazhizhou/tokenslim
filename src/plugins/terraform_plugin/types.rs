//! Terraform 插件类型定义。

pub struct TerraformPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
