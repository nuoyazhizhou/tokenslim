//! Git diff脱水：保留diff header和hunk header，折叠非关键行，压缩文件路径。

//! ## 保留信号
//! - diff --git行（文件差异标记）
//! - --- a/行（旧文件路径）
//! - +++ b/行（新文件路径）
//! - HUNK_HEADER行（块头部）
//! - index行（文件索引信息）

//! ## 压缩目标
//! - 非header行（如上下文内容）
//! - 文件路径简化（通过token_prefix）
pub mod methods;
pub mod types;

pub use types::*;

#[cfg(test)]
mod test;

#[cfg(test)]
mod showcase;
