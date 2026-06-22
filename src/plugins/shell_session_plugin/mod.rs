//! Shell会话脱水：保留命令和输出，折叠提示符和进度信息。

//! ## 保留信号
//! - 命令与输出行（未明确丢弃的文本块）

//! ## 压缩目标
//! - shell提示符（如$ # > PS ~$）
//! - ANSI转义序列
//! - 多空格（折叠为单个空格）
//! - 环境变量赋值行（env var=value）
//! - robocopy/curl/tar进度行
pub mod methods;
pub mod parser;

#[cfg(test)]
pub mod showcase;
