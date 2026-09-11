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
    /// TreeConfig 默认值：默认路径正则、最少 4 文件、共享深度 1、折叠单孩子、排序、Unicode 风格。
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
    /// 返回 `RenderStyle` 的默认值 `Unicode`，即树形重排渲染采用 Unicode 连线风格。
    fn default() -> Self {
        Self::Unicode
    }
}

/// 返回默认路径匹配正则（匹配常见文件路径字符集）。
///
/// P2-61：默认正则要求至少一个路径分隔符（`/` 或 `\`）。旧默认
/// `([a-zA-Z0-9_./\\-]+)` 不要求分隔符，导致 `"M  src/main.rs"` 的首个捕获
/// 是状态字母 `"M"` 而非路径——同状态多行在 Trie 中坍缩为单组件叶子，整棵
/// 文件列表被吞（信息丢失级缺陷）。要求分隔符后，纯状态字母不再被误捕。
fn default_path_pattern() -> String {
    r"([a-zA-Z0-9_./\\-]+[/\\][a-zA-Z0-9_./\\-]+)".to_string()
}

/// 返回最少匹配文件数门控默认值（4）。
fn default_min_files() -> usize {
    4
}

/// 返回最少共享深度门控默认值（1）。
fn default_min_shared_depth() -> usize {
    1
}

/// 返回是否折叠单孩子目录的默认值（true）。
fn default_collapse_single_child() -> bool {
    true
}

/// 返回是否排序的默认值（true）。
fn default_sort() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试：TreeConfig 默认值与各字段默认函数一致。
    #[test]
    fn test_default_config() {
        let config = TreeConfig::default();
        assert_eq!(config.min_files, 4);
        assert_eq!(config.min_shared_depth, 1);
        assert!(config.collapse_single_child);
        assert!(config.sort);
        assert_eq!(config.style, RenderStyle::Unicode);
    }

    /// 测试：RenderStyle 的 serde 序列化为小写字符串（如 "unicode"/"ascii"）且可反序列化。
    #[test]
    fn test_render_style_serde() {
        let style = RenderStyle::Unicode;
        let json = serde_json::to_string(&style).unwrap();
        assert_eq!(json, "\"unicode\"");

        let style: RenderStyle = serde_json::from_str("\"ascii\"").unwrap();
        assert_eq!(style, RenderStyle::Ascii);
    }
}
