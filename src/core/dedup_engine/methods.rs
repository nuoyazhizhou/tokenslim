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

impl SharedDedupEngine {
    pub fn new(config: DedupConfig) -> Self {
        Self {
            config,
            global_cache: DashMap::new(),
            seen_hashes: DashSet::new(),
            fuzzy_cache: DashMap::new(),
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
        if text.len() < 12 {
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
        if !self.seen_hashes.insert(h) {
            if self.global_cache.len() >= 200000 {
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

    pub fn dedup_cross_slice<'a>(
        &self,
        text: &str,
        dict: &mut DictionaryEngine,
        arena: &'a Bump,
    ) -> Option<DedupResult<'a>> {
        let mut dummy = HashMap::new();
        self.dedup_cross_slice_with_local(text, dict, arena, &mut dummy)
    }

    pub fn dedup_path<'a>(&self, _text: &str, _arena: &'a Bump) -> Option<DedupResult<'a>> {
        None
    }
}

impl DedupEngine {
    pub fn new(config: DedupConfig) -> Self {
        DedupEngine {
            config,
            global_cache: HashMap::new(),
            seen_hashes: std::collections::HashSet::new(),
            fuzzy_cache: HashMap::new(),
        }
    }

    pub fn dedup_cross_slice<'a>(
        &mut self,
        text: &str,
        dict: &mut DictionaryEngine,
        _arena: &'a Bump,
    ) -> Option<DedupResult<'a>> {
        if text.len() < 40 {
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

        if self.seen_hashes.contains(&h) {
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
