//! Git脱水：保留命令核心输出，折叠图形进度，压缩网络噪音和分页信息。

//! ## 保留信号
//! - 文件路径（diff/status/add等）
//! - 状态标记（M, A, D等）
//! - 提交哈希与提交信息（log）
//! - reflog条目（最多20条）
//! - 冲突标记（merge冲突）
//! - 分支切换信息（checkout reflog）

//! ## 压缩目标
//! - 进度行（fetch/push/pull网络噪音）
//! - 图形字符（*|/\图形输出）
//! - 超出的reflog条目（>20折叠）
//! - 装饰（origin/->o/, tag: ->t:）
//! - summary词长（files changed->files, insertions(+) ->ins等）
//! - 分页提示行（--More--等）
pub mod methods;
pub mod parser;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod tests;

pub use methods::*;
pub use parser::*;
