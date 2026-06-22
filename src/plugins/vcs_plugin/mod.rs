//! VCS 脱水：保留版本控制命令的语义结构，折叠冗余空白和路径信息，压缩差异输出以适应 AI 上下文。

//! ## 保留信号
//! - diff -- 开头行（差异头部）
//! - @@ 开头行（差异块范围）
//! - +++ / --- 开头行（文件路径）
//! - 状态指示符（如 M、A、D）
//! - [paths] 路径字典（路径重映射表）
//! - 日期时间令牌（如 YYYYMMDD HH:MM）

//! ## 压缩目标
//! - 前导连续空白（非 Python 文件或 hash 范围）
//! - 内联对齐多空白（非表格或代码注释保护）
//! - 长绝对路径（替换为 [paths] 字典条目）
//! - 目录树结构（如 git checkout 输出缩进）
//! - SVN 更新输出中的冗余行
//! - 差异输出中的重复文件路径信息
pub mod ir;
pub mod methods;
pub mod parser;
pub mod rule_engine;
pub mod types;

pub use types::*;

#[cfg(test)]
mod test;
