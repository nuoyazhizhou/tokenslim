//! Python异常脱水：保留异常类型和关键堆栈信息，折叠重复异常和深层堆栈，压缩路径和异常消息。

//! ## 保留信号
//! - Traceback (most recent call last): 头部
//! - 内置异常类名（如 Exception, ValueError 等）
//! - 错误消息行（如 ValueError: invalid literal）
//! - 文件路径和行号（如 File "...", line ..., in ...）
//! - 异常链中的连接信息（如直接原因、上下文等）

//! ## 压缩目标
//! - 重复的异常堆栈（超过阈值替换为 [DUPLICATE] 标记）
//! - 深层堆栈帧（超过阈值截断为 [...]）
//! - 文件路径（替换为 $PY|FL| 令牌并使用字典）
//! - 异常类型和消息（替换为 $PY|EX| 令牌并使用字典）
//! - 链式异常计数摘要（多个异常合并为 [CHAINED] 计数）
pub mod methods;
//
pub mod types;

pub use types::*;

#[cfg(test)]
mod test;

#[cfg(test)]
mod showcase;
