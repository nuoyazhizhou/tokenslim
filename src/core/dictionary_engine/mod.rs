//! dictionary engine 模块
//!
//! # 模块概述
//!
//! 本模块实现了 TokenSlim 的 dictionary engine 功能。

mod methods;
mod types;
pub use types::{DictError, DictType, Dictionary, DictionaryEngine, SemanticAliasRule};
#[cfg(test)]
mod test;
