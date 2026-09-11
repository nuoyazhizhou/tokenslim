//! 压缩核心模块
//!
//! 汇总压缩产物的核心数据结构（类型定义在 [`types`] 子模块）与压缩能力编排入口，
//! 是 `CompressionPipeline` 等上层流程依赖的基础数据层。
//!
//! ## 子模块
//!
//! - [`types`]：压缩产物类型（`Token`/`MarkerKind`/`CompressionOutput`/`CompressionMetadata` 等）。

mod types;
pub use types::*;
