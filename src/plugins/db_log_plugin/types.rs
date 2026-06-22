use regex::Regex;
use std::sync::Arc;

/// 数据库日志插件
pub struct DbLogPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) pg_pattern: Arc<Regex>,
    pub(crate) pg_duration_pattern: Arc<Regex>,
    pub(crate) mysql_pattern: Arc<Regex>,
    pub(crate) mongo_json_pattern: Arc<Regex>,
    pub(crate) redis_pattern: Arc<Regex>,
}
