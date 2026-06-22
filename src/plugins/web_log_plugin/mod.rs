//! Web访问日志脱水：保留异常/慢请求信号，折叠常规记录为紧凑摘要，压缩IP/UA/路径等冗余字段。

//! ## 保留信号
//! - $W|SUMMARY行（健康摘要）
//! - 异常行（如4xx/5xx错误）
//! - 慢请求行（slow lines）
//! - 原始错误日志行（error_log_pattern匹配的）

//! ## 压缩目标
//! - IP地址替换为字典令牌
//! - URI路径替换为分层字典令牌
//! - User-Agent替换为字典令牌
//! - 时间戳压缩为紧凑格式（YYYY-MM-DD HH:MM:SS）
//! - 流路径（如/stream/xxx）折叠为前8字符
//! - URL编码解码（%20等替换）
//! - 多个连续空格压缩为一个
//! - 重复的详细记录聚合为标准摘要
mod methods;
mod types;
pub use types::WebLogPlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
