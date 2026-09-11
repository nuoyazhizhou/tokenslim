use serde::{Deserialize, Serialize};

/// PHP/Ruby 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpRubyConfig {
    pub strip_html_wrappers: bool,
}

impl Default for PhpRubyConfig {
    /// PhpRubyConfig 默认值：剥离 HTML 包装开启。
    fn default() -> Self {
        Self {
            strip_html_wrappers: true,
        }
    }
}

/// PHP/Ruby 日志/报错分析插件
pub struct PhpRubyPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) config: PhpRubyConfig,
}
