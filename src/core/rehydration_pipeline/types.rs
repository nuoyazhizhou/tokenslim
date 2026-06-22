//! rehydration pipeline 类型定义
//!
//! # 类型概述
//!
//! 本模块定义了 rehydration pipeline 模块所需的核心数据类型。
//! 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。

use crate::core::dictionary_engine::Dictionary;
use crate::core::plugin_dispatcher::Plugin;
use std::collections::HashMap;

/// 还原流水线配置
#[derive(Clone, Debug)]
pub struct RehydrationConfig {
    pub preserve_order: bool,    // 是否保留重排序信息（未来）
    pub fallback_on_error: bool, // 遇到无法还原的 token 时是否 fallback（例如跳过）
}

impl Default for RehydrationConfig {
    fn default() -> Self {
        Self {
            preserve_order: false,
            fallback_on_error: true,
        }
    }
}

/// 还原流水线主结构
pub struct RehydrationPipeline {
    pub(crate) dict: Dictionary,
    pub(crate) plugins: HashMap<String, Box<dyn Plugin>>,
    #[allow(dead_code)]
    pub(crate) config: RehydrationConfig,
}

/// 还原错误类型
#[derive(Debug, thiserror::Error)]
pub enum RehydrationError {
    #[error("E_REHYDRATION_UNKNOWN_TOKEN:{0}")]
    UnknownToken(String),
    #[error("E_REHYDRATION_DICT_RESOLUTION_FAILED:{0}")]
    DictResolutionFailed(String),
    #[error("E_REHYDRATION_PLUGIN_NOT_FOUND:{0}")]
    PluginNotFound(String),
    #[error("E_REHYDRATION_PLUGIN_DECOMPRESS_FAILED:{0}")]
    PluginDecompressFailed(String),
    #[error("E_REHYDRATION_AST_RECONSTRUCTION_FAILED")]
    AstReconstructionFailed,
}
