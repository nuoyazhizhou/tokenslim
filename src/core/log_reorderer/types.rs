use serde::{Deserialize, Serialize};

/// 日志重排序配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderConfig {
    /// 是否启用全局重排序
    pub enabled: bool,
    /// 最大缓冲行数，防止内存溢出
    pub max_lines: usize,
    /// 是否尝试将无 Key 的行附着到前一个 Context
    pub sticky_context: bool,
    /// 是否强制全局上下文键按字母排序（消除 -jN 并发乱序产生的随机块顺序）
    pub deterministic_sort: bool,
}

impl Default for ReorderConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_lines: 100_000,
            sticky_context: true,
            deterministic_sort: true,
        }
    }
}

/// 重排序规则定义
pub struct ReorderRule {
    pub name: &'static str,
    pub pattern: regex::Regex,
    pub key_group: usize,
}
