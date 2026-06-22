//! 智能路径脱水：保留路径上下文，折叠长路径为字典令牌，压缩重复路径。

//! ## 保留信号
//! - 非路径文本（路径之外的文本，原样保留）

//! ## 压缩目标
//! - 文件路径（替换为字典令牌）
pub mod methods;
///
pub mod types;

pub use types::*;

#[cfg(test)]
mod showcase;
