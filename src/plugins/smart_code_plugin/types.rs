//! 智能代码插件类型定义模块：定义插件主体 `SmartCodePlugin` 等核心数据类型。

/// smart code plugin 类型定义

/// # 类型概述

/// 本模块定义了 smart code plugin 模块所需的核心数据类型。
/// 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。
use regex::Regex;
use std::sync::Arc;

/// 智能代码插件 (双向无损，适用于通用源码文件)
pub struct SmartCodePlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) identifier_pattern: Arc<Regex>,
    pub(crate) spaces_pattern: Arc<Regex>,
}
