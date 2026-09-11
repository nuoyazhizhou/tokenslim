//! gcc log plugin 类型定义

/// # 类型概述

/// 本模块定义了 gcc log plugin 模块所需的核心数据类型。
/// 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构 and 配置信息。
use regex::Regex;
use std::sync::Arc;

/// gcc 日志插件主结构
pub struct GccLogPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) gcc_pattern: Arc<Regex>,
    pub(crate) make_pattern: Arc<Regex>,
    pub(crate) cmake_pattern: Arc<Regex>,
    pub(crate) error_pattern: Arc<Regex>,
}
