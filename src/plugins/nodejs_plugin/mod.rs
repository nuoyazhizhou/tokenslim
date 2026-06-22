//! Node.js 运行时日志脱水：保留错误和 npm 错误，折叠下载进度。

//! ## 保留信号
//! - Error / TypeError 行
//! - npm ERR! 行
//! - WARN 行

//! ## 压缩目标
//! - npm 下载进度折叠
//! - node_modules 路径压缩
pub mod methods;
/// # 功能

/// - 识别 node_modules 路径
/// - 提取 npm/yarn 错误代码
/// - 识别 TypeScript 错误
/// - 识别 Webpack 打包信息
/// - 优化 Jenkins Pipeline 语法
pub mod types;

pub use types::*;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
