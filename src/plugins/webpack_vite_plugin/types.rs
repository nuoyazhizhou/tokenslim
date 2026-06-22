use serde::{Deserialize, Serialize};

/// Webpack/Vite 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebpackViteConfig {
    pub fold_asset_tables: bool,
    pub strip_timestamps: bool,
}

impl Default for WebpackViteConfig {
    fn default() -> Self {
        Self {
            fold_asset_tables: true,
            strip_timestamps: true,
        }
    }
}

/// Webpack/Vite 前端构建工具分析插件
pub struct WebpackVitePlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) config: WebpackViteConfig,
}
