//! CVS脱水：保留命令锚点和文件状态，折叠噪音和分隔线，压缩键值对。

//! ## 保留信号
//! - cvs 开头的命令锚点行（如 cvs update）
//! - 状态码映射行（U:, M:, A: 等）
//! - Index: 行（diff文件索引）
//! - 错误/冲突行（如 conflict, error:）
//! - No edits: 行（unedit无编辑文件）

//! ## 压缩目标
//! - 空行
//! - 等号分隔线（长度>=8全等号）
//! - 重复的cvs命令锚点（只保留第一条）
//! - CVS噪音行（如 cvs server: 信息）
//! - 日志模板行（is_cvs_log_boilerplate）
//! - 键值对压缩（如 Working revision: → WR:）
//! - 状态长词压缩（如 Up-to-date → OK）
pub mod methods;
pub mod parser;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod tests;
