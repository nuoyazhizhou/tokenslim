//! text slicer 模块
//!
//! # 模块概述
//!
//! 本模块实现了 TokenSlim 的 text slicer 功能。
//!
//! ## 主要功能
//!
//! - 提供核心类型定义和接口
//! - 协调各子组件的工作流程
//! - 对外提供统一的 API 接口

pub mod config_loader;
pub(crate) mod methods;
mod types;

pub use config_loader::ConfigLoader;
pub use methods::get_framework_configs;
pub use types::{
    DetectionRule, FeaturePattern, FeaturePatternType, FrameworkDetectionConfig, Slice, SliceFlags,
    SliceId, SliceMode, SliceType, SlicerConfig, TextSlicer,
};

#[cfg(test)]
mod test;
