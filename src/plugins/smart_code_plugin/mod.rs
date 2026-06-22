//! 代码脱水：保留内置异常和关键字标识符，折叠长自定义标识符为字典令牌，压缩连续空格为长度标记。

//! ## 保留信号
//! - SyntaxError（JavaScript / Python 内置异常）
//! - public（关键字）
//! - short_id（长度≤8的标识符）

//! ## 压缩目标
//! - 长自定义标识符（长度>8且非关键字非保留异常）压缩为$PK令牌
//! - 连续空格压缩为$S|长度标记
pub mod methods;
//
pub mod types;

pub use types::*;

#[cfg(test)]
mod test;

#[cfg(test)]
mod showcase;
