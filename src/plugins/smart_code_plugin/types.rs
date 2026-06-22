/// smart code plugin 类型定义

/// # 类型概述

/// 本模块定义了 smart code plugin 模块所需的核心数据类型。
/// 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。
use regex::Regex;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartCodeConfig {
    pub fold_large_bodies: bool,
    pub body_threshold_lines: usize,
    pub strip_comments: bool,
    pub compress_indentation: bool,
    pub extract_identifiers: bool,
}

impl Default for SmartCodeConfig {
    /// 提供该插件类型的默认配置实现。
    fn default() -> Self {
        SmartCodeConfig {
            fold_large_bodies: true,
            body_threshold_lines: 15,
            strip_comments: true,
            compress_indentation: true,
            extract_identifiers: true,
        }
    }
}

/// 智能代码插件 (双向无损，适用于通用源码文件)
pub struct SmartCodePlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) config: SmartCodeConfig,
    pub(crate) identifier_pattern: Arc<Regex>,
    pub(crate) spaces_pattern: Arc<Regex>,
}
