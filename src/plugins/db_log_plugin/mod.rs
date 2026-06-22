//! 数据库日志脱水：保留进程ID、持续时间、日志级别、查询标签等关键字段，折叠冗余信息，压缩为紧凑格式。

//! ## 保留信号
//! - pid（进程ID）
//! - duration（持续时间毫秒）
//! - level（日志级别，如LOG、ERROR）
//! - query（压缩后的查询标签）
//! - msg（日志消息主体）

//! ## 压缩目标
//! - 原始时间戳（被移除）
//! - 原始长查询（被compact_query_label截断或替换）
//! - 普通duration（仅保留数字，标记SLOW或DUR）
//! - ROI门控：若压缩后体积更大则回退原文
mod methods;
mod types;
pub use types::DbLogPlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
