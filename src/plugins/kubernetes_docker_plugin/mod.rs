//! Kubernetes/Docker日志脱水：保留关键事件和结构，折叠容器ID和Pod元数据，压缩为令牌字典。

//! ## 保留信号
//! - K8S_POD 正则匹配的行（命名空间/Pod）
//! - DOCKER_ID 正则匹配的容器 ID
//! - Docker CI 输出（如 'Step X/Y'）
//! - Kubernetes CI 输出（如 'kubectl' 命令）
//! - JSON 对象含 'message' 或 'logGroup'

//! ## 压缩目标
//! - 容器 ID 替换为短令牌（$D）
//! - Pod 名称/命名空间替换为令牌（$P/$PK）
//! - JSON 结构解包（展开嵌套）
pub mod methods;
#[cfg(test)]
mod test;
pub mod types;

pub use types::*;

#[cfg(test)]
mod showcase;
