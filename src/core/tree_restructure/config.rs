// tree_restructure/config.rs
// 树结构配置

use serde::{Deserialize, Serialize};

/// 树结构配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeConfig {
    /// 路径匹配正则表达式
    #[serde(default = "default_path_pattern")]
    pub path_pattern: String,

    /// 最少匹配文件数（门控）
    #[serde(default = "default_min_files")]
    pub min_files: usize,

    /// 最少共享深度（门控）
    #[serde(default = "default_min_shared_depth")]
    pub min_shared_depth: usize,

    /// 是否折叠单孩子目录
    #[serde(default = "default_collapse_single_child")]
    pub collapse_single_child: bool,

    /// 是否排序
    #[serde(default = "default_sort")]
    pub sort: bool,

    /// 渲染风格
    #[serde(default)]
    pub style: RenderStyle,
}

impl Default for TreeConfig {
    fn default() -> Self {
        Self {
            path_pattern: default_path_pattern(),
            min_files: default_min_files(),
            min_shared_depth: default_min_shared_depth(),
            collapse_single_child: default_collapse_single_child(),
            sort: default_sort(),
            style: RenderStyle::default(),
        }
    }
}

/// 渲染风格
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderStyle {
    /// Unicode 框线风格 (├─ │  └─)
    Unicode,
    /// ASCII 风格 (|- |  `- )
    Ascii,
    /// 纯缩进风格
    Indent,
}

impl Default for RenderStyle {
    fn default() -> Self {
        Self::Unicode
    }
}

fn default_path_pattern() -> String {
    r"([a-zA-Z0-9_./\\-]+)".to_string()
}

fn default_min_files() -> usize {
    4
}

fn default_min_shared_depth() -> usize {
    1
}

fn default_collapse_single_child() -> bool {
    true
}

fn default_sort() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TreeConfig::default();
        assert_eq!(config.min_files, 4);
        assert_eq!(config.min_shared_depth, 1);
        assert!(config.collapse_single_child);
        assert!(config.sort);
        assert_eq!(config.style, RenderStyle::Unicode);
    }

    #[test]
    fn test_render_style_serde() {
        let style = RenderStyle::Unicode;
        let json = serde_json::to_string(&style).unwrap();
        assert_eq!(json, "\"unicode\"");

        let style: RenderStyle = serde_json::from_str("\"ascii\"").unwrap();
        assert_eq!(style, RenderStyle::Ascii);
    }
}
