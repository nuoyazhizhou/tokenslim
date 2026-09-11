//! Dedup engine methods.

use super::types::*;
use crate::core::compression::Token;
use crate::core::dictionary_engine::DictionaryEngine;
use bumpalo::Bump;
use dashmap::{DashMap, DashSet};
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// 去重参与门槛：长度小于该值的文本直接跳过去重。
///
/// P2-10/P2-56 收口：旧实现 Shared（多线程）与单线程两版引擎分别硬编码
/// `text.len() < 12` / `< 40`——同一输入随并行与否去重行为漂移，且 preset
/// 写入的 `pattern_threshold`（fast=5 / ai=2）从未被读取，属「假调参」死配置。
/// 现统一为单一公式 `pattern_threshold * 4`，让两版引擎行为一致，并使 preset
/// 真实生效：默认 `pattern_threshold=3` → 门槛 12（与旧 Shared 版一致），
/// fast=5 → 20、ai=2 → 8。
fn dedup_min_len(config: &DedupConfig) -> usize {
    config.pattern_threshold.saturating_mul(4)
}

impl SharedDedupEngine {
    /// 基于给定配置创建共享去重引擎，初始化全局缓存、已见哈希集合与模糊缓存（均为并发安全结构）。
    pub fn new(config: DedupConfig) -> Self {
        Self {
            config,
            global_cache: DashMap::new(),
            seen_hashes: DashSet::new(),
        }
    }

    /// 全局去重逻辑：增加了对本地缓存的支持和跨线程发现逻辑
    pub fn dedup_cross_slice_with_local<'a>(
        &self,
        text: &str,
        dict: &mut DictionaryEngine,
        _arena: &'a Bump,
        local_cache: &mut HashMap<u64, String>,
    ) -> Option<DedupResult<'a>> {
        if text.len() < dedup_min_len(&self.config) {
            return None;
        }

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let h = hasher.finish();

        // 1. 查本地缓存 (Tier 0: No Lock)
        if let Some(token) = local_cache.get(&h) {
            return Some(DedupResult {
                tokens: vec![Token::DictRef(Cow::Owned(token.clone()))],
                count: 1,
            });
        }

        // 2. 查全局缓存 (Tier 1: Sharded Lock)
        if let Some(token) = self.global_cache.get(&h) {
            let t = token.value().clone();
            local_cache.insert(h, t.clone());
            return Some(DedupResult {
                tokens: vec![Token::DictRef(Cow::Owned(t))],
                count: 1,
            });
        }

        // 3. 发现新重复
        // P2-09：seen_hashes 分代清空——达到配置上限即整体清空重开一代，
        // 长驻进程不再随输入线性增长（DashSet 无插入序，整体清空是 bounded
        // 与实现复杂度的取舍，见 DedupConfig::max_seen_hashes 文档）。
        if self.seen_hashes.len() >= self.config.max_seen_hashes {
            self.seen_hashes.clear();
        }
        if !self.seen_hashes.insert(h) {
            if self.global_cache.len() >= self.config.max_cache_entries {
                return None;
            }

            let token = dict.add_macro(text);
            self.global_cache.insert(h, token.clone());
            local_cache.insert(h, token.clone());

            return Some(DedupResult {
                tokens: vec![Token::DictRef(Cow::Owned(token))],
                count: 1,
            });
        }

        None
    }

    /// 跨切片去重的便捷封装：使用空本地缓存调用 `dedup_cross_slice_with_local`，返回去重结果或 `None`。
    pub fn dedup_cross_slice<'a>(
        &self,
        text: &str,
        dict: &mut DictionaryEngine,
        arena: &'a Bump,
    ) -> Option<DedupResult<'a>> {
        let mut dummy = HashMap::new();
        self.dedup_cross_slice_with_local(text, dict, arena, &mut dummy)
    }
}

impl DedupEngine {
    /// 基于给定配置创建（单线程）去重引擎，初始化全局缓存、已见哈希集合与模糊缓存。
    pub fn new(config: DedupConfig) -> Self {
        DedupEngine {
            config,
            global_cache: HashMap::new(),
            seen_hashes: std::collections::HashSet::new(),
        }
    }

    /// 跨切片去重：短于 40 字符的文本直接跳过；命中全局缓存或首次发现的重复片段时返回字典引用 token。
    pub fn dedup_cross_slice<'a>(
        &mut self,
        text: &str,
        dict: &mut DictionaryEngine,
        _arena: &'a Bump,
    ) -> Option<DedupResult<'a>> {
        if text.len() < dedup_min_len(&self.config) {
            return None;
        }
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let h = hasher.finish();

        if let Some(token) = self.global_cache.get(&h) {
            return Some(DedupResult {
                tokens: vec![Token::DictRef(Cow::Owned(token.clone()))],
                count: 1,
            });
        }

        // P2-09：与 Shared 版同口径——seen_hashes 分代清空 + global_cache 容量上限
        // （单线程版旧实现两者皆无界/无上限）。
        if self.seen_hashes.len() >= self.config.max_seen_hashes {
            self.seen_hashes.clear();
        }
        if self.seen_hashes.contains(&h) {
            if self.global_cache.len() >= self.config.max_cache_entries {
                return None;
            }
            let token = dict.add_macro(text);
            self.global_cache.insert(h, token.clone());
            return Some(DedupResult {
                tokens: vec![Token::DictRef(Cow::Owned(token))],
                count: 1,
            });
        }

        self.seen_hashes.insert(h);
        None
    }
}
