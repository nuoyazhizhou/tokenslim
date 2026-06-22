/// yaml plugin 类型定义

/// # 类型概述

/// 本模块定义了 yaml plugin 模块所需的核心数据类型。
/// 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。
use serde::{Deserialize, Serialize};

/// YAML 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlConfig {
    /// 序列最大长度，超过此长度的部分将被截断
    #[serde(default = "default_max_seq_len")]
    pub max_seq_len: usize,
    /// 字符串值最大长度，超过此长度的值将被存入字典
    #[serde(default = "default_max_string_val_len")]
    pub max_string_val_len: usize,
    /// 最大递归深度
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    /// 是否开启 Key 字典化
    #[serde(default = "default_true")]
    pub dictionaryize_keys: bool,
}

/// 内部辅助函数：执行与 default max seq len 相关的具体逻辑。
fn default_max_seq_len() -> usize {
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

impl Default for YamlConfig {
    /// 提供该插件类型的默认配置实现。
    fn default() -> Self {
        Self {
            max_seq_len: default_max_seq_len(),
            max_string_val_len: default_max_string_val_len(),
            max_depth: default_max_depth(),
            dictionaryize_keys: true,
        }
    }
}

/// YAML 压缩插件主结构
pub struct YamlPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub config: YamlConfig,
}
