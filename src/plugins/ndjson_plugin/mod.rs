//! NDJSON脱水：保留首尾行，折叠中间行，压缩测试摘要。

//! ## 保留信号
//! - 前5行（保留开头）
//! - 后5行（保留结尾）

//! ## 压缩目标
//! - 中间行（折叠为省略行）
pub mod methods;
pub mod types;

pub use types::*;

#[cfg(test)]
mod test;

#[cfg(test)]
mod showcase;
