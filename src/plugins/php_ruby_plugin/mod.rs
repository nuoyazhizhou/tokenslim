//! PHP/Ruby 错误栈脱水：保留错误关键行，折叠 HTML 包裹。

//! ## 保留信号
//! - Fatal error: 行（PHP 致命错误）
//! - PHP Stack trace: 行（PHP 堆栈追踪）
//! - Uncaught Error: 行（PHP 未捕获错误）
//! - ActionView::Template::Error（Ruby 模板错误）
//! - .rb: 行（Ruby 文件引用）
//! - rake aborted!（Rake 中止）
//! - HTML 错误页面标题（Whoops! / exception_title）

//! ## 压缩目标
//! - HTML 标签包裹（<html>/<div> 等）
pub mod methods;
pub mod types;
pub use types::*;

#[cfg(test)]
mod showcase;
