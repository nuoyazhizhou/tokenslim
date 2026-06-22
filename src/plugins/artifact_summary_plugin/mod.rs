//! 构建产物摘要脱水：保留测试失败/错误和 SARIF 关键信号，折叠冗余的测试用例细节和 SARIF 结果条目。

//! ## 保留信号
//! - 测试套件名称（JUnit 套件标识）
//! - 测试用例名称（含类名）
//! - 测试状态（失败/错误/跳过）
//! - SARIF 规则ID（安全规则标识）
//! - 严重级别（SARIF 严重性等级）
//! - 源位置（SARIF 问题位置）
//! - 工具名称（扫描工具）

//! ## 压缩目标
//! - 原始 XML/JSON 文本（替换为紧凑摘要）
//! - 长字符串（使用字典引擎压缩重复令牌）
//! - 通过状态的测试用例（可能丢弃，仅保留失败/错误/跳过）
//! - 冗余的 SARIF 结果条目（聚合为摘要）
mod methods;
mod types;

pub use types::ArtifactSummaryPlugin;

#[cfg(test)]
mod showcase;

#[cfg(test)]
mod test;
