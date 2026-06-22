//! Hg命令脱水：保留命令首行、变更集、文件操作摘要，折叠重复grafting和注释行。

//! ## 保留信号
//! - 命令首行（如hg update）
//! - 变更集ID（grafting行中提取）
//! - 文件操作摘要（如'3 updated, 2 removed'）
//! - 分支名（含~表示inactive）
//! - 日期（压缩为YYYY-MM-DD HH:MM格式）
//! - histedit命令（pick/edit/fold等）
//! - shelve列表条目（--list时）

//! ## 压缩目标
//! - 重复的grafting行（合并为一条，计数）
//! - 注释行（#开头）与空行
//! - 分支inactive状态（替换为~）
//! - 日期（压缩为紧凑格式）
//! - 跳过merging行（详细信息）
pub mod methods;
pub mod parser;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod tests;

pub use methods::*;
pub use parser::*;
