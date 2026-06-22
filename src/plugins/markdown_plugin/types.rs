/// markdown plugin 类型定义
use serde::{Deserialize, Serialize};

/// Markdown 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownConfig {
    pub extract_links: bool,
    pub extract_images: bool,
    pub remove_comments: bool,
}

impl Default for MarkdownConfig {
    /// 提供该插件类型的默认配置实现。
    fn default() -> Self {
        MarkdownConfig {
            extract_links: true,
            extract_images: true,
            remove_comments: true,
        }
    }
}

/// Markdown 插件结构
pub struct MarkdownPlugin {
    pub name: &'static str,
    pub priority: u8,
    pub config: MarkdownConfig,
}
