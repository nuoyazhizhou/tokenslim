//! Helm输出脱水：保留关键字段和资源类型，折叠重复资源，压缩空格和丢弃非核心行。

//! ## 保留信号
//! - NAME: 行（资源名称）
//! - LAST DEPLOYED: 行（部署时间）
//! - NAMESPACE: 行（命名空间）
//! - STATUS: 行（状态）
//! - REVISION: 行（版本号）
//! - TEST SUITE: 行（测试套件）
//! - Last Started: / Last Completed: / Phase: 行（状态字段）
//! - deployment/ / service/ / configmap/ / secret/ 资源行（去重保留）
//! - error: 行（错误信息）

//! ## 压缩目标
//! - 行内多余空格（compact_spaces 压缩）
//! - 重复的资源行（BTreeSet 去重）
//! - 非字段、非资源、非错误的普通输出行（丢弃）
mod methods;
mod types;

pub use types::HelmPlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
