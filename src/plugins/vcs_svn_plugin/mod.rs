//! SVN 命令输出脱水：保留命令锚点和变更状态，压缩路径。

//! ## 保留信号
//! - 原始命令锚点
//! - 文件变更状态（A/D/M/U/C）
//! - commit 信息

//! ## 压缩目标
//! - 长路径替换为 $SVN 令牌字典
//! - 重复行去重
pub mod methods;
pub mod parser;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod showcase;
