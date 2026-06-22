//! 模板驱动脱水：保留模板结构，压缩可变部分为字典令牌。

//! ## 保留信号
//! - 匹配模板模式的行（保留模板固定文本）

//! ## 压缩目标
//! - 捕获组中变量部分（压缩为 $TEMPLA 字典令牌）
pub mod methods;
#[cfg(test)]
mod test;
pub mod types;

pub use types::*;

#[cfg(test)]
mod showcase;
