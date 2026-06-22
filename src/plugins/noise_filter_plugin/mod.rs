//! 噪声过滤：检测并替换二进制数据为简短标记，保留纯文本内容。

//! ## 保留信号
//! - 纯文本行（非二进制控制字符）

//! ## 压缩目标
//! - 二进制数据（替换为 [BINARY_DATA: Size=..., MD5=...] 标记）
pub mod methods;
#[cfg(test)]
mod test;
pub mod types;

pub use types::*;

#[cfg(test)]
mod showcase;
