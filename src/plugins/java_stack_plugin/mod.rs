//! Java异常堆栈脱水：保留关键异常类和帧，折叠重复和深层堆栈，压缩类名和路径。

//! ## 保留信号
//! - 异常行（Exception in thread, Exception, Error等）
//! - 常见异常类名（java.lang白名单）
//! - Caused by 行（异常链）
//! - 堆栈帧前N行（通过阈值）

//! ## 压缩目标
//! - 重复相同异常堆栈（超过阈值折叠为[DUPLICATE]）
//! - 深层堆栈超出N的行（截断并添加摘要）
//! - 抑制异常（suppressed exceptions）压缩为简短形式
//! - 异常类名（非白名单）用字典编码为$JEX令牌
//! - 堆栈帧类名和方法用$JST令牌编码
//! - Caused by 类名用$JCB令牌编码
mod methods;
//
mod types;
pub use types::JavaStackPlugin;

#[cfg(test)]
mod test;

#[cfg(test)]
mod showcase;
