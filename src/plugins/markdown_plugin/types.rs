/// markdown plugin 类型定义
use serde::{Deserialize, Serialize};

/// Markdown 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownConfig {
    pub remove_comments: bool,
}

impl Default for MarkdownConfig {
    /// 构造 MarkdownConfig 默认配置：移除注释开启。
    fn default() -> Self {
        MarkdownConfig {
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
