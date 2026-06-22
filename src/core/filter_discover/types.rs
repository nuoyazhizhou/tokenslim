// filter_discover/types.rs
// 过滤器发现的数据类型定义

use serde::{Deserialize, Serialize};

/// Session 文件中的命令记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCommand {
    /// 原始命令
    pub command: String,
    /// 输入字节数
    pub input_bytes: Option<u64>,
    /// 输出字节数
    pub output_bytes: Option<u64>,
    /// 输入 Token 数
    pub input_tokens: Option<i64>,
    /// 输出 Token 数
    pub output_tokens: Option<i64>,
    /// 时间戳
    pub timestamp: Option<String>,
}

/// 命令分类
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandClass {
    /// 已被 tokenslim 包装
    AlreadyFiltered,
    /// 存在匹配的过滤器
    Filterable { filter_name: String },
    /// 无匹配过滤器
    NoFilter,
}

/// 分类后的命令
#[derive(Debug, Clone)]
pub struct ClassifiedCommand {
    pub command: SessionCommand,
    pub class: CommandClass,
}

/// 聚合的命令组
#[derive(Debug, Clone, Serialize)]
pub struct CommandGroup {
    /// 命令分组键（如 "git status", "cargo test"）
    pub key: String,
    /// 命令数量
    pub count: usize,
    /// 总输入字节数
    pub total_input_bytes: u64,
    /// 总输出字节数
    pub total_output_bytes: u64,
    /// 总输入 Token 数
    pub total_input_tokens: i64,
    /// 总输出 Token 数
    pub total_output_tokens: i64,
    /// 估算的节省百分比（从历史数据加载）
    pub estimated_savings_pct: Option<f64>,
    /// 估算的节省 Token 数
    pub estimated_tokens_saved: Option<i64>,
}

/// 发现结果
#[derive(Debug, Clone, Serialize)]
pub struct DiscoverResult {
    /// 已过滤的命令组
    pub already_filtered: Vec<CommandGroup>,
    /// 可过滤的命令组
    pub filterable: Vec<CommandGroup>,
    /// 无过滤器的命令组
    pub no_filter: Vec<CommandGroup>,
    /// 总命令数
    pub total_commands: usize,
    /// 估算的总潜在节省 Token 数
    pub total_potential_savings: i64,
}
