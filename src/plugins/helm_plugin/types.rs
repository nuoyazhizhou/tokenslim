//! Helm 插件类型定义。

/// Helm 安装/升级/回滚日志压缩插件主体：提取 release 元字段、汇总部署资源、保留错误信号。
pub struct HelmPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
