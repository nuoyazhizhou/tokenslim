//! SQL脱水：保留SQL语句结构，折叠超长INSERT VALUES内容。

//! ## 保留信号
//! - SQL关键字（SELECT、INSERT等）所在行
//! - INSERT语句的前缀部分（不被截断）
//! - 短SQL语句（完整保留）

//! ## 压缩目标
//! - 长INSERT VALUES（超过max_insert_values_len时截断为占位符）
pub mod methods;
#[cfg(test)]
mod test;
pub mod types;

pub use types::*;

#[cfg(test)]
mod showcase;
