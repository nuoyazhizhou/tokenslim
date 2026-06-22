//! TokenSlim 初始化命令
//!
//! 生成项目级 `.tokenslim.toml` 配置文件，自动探测项目类型并生成对应配置。
//! 可选安装 shell hooks。

pub mod methods;
pub mod types;

pub use methods::*;
pub use types::*;
