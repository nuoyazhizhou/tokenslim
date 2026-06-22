use regex::Regex;
use std::sync::Arc;

/// Web 访问与错误日志插件
pub struct WebLogPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) combined_log_pattern: Arc<Regex>,
    pub(crate) common_log_pattern: Arc<Regex>,
    pub(crate) error_log_pattern: Arc<Regex>,
    pub(crate) uvicorn_access_pattern: Arc<Regex>,
    pub(crate) envoy_access_pattern: Arc<Regex>,
    pub(crate) alb_access_pattern: Arc<Regex>,
    pub(crate) aws_logs_tail_pattern: Arc<Regex>,
    pub(crate) cloudwatch_table_row_pattern: Arc<Regex>,
}
