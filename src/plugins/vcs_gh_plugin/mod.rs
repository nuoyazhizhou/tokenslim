//! GitHub CLI输出脱水：保留命令锚点和结构化数据行，折叠块内多行，压缩URL和键名。

//! ## 保留信号
//! - gh 命令本身（如 "gh pr list"）
//! - PR/Issue 表格行（如带序号和标题的行）
//! - KV 对中的短符号键值（如 ST:success）
//! - 去符号后的状态行（含缩写URL）
//! - 通用非空非分隔符非表头行（经空格压缩）

//! ## 压缩目标
//! - 空行（跳过）
//! - 分隔线（如 "---" 行）
//! - 表头行（列标题行）
//! - 状态符号 ✓ ✗ ○（替换为空格后去除）
//! - 连续多个空格（压缩为单个空格）
//! - URL 缩写（如 https://github.com/... 变为 URL:...）
//! - 长键名替换为短符号（如 workflow -> WF, status -> ST）
//! - run list 中块内多行合并为一行（空格分隔）
pub mod methods;
pub mod parser;
#[cfg(test)]
mod showcase;
#[cfg(test)]
mod tests;
