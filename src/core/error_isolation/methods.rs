//! error isolation 方法实现
//!
//! # 方法概述
//!
//! 本模块实现了 error isolation 模块的主要业务逻辑。
//! 包含所有公共 API 的实现，以及内部辅助函数。

use super::types::*;
use std::panic;
use std::time::Duration;

impl SafeExecutor {
    /// 创建一个新的安全执行器（SafeExecutor）。
    pub fn new(config: SafeExecutorConfig) -> Self {
        Self { config }
    }

    /// 捕获闭包运行过程中的 Panic。
    /// 只有在配置中开启了 `catch_panic` 时才会生效。
    pub fn catch_panic<F, R>(&self, f: F) -> Result<R, ExecutionError>
    where
        F: FnOnce() -> R + panic::UnwindSafe,
    {
        if !self.config.catch_panic {
            Ok(f())
        } else {
            match panic::catch_unwind(f) {
                Ok(result) => Ok(result),
                Err(_) => Err(ExecutionError::Panic),
            }
        }
    }

    /// 在受控环境下执行闭包，支持超时控制和 Panic 捕获。
    ///
    /// # 参数
    /// - `f`: 待执行的闭包。
    /// - `timeout`: 最大允许运行时间。如果为 None，则仅捕获 Panic。
    pub fn execute<F, R>(&self, f: F, timeout: Option<Duration>) -> Result<R, ExecutionError>
    where
        F: FnOnce() -> R + Send + panic::UnwindSafe,
        R: Send,
    {
        if let Some(t) = timeout {
            let (tx, rx) = std::sync::mpsc::channel();

            // 使用 thread::scope 允许非 'static 借用。
            // 注意：由于 Rust 标准库线程无法强制终止，如果发生超时，子线程仍会运行直到结束。
            let res = std::thread::scope(|s| {
                s.spawn(|| {
                    let r = panic::catch_unwind(f);
                    let _ = tx.send(r);
                });

                rx.recv_timeout(t)
            });

            match res {
                Ok(Ok(val)) => Ok(val),
                Ok(Err(_)) => Err(ExecutionError::Panic),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(ExecutionError::Timeout(t)),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    Err(ExecutionError::Other("Channel disconnected".to_string()))
                }
            }
        } else {
            self.catch_panic(f)
        }
    }

    /// 对执行结果进行后续验证。
    pub fn validate<R, V>(&self, result: R, validator: V) -> Result<R, ExecutionError>
    where
        V: FnOnce(&R) -> bool,
    {
        if validator(&result) {
            Ok(result)
        } else {
            Err(ExecutionError::InvalidResult)
        }
    }
}
