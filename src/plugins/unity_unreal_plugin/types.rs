use serde::{Deserialize, Serialize};

/// Unity/Unreal 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnityUnrealConfig {
    pub fold_asset_loading: bool,
    pub strip_memory_stats: bool,
}

impl Default for UnityUnrealConfig {
    fn default() -> Self {
        Self {
            fold_asset_loading: true,
            strip_memory_stats: true,
        }
    }
}

/// 游戏引擎（Unity/Unreal）日志分析插件
pub struct UnityUnrealPlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) config: UnityUnrealConfig,
}
