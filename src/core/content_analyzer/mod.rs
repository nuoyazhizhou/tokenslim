//! content analyzer 模块
//!
//! # 模块概述
//!
//! 本模块实现了 TokenSlim 的 content analyzer 功能。
//!
//! ## 主要功能
//!
//! - 文档级语义分类（[`ContentAnalyzer::document_category`]）与剥皮类别识别（[`document_skin`](ContentAnalyzer::document_skin)）
//! - 切片级候选插件提升（[`ContentAnalyzer::candidate_plugins_for_slice`]，贝叶斯 + 配置样锚点前置）
//! - `drain` 子模块：日志模板聚类（Drain 算法）

pub mod drain;
mod methods;
mod types;
pub use types::ContentAnalyzer;
#[cfg(test)]
mod test;
