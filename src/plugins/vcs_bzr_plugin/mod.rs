//! Bazaar脱水：保留命令锚点和文件状态，折叠噪音和空行，压缩长路径状态。

//! ## 保留信号
//! - bzr 命令锚点行（如 bzr status, bzr log）
//! - 文件状态行（modified, added 等）
//! - reverted 行（格式化为 ST:R）
//! - commit 行
//! - pull/merge/push 行
//! - 警告/错误信息（map_bzr_alert）

//! ## 压缩目标
//! - 噪音行（如进度条、统计信息等）
//! - 空行
//! - 重复 bzr 命令锚点（只保留第一个）
//! - 长路径状态文本（映射为短格式）
pub mod methods;
pub mod parser;
#[cfg(test)]
mod showcase;
#[cfg(test)]
mod tests;
