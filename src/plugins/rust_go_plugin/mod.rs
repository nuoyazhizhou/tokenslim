//! Rust/Go日志脱水：保留编译与测试关键信号，折叠重复编译行，压缩测试统计与路径。

//! ## 保留信号
//! - Compiling .. (编译开始，折叠后保留统计行)
//! - Finished .. (编译完成行)
//! - test .. FAILED (失败测试详情)
//! - Warning/error/panic (错误警告行)
//! - goroutine .. [..]: (Go协程信息)
//! - running N tests (测试开始行，被替换为统计)

//! ## 压缩目标
//! - 重复的Compiling行折叠为一行计数并隐藏细节
//! - 多个测试结果汇总为统计行，仅保留失败详情
//! - 长路径替换为字典令牌如$Pn
//! - Go测试输出类似压缩
mod methods;
mod types;
pub use types::RustGoPlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
