//! Ansible脱水：保留任务头和主机状态，折叠重复任务细节，压缩语法错误。

//! ## 保留信号
//! - TASK [名称] 行（任务标题）
//! - RUNNING HANDLER [名称] 行（处理程序标题）
//! - ok/changed/failed/unreachable/skipping: [主机] 行（主机状态）
//! - PLAY RECAP 行（汇总）
//! - ERROR! 语法错误行（压缩后的错误信息）

//! ## 压缩目标
//! - 重复的任务输出折叠为单行摘要
//! - 主机列表合并为范围格式如 host[1,2]
//! - 详情中的 msg 字段提取并压缩
//! - 语法错误多行压缩为单行
//! - 空行和无关注释行删除
mod methods;
mod types;

pub use types::AnsiblePlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
