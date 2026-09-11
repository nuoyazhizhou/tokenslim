//! TOML/INI 配置结构化脱水：识别并规范化键值配置，避免落入 CodeBlock/smart_path 兜底。
//!
//! TOML 与 INI 共享 `[段头]` + `key=value` 行结构，收敛为单一插件。`detect` 用行级结构
//! 统计（不经语义解析）同时覆盖二者；`compress` 能 toml 解析则规范化紧凑化，否则原样保留。

pub mod methods;
pub mod types;

pub use types::*;

#[cfg(test)]
mod test;

#[cfg(test)]
mod showcase;
