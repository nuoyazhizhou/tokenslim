//! YAML脱水：保留键值结构，折叠长序列，压缩键标识符。

//! ## 保留信号
//! - 有效YAML的映射键值对结构（键被字典化）
//! - 序列的前max_seq_len个元素
//! - YAML解析失败时的原始文本

//! ## 压缩目标
//! - 长序列截断（超过max_seq_len部分替换为$SEQ-$n占位符）
//! - 映射键替换为字典宏
//! - YAML缩进与空白压缩为紧凑格式
//! - 深度超过max_depth时替换为"...depth limit..."
pub mod methods;
pub mod types;

pub use types::*;

#[cfg(test)]
mod test;

#[cfg(test)]
mod showcase;
