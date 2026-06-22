use regex::Regex;
use std::sync::Arc;

/// 系统日志插件
pub struct SyslogPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) syslog_pattern: Arc<Regex>,
}
