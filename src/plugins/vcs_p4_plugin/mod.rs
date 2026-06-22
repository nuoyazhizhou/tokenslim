//! P4脱水：保留change编号、文件操作与结果，折叠统计摘要，压缩长路径和日期。

//! ## 保留信号
//! - change 编号（如 1234）
//! - 文件操作（如 opened, edit, add）
//! - 精简后的 depot 路径（并压缩公共前缀）
//! - 命令结果（如 resolved, would-update）
//! - 差异统计摘要（文件数:增加-删除）

//! ## 压缩目标
//! - 长路径替换为公共前缀（$VCS_P4）
//! - 日期时间截断为 19 字符 YYYY-MM-DD HH:MM:SS
//! - 文件大小数字转人类可读（如 1234567 -> 1.2M）
//! - diff 头行（---/+++）丢弃
//! - 叙事性噪音行丢弃（如 p4 narrative noise）
//! - 同步预览摘要压缩为“数 would-update”
pub mod methods;
pub mod parser;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod showcase;
