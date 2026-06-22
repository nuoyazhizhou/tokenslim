use crate::core::plugin_config_loader::CompiledPluginConfig;
/// python traceback plugin 类型定义

/// # 类型概述

/// 本模块定义了 python traceback plugin 模块所需的核心数据类型。
/// 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。
use regex::Regex;
use std::sync::Arc;

/// Python Traceback 插件主结构
pub struct PythonTracebackPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) trace_header_pattern: Arc<Regex>,
    pub(crate) file_line_pattern: Arc<Regex>,
    pub(crate) exception_pattern: Arc<Regex>,
    /// 配置文件
    pub config: Option<CompiledPluginConfig>,
}
