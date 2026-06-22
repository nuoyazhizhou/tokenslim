//! Markdown脱水：保留标题、链接、图片、列表等核心结构，折叠注释以压缩体积。

//! ## 保留信号
//! - # 标题行
//! - [链接]或![图片]
//! - - 或 * 或 1. 列表项

//! ## 压缩目标
//! - HTML/XML注释（如 <!-- comment -->）
pub mod methods;
#[cfg(test)]
mod test;
pub mod types;

pub use types::*;

#[cfg(test)]
mod showcase;
