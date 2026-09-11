//! content analyzer 类型定义
//!
//! # 类型概述
//!
//! 本模块定义了 content analyzer 模块的核心数据类型。

/// 内容分析器主结构。
///
/// 无状态分析器：所有识别方法（文档级分类、候选插件提升、皮识别）均为纯函数，
/// 不依赖任何配置，因此本结构为零大小标记，仅作为 API 承载。
pub struct ContentAnalyzer;
