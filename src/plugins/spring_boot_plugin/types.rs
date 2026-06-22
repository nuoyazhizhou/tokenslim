/// spring boot plugin 类型定义
use serde::{Deserialize, Serialize};

/// Spring Boot 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpringBootConfig {
    /// 是否折叠 Maven 下载日志
    pub fold_maven_downloads: bool,
    /// 是否合并 Spring 生命周期日志
    pub merge_lifecycle_logs: bool,
    /// 是否提取 Bean 名称和包名到字典
    pub extract_beans_packages: bool,
}

impl Default for SpringBootConfig {
    /// 提供该插件类型的默认配置实现。
    fn default() -> Self {
        SpringBootConfig {
            fold_maven_downloads: true,
            merge_lifecycle_logs: true,
            extract_beans_packages: true,
        }
    }
}

/// Spring Boot 插件结构
pub struct SpringBootPlugin {
    pub name: &'static str,
    pub priority: u8,
    pub config: SpringBootConfig,
}
