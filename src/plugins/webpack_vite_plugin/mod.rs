//! Webpack/Vite 构建日志脱水：保留错误和警告行，折叠资产列表，压缩噪音行。

//! ## 保留信号
//! - ERROR in 行（编译错误）
//! - Module parse failed 行（模块解析失败）
//! - warning: 行（构建警告）
//! - ⚠️ 行（警告符号）

//! ## 压缩目标
//! - 噪音行（is_noise_line 过滤的废话行）
//! - 长路径（可能通过字典替换）
pub mod methods;
pub mod types;
pub use types::*;

#[cfg(test)]
mod showcase;
