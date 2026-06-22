//! GCC/Clang编译日志脱水：保留错误、警告和关键信息，折叠重复警告，压缩长路径和宏定义。

//! ## 保留信号
//! - error: 行（编译错误）
//! - warning: 行（相同警告类型的前 N 次）
//! - note: 行（编译提示）
//! - undefined reference 行（链接器错误）
//! - Build files have been written to: 等关键行
//! - CMake Error 等构建错误行
//! - Error 1 或 Error 2 结尾的行

//! ## 压缩目标
//! - 长路径替换为 $GCC 令牌字典
//! - 宏定义（-D...）替换为令牌
//! - 相同 warning 超过阈值的后续行（折叠）
//! - 重复行去重（threshold=1）
mod methods;
//
mod types;
pub use types::GccLogPlugin;
#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
