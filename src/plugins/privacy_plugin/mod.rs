//! 隐私处理插件：在其他插件之前，将高置信度敏感值替换为不可逆占位符。
//!
//! 本插件只生成安全占位符，不保存原文映射，也不负责回填真实 secret。

pub mod types;

pub use types::PrivacyPlugin;

pub(crate) use types::redact_with_builtin_patterns;

#[cfg(test)]
mod test;
