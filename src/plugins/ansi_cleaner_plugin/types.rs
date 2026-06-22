//! ansi cleaner plugin 类型定义

use crate::core::plugin_config_loader::CompiledPluginConfig;
/// # 类型概述

/// 本模块定义了 ansi cleaner plugin 模块所需的核心数据类型。
/// 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。
use regex::Regex;
use std::sync::Arc;

/// ANSI 颜色清理插件主结构 (Utility 插件，无缝传递给下一个或单独运行)
pub struct AnsiCleanerPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) ansi_pattern: Arc<Regex>,
    /// 配置文件
    pub config: Option<CompiledPluginConfig>,
}
