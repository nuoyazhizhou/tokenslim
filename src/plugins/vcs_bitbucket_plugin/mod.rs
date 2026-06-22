//! Bitbucket CLI输出脱水：保留PR/Issue关键元数据，折叠表头分隔线和噪声信息，压缩为紧凑格式。

//! ## 保留信号
//! - 命令锚点（bitbucket pr list等）
//! - PR列表数据行（#ID ST:STATE OW:@author title）
//! - PR视图元数据（状态、描述、分支等）
//! - PR创建结果（Created PR #...）
//! - Issue列表数据行（#ID ST:STATUS OW:@assignee title PRI:priority）
//! - 源代码分支映射（Source:feature-auth->main）

//! ## 压缩目标
//! - 空行
//! - 表头行（全大写缩写的标题行）
//! - 分隔线（全由-或=组成的长行）
//! - 噪声信息（Created/Updated/Participants/Comments等）
//! - URL行（URL:或http开头）
//! - 冗余标题行（如'Pull request #...'）
pub mod methods;
pub mod parser;
#[cfg(test)]
mod showcase;
#[cfg(test)]
mod tests;
