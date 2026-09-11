//! Ansible 插件类型定义。

/// Ansible playbook/任务日志压缩插件主体：识别 `PLAY`/`TASK`/`RECAP` 等信号并做语义摘要。
pub struct AnsiblePlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
