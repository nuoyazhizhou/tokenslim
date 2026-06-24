//! TokenSlim 核心模块
//!
//! # 模块概述
//!
//! 本模块包含 TokenSlim 文本压缩引擎的核心功能组件，提供完整的文本压缩、去重、
//! 字典管理、插件调度等能力。
//!
//! ## 核心子模块
//!
//! - [`compression`] - 压缩基础类型定义，包括 Token 类型和压缩输出结构
//! - [`compression_pipeline`] - 压缩流水线，整合各组件完成端到端的压缩流程
//! - [`content_analyzer`] - 内容分析器，分析文本切片的类型和特征
//! - [`dedup_engine`] - 去重引擎，识别并压缩重复的文本内容
//! - [`dictionary_engine`] - 字典引擎，管理压缩过程中的文本字典映射
//! - [`error_isolation`] - 错误隔离执行器，确保插件执行的安全性和稳定性
//! - [`metrics`] - 指标收集器，收集和统计压缩过程中的各项性能指标
//! - [`plugin_dispatcher`] - 插件调度器，根据内容类型分发给合适的压缩插件
//! - [`plugin_config_loader`] - 插件配置加载器，从配置文件加载插件规则
//! - [`rehydration_pipeline`] - 还原流水线，将压缩后的 Token 还原为原始文本
//! - [`stream_reader`] - 流式读取器，支持从文件或字符串读取文本流
//! - [`text_slicer`] - 文本切片器，将输入文本切分成合适的处理单元
//! - [`sys_env`] - 系统环境信息，获取操作系统、文件系统、区域设置等信息
//! - [`timestamp_converter`] - 时间戳转换器，处理时间戳格式的转换
//! - [`path_compressor`] - 路径压缩器，压缩和优化文件路径
//! - [`path_optimizer`] - 路径字典优化器，进行收益驱动的层级路径字典收敛

pub mod compression;
pub mod compression_context;
pub mod compression_pipeline;
pub mod content_analyzer;
pub mod dedup_engine;
pub mod dictionary_engine;
pub mod dictionary_manager;
pub mod doctor_encoding;
pub mod doctor_workspace;
pub mod dynamic_plugin_loader;
#[cfg(feature = "experimental")]
pub mod embedding_engine;
pub mod encoding_fallback;
pub mod error_isolation;
pub mod filter_discover;
pub mod filter_variants;
pub mod init_command;
pub mod json_extractor;
pub mod log_reorderer;
pub mod metrics;
pub mod observability;
pub mod path_analyzer;
pub mod path_compressor;
pub mod path_optimizer;
pub mod plugin_config_loader;
pub mod plugin_dispatcher;
pub mod rehydration_pipeline;
pub mod rewrite;
pub mod rule_diagnosis;
pub mod safety_check;
pub mod stream_reader;
pub mod sys_env;
pub mod template_render;
pub mod text_slicer;
pub mod timestamp_converter;
pub mod tracing_init;
pub mod tracking;
pub mod tree_restructure;
pub mod utils;
