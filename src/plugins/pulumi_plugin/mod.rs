//! Pulumi部署输出脱水：保留错误信号和资源操作摘要，折叠详细资源行为操作类型+字典化URN，压缩非关键行。

//! ## 保留信号
//! - 错误行（保留错误信号，如包含error:的行）
//! - Resources: 行（保留资源计数摘要行）
//! - 操作计数汇总（如A: 3 D: 1 M: 2，在行尾添加）

//! ## 压缩目标
//! - 详细资源行（匹配+/-~模式的资源行，折叠为操作类型+字典调用的短标记）
//! - 资源URN和类型（通过dict_engine.add_path_layered进行令牌字典替换）
//! - 非资源非错误非摘要行（可能被丢弃或压缩）
mod methods;
mod types;

pub use types::PulumiPlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
