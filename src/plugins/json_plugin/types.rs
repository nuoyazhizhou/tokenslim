/// json plugin 类型定义

/// # 类型概述

/// 本模块定义了 json plugin 模块所需的核心数据类型。
/// 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。
use regex::Regex;
use std::sync::Arc;
// use crate::core::plugin_config_loader::CompiledPluginConfig;

use serde::{Deserialize, Serialize};

/// JSON 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonConfig {
    /// 数组最大长度，超过此长度的部分将被截断
    #[serde(default = "default_max_array_len")]
    pub max_array_len: usize,
    /// 字符串值最大长度，超过此长度的值将被存入字典
    #[serde(default = "default_max_string_val_len")]
    pub max_string_val_len: usize,
    /// 最大递归深度，防止极度嵌套导致栈溢出
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    /// 是否开启 Key 字典化
    #[serde(default = "default_true")]
    pub dictionaryize_keys: bool,
}

/// 内部辅助函数：执行与 default max array len 相关的具体逻辑。
fn default_max_array_len() -> usize {
    50
}
/// 内部辅助函数：执行与 default max string val len 相关的具体逻辑。
fn default_max_string_val_len() -> usize {
    100
}
/// 内部辅助函数：执行与 default max depth 相关的具体逻辑。
fn default_max_depth() -> usize {
    20
}
/// 内部辅助函数：执行与 default true 相关的具体逻辑。
fn default_true() -> bool {
    true
}

impl Default for JsonConfig {
    /// 提供该插件类型的默认配置实现。
    fn default() -> Self {
        Self {
            max_array_len: default_max_array_len(),
            max_string_val_len: default_max_string_val_len(),
            max_depth: default_max_depth(),
            dictionaryize_keys: true,
        }
    }
}

/// JSON 压缩插件主结构
pub struct JsonPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    #[allow(dead_code)]
    pub(crate) key_pattern: Arc<Regex>,
    pub(crate) json_detect_pattern: Arc<Regex>,
    pub config: JsonConfig,
}
