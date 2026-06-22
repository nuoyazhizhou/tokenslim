//! JSON脱水：保留JSON结构，折叠长字符串和路径，压缩为紧凑格式并添加前缀。

//! ## 保留信号
//! - { } 对象结构
//! - [ ] 数组结构
//! - 数字/布尔/null 值
//! - 短字符串（长度≤max_string_val_len）
//! - 键名（未启用字典化时）

//! ## 压缩目标
//! - 长字符串（长度>max_string_val_len）替换为字典令牌
//! - 键名（启用dictionaryize_keys时）替换为字典宏
//! - JSON文本压缩为单行紧凑格式
//! - 短JSON通过ROI门控避免膨胀
pub mod methods;
//
pub mod types;

pub use types::*;

#[cfg(test)]
mod test;

#[cfg(test)]
mod showcase;
