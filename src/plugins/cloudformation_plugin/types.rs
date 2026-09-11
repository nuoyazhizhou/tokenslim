//! CloudFormation 插件类型定义。

/// CloudFormation 变更集/事件日志压缩插件主体：统计 `CREATE_IN_PROGRESS`/`ROLLBACK` 等状态、保留失败事件。
pub struct CloudFormationPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
