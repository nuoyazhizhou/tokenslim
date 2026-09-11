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
    /// 字符串值最大长度，超过此长度的值将被存入字典
    #[serde(default = "default_max_string_val_len")]
    pub max_string_val_len: usize,
    /// 是否开启 Key 字典化
    #[serde(default = "default_true")]
    pub dictionaryize_keys: bool,
}

/// 返回字符串值最大长度的默认值（100）。
fn default_max_string_val_len() -> usize {
    100
}
/// 返回布尔默认值 true（开启 Key 字典化）。
fn default_true() -> bool {
    true
}

impl Default for JsonConfig {
    /// 构造 JsonConfig 默认配置：字符串值最大 100 字符、开启 Key 字典化。
    fn default() -> Self {
        Self {
            max_string_val_len: default_max_string_val_len(),
            dictionaryize_keys: true,
        }
    }
}

/// JSON 压缩插件主结构
pub struct JsonPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) json_detect_pattern: Arc<Regex>,
    pub config: JsonConfig,
}
