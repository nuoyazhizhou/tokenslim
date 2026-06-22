use serde::{Deserialize, Serialize};

/// PHP/Ruby 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpRubyConfig {
    pub strip_html_wrappers: bool,
    pub fold_internal_frames: bool,
}

impl Default for PhpRubyConfig {
    fn default() -> Self {
        Self {
            strip_html_wrappers: true,
            fold_internal_frames: true,
        }
    }
}

/// PHP/Ruby 日志/报错分析插件
pub struct PhpRubyPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) config: PhpRubyConfig,
}
