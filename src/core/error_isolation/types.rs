//! error isolation 类型定义
//!
//! # 类型概述
//!
//! 本模块定义了 error isolation 模块所需的核心数据类型。
//! 这些类型包括结构体、枚举、 trait 等，用于表示该模块的数据结构和配置信息。

use std::time::Duration;

/// 安全执行器配置
#[derive(Clone)]
pub struct SafeExecutorConfig {
    pub default_timeout: Duration,
    pub catch_panic: bool,
}

impl Default for SafeExecutorConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_millis(1000),
            catch_panic: true,
        }
    }
}

/// 安全执行器主结构
pub struct SafeExecutor {
    pub(crate) config: SafeExecutorConfig,
}

/// 执行错误类型
#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    #[error("E_EXECUTION_PLUGIN_PANIC")]
    Panic,
    #[error("E_EXECUTION_TIMEOUT:{0:?}")]
    Timeout(Duration),
    #[error("E_EXECUTION_INVALID_RESULT")]
    InvalidResult,
    #[error("E_EXECUTION_OTHER:{0}")]
    Other(String),
}
