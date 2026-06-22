/// noise filter plugin 类型定义
use serde::{Deserialize, Serialize};

/// 噪点过滤插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseFilterConfig {
    /// 是否清理进度条（带有 \r 的行）
    pub clean_progress_bars: bool,
    /// 是否折叠连续的相似行（如 Javadoc 生成、文件复制等）
    pub fold_repetitive_noise: bool,
    /// 是否替换长哈希/Hex 字符串
    pub mask_long_hex: bool,
    /// 哈希/Hex 屏蔽阈值（字符数）
    pub hex_threshold: usize,
}

impl Default for NoiseFilterConfig {
    /// 提供该插件类型的默认配置实现。
    fn default() -> Self {
        NoiseFilterConfig {
            clean_progress_bars: true,
            fold_repetitive_noise: true,
            mask_long_hex: true,
            hex_threshold: 32,
        }
    }
}

/// 噪点过滤插件结构
pub struct NoiseFilterPlugin {
    pub name: &'static str,
    pub priority: u8,
    pub config: NoiseFilterConfig,
}
