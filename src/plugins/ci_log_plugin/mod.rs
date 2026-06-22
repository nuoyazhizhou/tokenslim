//! CI/CD日志脱水：保留步骤、错误、警告和状态信号，折叠详细日志行为统计摘要。

//! ## 保留信号
//! - ::error 行（错误信号）
//! - ::warning 行（警告信号）
//! - ::group::/section_start: 行（步骤起始）
//! - ::endgroup::/section_end: 行（步骤结束）
//! - ##[error] 行（Azure错误）
//! - finished: failure 行（Job失败）
//! - process completed with exit code 行（退出码）

//! ## 压缩目标
//! - 步骤内的非关键日志行（折叠为行数统计）
//! - 缓存操作行（压缩为缓存计数）
//! - 重试操作行（压缩为重试计数）
//! - 空行（完全丢弃）
mod methods;
mod types;

pub use types::CiLogPlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
