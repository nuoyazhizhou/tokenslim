//! Ansible 插件类型定义。

pub struct AnsiblePlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
