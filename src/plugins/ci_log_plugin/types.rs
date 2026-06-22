//! CI/CD 外壳日志插件类型定义。

pub struct CiLogPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
