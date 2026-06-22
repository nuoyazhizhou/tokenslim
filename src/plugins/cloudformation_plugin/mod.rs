//! CloudFormation事件行脱水：保留失败/回滚事件，折叠重复状态，压缩冗长输出。

//! ## 保留信号
//! - 事件行（状态+资源ID）
//! - 失败/回滚行（FAILED或ROLLBACK状态）
//! - 错误信号行（keep_error_signal保留）

//! ## 压缩目标
//! - 空行或全分隔符行
//! - 锚行（首行非空内容）
//! - 重复状态事件行（按状态计数折叠）
mod methods;
mod types;

pub use types::CloudFormationPlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
