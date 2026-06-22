//! Node.js错误脱水：保留异常类名与消息，折叠堆栈帧为紧凑令牌，压缩自定义类名和文件路径。

//! ## 保留信号
//! - Error, SyntaxError 等内置异常类名（白名单保留字面量）
//! - 以 Error 或 Exception 结尾的自定义类名
//! - 异常消息（msg）
//! - 堆栈帧中的函数名（func）
//! - 堆栈帧中的行号（line）
//! - 堆栈帧中的列号（col）
//! - <anonymous> 文件（保留字面量）

//! ## 压缩目标
//! - 非白名单自定义异常类名（字典化）
//! - 堆栈帧中的文件路径（字典化，除 <anonymous>）
//! - 缩进符（转换为数字编码）
pub mod methods;
//
pub mod types;

pub use types::*;

#[cfg(test)]
mod test;

#[cfg(test)]
mod showcase;
