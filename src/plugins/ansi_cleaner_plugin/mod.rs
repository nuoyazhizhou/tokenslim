//! ANSI脱水：保留文本内容，移除ANSI控制码，压缩进度条覆盖，丢弃空行。

//! ## 保留信号
//! - 非空文本行（保留有效内容）
//! - 进度条最后有效状态（保留最后一次更新）

//! ## 压缩目标
//! - ANSI转义序列（如\x1b[31m）
//! - 回车符\r导致的进度条历史记录（仅保留最后一行）
//! - 空白行（剥离后空行丢弃）
pub mod methods;
//
pub mod types;

pub use types::*;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
