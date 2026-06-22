/// git diff plugin 类型定义
use serde::{Deserialize, Serialize};

/// Git Diff 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitDiffConfig {
    /// 保留的上下文行数
    #[serde(default = "default_context_lines")]
    pub context_lines: usize,
    /// 是否汇总未修改的文件
    #[serde(default = "default_true")]
    pub summarize_unmodified: bool,
    /// 是否对文件路径进行字典化
    #[serde(default = "default_true")]
    pub dictionaryize_paths: bool,
}

/// 内部辅助函数：执行与 default context lines 相关的具体逻辑。
fn default_context_lines() -> usize {
    1
}
/// 内部辅助函数：执行与 default true 相关的具体逻辑。
fn default_true() -> bool {
    true
}

impl Default for GitDiffConfig {
    /// 提供该插件类型的默认配置实现。
    fn default() -> Self {
        Self {
            context_lines: default_context_lines(),
            summarize_unmodified: true,
            dictionaryize_paths: true,
        }
    }
}

/// Git Diff 压缩插件主结构
pub struct GitDiffPlugin {
    pub name: &'static str,
    pub priority: u8,
    pub config: GitDiffConfig,
}
