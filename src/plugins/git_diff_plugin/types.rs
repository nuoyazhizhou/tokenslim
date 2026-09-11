/// git diff plugin 类型定义
use serde::{Deserialize, Serialize};

/// Git Diff 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitDiffConfig {
    /// 保留的上下文行数
    #[serde(default = "default_context_lines")]
    pub context_lines: usize,
}

/// 返回保留上下文行数的默认值（1）。
fn default_context_lines() -> usize {
    1
}

impl Default for GitDiffConfig {
    /// 构造 GitDiffConfig 默认配置：上下文 1 行。
    fn default() -> Self {
        Self {
            context_lines: default_context_lines(),
        }
    }
}

/// Git Diff 压缩插件主结构
pub struct GitDiffPlugin {
    pub name: &'static str,
    pub priority: u8,
    pub config: GitDiffConfig,
}
