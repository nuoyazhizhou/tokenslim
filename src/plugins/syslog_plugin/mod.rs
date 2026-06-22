//! 系统日志脱水：保留时间戳和消息内容，折叠主机名和进程名为字典令牌，压缩重复标识。

//! ## 保留信号
//! - 时间戳（月 日 时:分:秒）
//! - 消息内容（msg）
//! - PID（进程ID，可选）
//! - 非syslog格式的行（原样保留）

//! ## 压缩目标
//! - 主机名（替换为$SYS格式中的字典令牌）
//! - 进程名（替换为$SYS格式中的字典令牌）
mod methods;
mod types;
pub use types::SyslogPlugin;

#[cfg(test)]
mod showcase;
#[cfg(test)]
mod test;
