//! Maven 构建日志脱水：保留错误/警告和构建结果，折叠下载进度行。

//! ## 保留信号
//! - [ERROR] 行
//! - [WARNING] 行
//! - BUILD SUCCESS / BUILD FAILURE
//! - Tests run: 测试摘要

//! ## 压缩目标
//! - [INFO] 下载进度行折叠
//! - 重复 [INFO] 行去重
pub mod methods;
#[cfg(test)]
mod test;
pub mod types;

pub use types::*;

#[cfg(test)]
mod showcase;
