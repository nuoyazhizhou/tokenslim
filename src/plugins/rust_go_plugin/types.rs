use regex::Regex;
use std::sync::Arc;

/// Rust/Go 编译日志插件
pub struct RustGoPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) rust_compile_pattern: Arc<Regex>,
    pub(crate) go_panic_pattern: Arc<Regex>,
    pub(crate) go_frame_pattern: Arc<Regex>,
}
