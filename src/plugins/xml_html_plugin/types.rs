/// xml html plugin 类型定义

/// # 类型概述

/// 本模块定义了 xml html plugin 模块所需的核心数据类型。
/// 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。
use crate::core::plugin_config_loader::CompiledPluginConfig;

/// XML/HTML 压缩插件主结构
pub struct XmlHtmlPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    /// 配置文件
    pub config: Option<CompiledPluginConfig>,
}
