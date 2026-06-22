//! XML/HTML脱水：保留标签结构和文本内容，折叠标签间的空白字符，压缩冗余空格。

//! ## 保留信号
//! - XML/HTML标签（如 <div>）
//! - 标签属性（如 class="example"）
//! - 文本内容（标签之间的文字）

//! ## 压缩目标
//! - 标签间的空白字符（换行、缩进等）
pub mod methods;
pub mod types;

pub use types::*;

#[cfg(test)]
mod test;

#[cfg(test)]
mod showcase;
