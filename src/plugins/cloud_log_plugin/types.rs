use regex::Regex;
use std::sync::Arc;

/// 云厂商日志外壳剥离插件
pub struct CloudLogPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) aws_tail_pattern: Arc<Regex>,
    pub(crate) generic_cloud_line_pattern: Arc<Regex>,
    pub(crate) uvicorn_access_pattern: Arc<Regex>,
    pub(crate) aws_lambda_pattern: Arc<Regex>,
    pub(crate) standard_bracket_pattern: Arc<Regex>,
}
