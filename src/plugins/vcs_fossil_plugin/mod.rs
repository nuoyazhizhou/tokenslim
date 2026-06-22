//! Fossil脱水：保留命令锚点和状态变更摘要，折叠叙事废话，压缩元数据噪音。

//! ## 保留信号
//! - fossil status/changes 等命令锚点行
//! - 映射后的状态码行（M/A/D/R/!）
//! - Pull/Push 行（sync命令）
//! - 警报行（map_fossil_alert）
//! - 非噪音非叙事的有效行（generic fallback）

//! ## 压缩目标
//! - 空行
//! - 元数据噪音行（如Repository/Check-ins等）
//! - 叙事废话行（如Stash changes/Autosync等）
//! - 非锚点的fossil命令前缀行（重复命令）
//! - 状态变化前的冗长描述行（语法覆盖）
pub mod methods;
pub mod parser;
#[cfg(test)]
mod showcase;
#[cfg(test)]
mod tests;
