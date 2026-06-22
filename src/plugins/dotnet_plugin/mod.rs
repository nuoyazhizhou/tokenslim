//! .NET构建日志脱水：保留错误定位和堆栈帧方法名，折叠冗余参数。

//! ## 保留信号
//! - at method(...) in file:line（堆栈帧行）
//! - file(line,col): error code: message（MSBuild错误行）
//! - 包含System./Microsoft.的行（疑似.NET相关）

//! ## 压缩目标
//! - 堆栈帧参数列表（替换为(...)）
pub mod methods;
pub mod types;

pub use types::*;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
