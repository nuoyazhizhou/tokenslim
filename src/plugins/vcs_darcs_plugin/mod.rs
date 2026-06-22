//! Darcs脱水：保留命令锚点与关键操作信息，折叠无关噪声和冗余行。

//! ## 保留信号
//! - darcs命令锚点行（如darcs log, darcs status）
//! - 补丁结构信息（hash, author, date, subject, files）
//! - Old message: 和 New message: 行（amend命令）
//! - Rebasing from: 和 Rebasing to: 行（rebase命令）
//! - 警报映射行（map_darcs_alert返回的非空内容）

//! ## 压缩目标
//! - 空行
//! - 噪声行（is_darcs_noise过滤的无关行）
//! - 长输出中超出cost_gate阈值的冗余行
//! - 通用fallback中第一条之后的darcs命令
pub mod methods;
pub mod parser;
#[cfg(test)]
mod showcase;
#[cfg(test)]
mod tests;
