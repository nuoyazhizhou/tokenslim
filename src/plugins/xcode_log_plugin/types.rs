use regex::Regex;
use std::sync::Arc;

/// Xcode 构建日志插件
pub struct XcodeLogPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) compile_c_pattern: Arc<Regex>,
    pub(crate) clang_pattern: Arc<Regex>,
}
