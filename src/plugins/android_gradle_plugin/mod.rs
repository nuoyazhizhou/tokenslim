//! Android/Gradle日志脱水：保留错误Task和资源警告，折叠重复Task状态行，压缩构建路径和环境变量。

//! ## 保留信号
//! - > Task ... FAILED 行（构建失败的任务）
//! - warn: removing resource 行（资源删除警告）
//! - WORKSPACE 等环境变量行（Jenkins关键变量）

//! ## 压缩目标
//! - 非FAILED的Task行（UP-TO-DATE等）折叠为计数
//! - 连续5个以上相同包名的资源警告合并
//! - 构建路径替换为$GRADLE令牌字典
pub mod methods;
/// # 功能
/// - 识别 Gradle 构建任务路径（`:app:compileDebug...`）
/// - 提取并压缩构建路径（`app/build/intermediates/...`）
/// - 识别 D8/R8 编译消息
/// - 识别 APK 签名信息
/// - 优化 Jenkins 环境变量
pub mod types;

pub use types::*;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
