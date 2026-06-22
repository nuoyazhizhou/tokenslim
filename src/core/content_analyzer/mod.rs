//! content analyzer 模块
//!
//! # 模块概述
//!
//! 本模块实现了 TokenSlim 的 content analyzer 功能。
//!
//! ## 主要功能
//!
//! - 提供核心类型定义和接口
//! - 协调各子组件的工作流程
//! - 对外提供统一的 API 接口

pub mod drain;
mod methods;
mod types;
pub use types::{
    AnalysisResult, AnalyzerConfig, AnalyzerError, ContentAnalyzer, QuickAnalysisResult, Rule,
};
#[cfg(test)]
mod test;
