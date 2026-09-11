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
    /// Panic payload 的消息（`String`/`&str` downcast）会被记录到 error 日志，
    /// 避免隔离点只留下一个无消息的 `Panic` 错误（P3-34①）。
    pub fn catch_panic<F, R>(&self, f: F) -> Result<R, ExecutionError>
    where
        F: FnOnce() -> R + panic::UnwindSafe,
    {
        if !self.config.catch_panic {
            Ok(f())
        } else {
            match panic::catch_unwind(f) {
                Ok(result) => Ok(result),
                Err(payload) => {
                    let msg = payload
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "<non-string panic payload>".to_string());
                    log::error!("E_EXECUTION_PLUGIN_PANIC: {msg}");
                    Err(ExecutionError::Panic)
                }
            }
        }
    }

    /// 在受控线程中执行闭包，并将 Panic 或接收超时映射为 [ExecutionError]。
    ///
    /// 当 `timeout` 为 `Some` 时，执行会在作用域线程中运行并始终捕获闭包 Panic；
    /// 当其为 `None` 时，则委托给 [Self::catch_panic]，是否捕获 Panic 取决于配置。
    /// 注意：作用域线程在离开作用域前会被等待，因此 `Timeout` 仅表示在给定时限内未收到结果，
    /// 而不表示闭包被强制终止或本方法保证立即返回。
    ///
    /// # 参数
    /// - `f`: 待执行的闭包。
    /// - `timeout`: 可选的结果接收时限；`None` 时不创建工作线程。
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
