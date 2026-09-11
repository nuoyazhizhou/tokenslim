/// maven plugin 类型定义
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Maven 插件配置：是否折叠 Javadoc 噪声、依赖下载噪声。
pub struct MavenConfig {
    pub fold_javadoc_noise: bool,
    pub fold_download_noise: bool,
}

impl Default for MavenConfig {
    /// 提供该插件类型的默认配置实现。
    fn default() -> Self {
        MavenConfig {
            fold_javadoc_noise: true,
            fold_download_noise: true,
        }
    }
}

/// Maven 构建日志压缩插件主体：折叠 javac 警告/错误、JUnit 测试摘要、依赖下载，并在末尾追加构建摘要。
pub struct MavenPlugin {
    pub name: &'static str,
    pub priority: u8,
    pub config: MavenConfig,
}
