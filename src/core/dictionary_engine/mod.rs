//! dictionary engine 模块
//!
//! # 模块概述
//!
//! 本模块实现了 TokenSlim 的 dictionary engine 功能。

mod methods;
mod types;
pub(crate) use methods::is_semantic_macro;
pub use types::{DictError, Dictionary, DictionaryEngine};
#[cfg(test)]
mod test;
