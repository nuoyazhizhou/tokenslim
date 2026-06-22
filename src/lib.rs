//! TokenSlim - AI 输入解压缩引擎
//!
//! # 项目概述
//!
//! TokenSlim 是一个专门用于压缩日志、代码等文本内容的 Rust 库，旨在减少 LLM Token 使用量。
//! 通过智能识别和压缩重复内容、建立字典引用、使用结构标记等技术，实现高效的文本压缩。
//!
//! ## 主要特性
//!
//! - **智能内容分析**: 自动识别日志、代码、堆栈跟踪等不同类型的文本
//! - **高效压缩**: 使用字典引用、重复压缩、结构标记等多种压缩策略
//! - **插件架构**: 支持针对不同类型内容的专用压缩插件
//! - **无损还原**: 完整的元数据和字典信息确保压缩内容可以完全还原
//! - **流式处理**: 支持大文件的流式读取和处理，内存占用低
//!
//! ## 核心模块
//!
//! - [`core`] - 核心压缩引擎，包含所有基础组件
//! - [`plugins`] - 压缩插件集合，提供针对不同类型内容的压缩实现
//! - [`cli`] - 命令行接口，提供用户友好的操作界面
//! - [`utils`] - 工具函数，提供国际化等辅助功能
//!
//! ## 使用示例
//!
//! ```rust,ignore
//! use tokenslim::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
//! use tokenslim::core::metrics::{MetricsCollector, MetricsConfig};
//!
//! // 创建压缩流水线
//! let config = PipelineConfig::default();
//! let metrics = MetricsCollector::new(MetricsConfig::default());
//! let mut pipeline = CompressionPipeline::new(config, vec![], metrics);
//!
//! // 压缩文件
//! let result = pipeline.compress_file("input.log").unwrap();
//!
//! // 输出压缩结果
//! println!("压缩率：{}", result.metadata.compression_ratio);
//! ```
//!
//! ## 架构设计
//!
//! TokenSlim 采用流水线架构，主要包含以下处理阶段：
//!
//! 1. **流式读取** ([`stream_reader`]) - 从文件或字符串读取文本流
//! 2. **文本切片** ([`text_slicer`]) - 将文本切分成合适的处理单元
//! 3. **内容分析** ([`content_analyzer`]) - 分析每个切片的类型和特征
//! 4. **插件分发** ([`plugin_dispatcher`]) - 根据类型分发给合适的压缩插件
//! 5. **压缩处理** - 使用字典引擎和去重引擎进行压缩
//! 6. **指标收集** ([`metrics`]) - 收集压缩过程的性能指标
//!
//! ## 性能指标
//!
//! TokenSlim 提供详细的性能指标，包括：
//! - 压缩率（compression_ratio）
//! - Token 节省率（token_savings）
//! - 处理耗时（processing_time_ms）
//! - 内存占用

pub mod cli;
pub mod core;
pub mod plugins;
pub mod utils;
