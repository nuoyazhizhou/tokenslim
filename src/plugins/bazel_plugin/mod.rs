//! Bazel输出脱水：保留错误行和关键构建摘要，折叠普通INFO日志，压缩目标列表。

//! ## 保留信号
//! - error: 行（编译错误）
//! - BUILD 完成行（如 Build completed）
//! - INFO: Analyzed 行（分析摘要）
//! - bazel version 摘要行（版本信息）
//! - bazel query 目标行（查询结果）

//! ## 压缩目标
//! - 普通 INFO 日志行（折叠丢弃）
//! - 冗长的目标列表（压缩为 TARGETS[count]）
//! - 重复行（去重）
mod methods;
mod types;

pub use types::BazelPlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
