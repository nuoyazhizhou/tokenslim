use crate::core::plugin_config_loader::CompiledPluginConfig;
/// node error plugin 类型定义

/// # 类型概述

/// 本模块定义了 node error plugin 模块所需的核心数据类型。
use regex::Regex;
use std::sync::Arc;

/// Node.js Error / JS Stack Trace 插件主结构
pub struct NodeErrorPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) exception_pattern: Arc<Regex>,
    pub(crate) frame_pattern: Arc<Regex>,
    pub config: Option<CompiledPluginConfig>,
}
