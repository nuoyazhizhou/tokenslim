//! dedup engine 类型定义

use crate::core::compression::Token;
use dashmap::{DashMap, DashSet};
use std::collections::HashMap;

/// 去重结果
#[derive(Debug, Clone)]
pub struct DedupResult<'a> {
    pub tokens: Vec<Token<'a>>,
    pub count: usize,
}

/// 去重配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DedupConfig {
    /// 模式去重参与门槛（P2-56 收口：仅此阈值被消费，两版引擎统一按
    /// `pattern_threshold * 4` 计算最短参与长度；原 line/stack_frame/path
    /// 三阈值零消费，属「假调参」死字段，已删除）。
    pub pattern_threshold: usize,
    /// `seen_hashes` 分代上限（P2-09）：集合规模达到该值即整体清空重开一代，
    /// 防止长驻进程（server / 长时间 `--stream`）随输入线性增长内存泄漏。
    /// 正确性无损：已产出输出的 token 不受影响，代价仅是清空后旧行的重复
    /// 发现需重新登记（重新走一次 add_macro）。
    #[serde(default = "default_max_seen_hashes")]
    pub max_seen_hashes: usize,
    /// `global_cache` 容量上限（P2-09：原 Shared 版 200000 硬编码提为可配，
    /// 单线程版同口径补上限）。达到上限后不再登记新条目，仅跳过。
    #[serde(default = "default_max_cache_entries")]
    pub max_cache_entries: usize,
}

/// 返回 `max_seen_hashes` 默认值（1_000_000）。
fn default_max_seen_hashes() -> usize {
    1_000_000
}

/// 返回 `max_cache_entries` 默认值（200_000，与旧 Shared 版硬编码一致）。
fn default_max_cache_entries() -> usize {
    200_000
}

impl Default for DedupConfig {
    /// 返回去重默认配置：模式阈值 3，seen 分代上限 100 万，缓存上限 20 万。
    fn default() -> Self {
        Self {
            pattern_threshold: 3,
            max_seen_hashes: default_max_seen_hashes(),
            max_cache_entries: default_max_cache_entries(),
        }
    }
}

/// 原始 DedupEngine (单线程/局部使用)
pub struct DedupEngine {
    pub config: DedupConfig,
    pub(crate) global_cache: HashMap<u64, String>,
    pub(crate) seen_hashes: std::collections::HashSet<u64>,
}

/// 增强版 SharedDedupEngine (多线程共享)
pub struct SharedDedupEngine {
    pub config: DedupConfig,
    pub(crate) global_cache: DashMap<u64, String>,
    pub(crate) seen_hashes: DashSet<u64>,
}
