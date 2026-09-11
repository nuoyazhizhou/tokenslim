//! CI/CD 外壳日志插件类型定义。

/// CI/CD 外壳日志（GitHub Actions / GitLab / Jenkins / CircleCI 等）压缩插件主体：剥离时间戳前缀、按步骤聚合汇总。
pub struct CiLogPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
}
