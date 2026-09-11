//! error isolation 模块
//!
//! # 模块概述
//!
//! 为插件执行提供错误隔离能力：捕获插件调用过程中的 panic（含消息记录）、
//! 提供可选的作用域线程超时包装，以及对执行结果的验证钩子。
//!
//! ## 主要组件
//!
//! - [`SafeExecutor`]：错误隔离执行器。`catch_panic` 捕获闭包 panic 并记录
//!   payload 消息；`execute` 额外支持超时语义（注意：作用域线程无法强制终止，
//!   `Timeout` 仅表示时限内未收到结果，不保证立即返回）；`validate` 对结果做
//!   后置校验。
//! - [`ExecutionError`]：隔离层错误类型（Panic / Timeout / InvalidResult / Other）。
//!
//! ## 接线现状（P1-10）
//!
//! `PluginDispatcher::execute_plugin_chain_fast` 的插件压缩调用统一经由
//! `SafeExecutor::catch_panic` 包装：插件 panic 被隔离为「该插件本次产出为空」，
//! 计入失败黑名单与 panic 指标，不再击穿压缩进程。

mod methods;
#[cfg(test)]
mod test;
mod types;
pub use types::{ExecutionError, SafeExecutor, SafeExecutorConfig};
