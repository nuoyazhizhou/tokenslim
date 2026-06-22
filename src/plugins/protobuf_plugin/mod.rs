//! Protobuf脱水：保留诊断信息，折叠重复警告，压缩文件路径。

//! ## 保留信号
//! - error 诊断行（保留错误位置和消息）
//! - warning 诊断行（最多前6个）
//! - 错误计数和警告计数（汇总行）
//! - is_error_line 匹配的行（错误信号）

//! ## 压缩目标
//! - .proto 文件路径（替换为 $PB 令牌字典）
//! - 多余空白字符（compact_spaces 压缩）
//! - 重复的警告（超过6个后丢弃）
//! - 非诊断且非错误信号的行（丢弃）
mod methods;
mod types;

pub use types::ProtobufPlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
