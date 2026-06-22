//! dedup engine 类型定义

use crate::core::compression::Token;
use dashmap::{DashMap, DashSet};
use std::collections::HashMap;

/// 去重类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DedupType {
    Line,
    StackFrame,
    Path,
    Pattern,
}

/// 去重结果
#[derive(Debug, Clone)]
pub struct DedupResult<'a> {
    pub tokens: Vec<Token<'a>>,
    pub count: usize,
}

/// 去重配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DedupConfig {
    pub line_threshold: usize,
    pub stack_frame_threshold: usize,
    pub path_threshold: usize,
    pub pattern_threshold: usize,
    pub fuzzy_threshold: f32,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            line_threshold: 3,
            stack_frame_threshold: 3,
            path_threshold: 3,
            pattern_threshold: 3,
            fuzzy_threshold: 0.9,
        }
    }
}

/// 原始 DedupEngine (单线程/局部使用)
pub struct DedupEngine {
    pub config: DedupConfig,
    pub(crate) global_cache: HashMap<u64, String>,
    pub(crate) seen_hashes: std::collections::HashSet<u64>,
    #[allow(dead_code)]
    pub(crate) fuzzy_cache: HashMap<u64, (String, String)>,
}

/// 增强版 SharedDedupEngine (多线程共享)
pub struct SharedDedupEngine {
    pub config: DedupConfig,
    pub(crate) global_cache: DashMap<u64, String>,
    pub(crate) seen_hashes: DashSet<u64>,
    #[allow(dead_code)]
    pub(crate) fuzzy_cache: DashMap<u64, (String, String)>,
}
