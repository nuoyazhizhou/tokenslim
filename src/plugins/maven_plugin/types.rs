/// maven plugin 类型定义
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MavenConfig {
    pub fold_javadoc_noise: bool,
    pub fold_download_noise: bool,
    pub extract_artifacts: bool,
}

impl Default for MavenConfig {
    /// 提供该插件类型的默认配置实现。
    fn default() -> Self {
        MavenConfig {
            fold_javadoc_noise: true,
            fold_download_noise: true,
            extract_artifacts: true,
        }
    }
}

pub struct MavenPlugin {
    pub name: &'static str,
    pub priority: u8,
    pub config: MavenConfig,
}
