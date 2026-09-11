//! dedup engine 测试模块

#[cfg(test)]
mod tests {
    use super::super::types::*;
    use crate::core::compression::Token;
    use crate::core::dedup_engine::SharedDedupEngine;
    use crate::core::dictionary_engine::DictionaryEngine;
    use bumpalo::Bump;
    use std::collections::HashMap;

    /// 将压缩后的 token 序列还原为可读字符串（递归展开 Text/DictRef/Marker），用于测试断言。
    fn rehydrate(tokens: &[Token]) -> String {
        let mut res = String::new();
        for t in tokens {
            match t {
                Token::Text(s) => res.push_str(s),
                Token::DictRef(s) => res.push_str(s),
                Token::Marker { value, .. } => res.push_str(value),
            }
        }
        res
    }

    /// 验证 `SharedDedupEngine::new` 在默认配置下 `pattern_threshold` 为 3（P2-56 收口后仅此阈值保留）。
    #[test]
    fn test_new() {
        let config = DedupConfig::default();
        let engine = SharedDedupEngine::new(config);
        assert_eq!(engine.config.pattern_threshold, 3);
    }

    /// 验证首次出现返回 `None`、二次出现返回以 `$M` 开头的字典引用 token。
    #[test]
    fn test_dedup_cross_slice_with_local() {
        let engine = SharedDedupEngine::new(DedupConfig::default());
        let mut dict = DictionaryEngine::new();
        let arena = Bump::new();
        let mut local_cache = HashMap::new();

        let text =
            "This is a long repeated block of text that should be deduplicated across slices.";

        // First occurrence: should be inserted into seen_hashes, but return None
        let res1 = engine.dedup_cross_slice_with_local(text, &mut dict, &arena, &mut local_cache);
        assert!(res1.is_none());

        // Second occurrence: should return a DictRef Token
        let res2 = engine.dedup_cross_slice_with_local(text, &mut dict, &arena, &mut local_cache);
        assert!(res2.is_some());
        if let Some(r) = res2 {
            assert_eq!(r.count, 1);
            let s = rehydrate(&r.tokens);
            assert!(s.starts_with("$M"));
        }
    }

    /// P2-09 回归：Shared 引擎 `seen_hashes` 达到配置上限即分代清空——
    /// 喂入远超上限的互异行后集合规模必须有界（旧实现只增不减，长驻进程
    /// 内存随输入线性增长）。
    #[test]
    fn shared_seen_hashes_generational_clear_bounded() {
        let engine = SharedDedupEngine::new(DedupConfig {
            max_seen_hashes: 64,
            ..DedupConfig::default()
        });
        let mut dict = DictionaryEngine::new();
        let arena = Bump::new();

        for i in 0..2000 {
            let text = format!("unique dedup payload line number {:06} long enough", i);
            engine.dedup_cross_slice(&text, &mut dict, &arena);
        }
        assert!(
            engine.seen_hashes.len() <= 64,
            "seen_hashes 必须被分代清空约束在 64 以内，实际 {}",
            engine.seen_hashes.len()
        );
    }

    /// P2-09 回归：单线程引擎与 Shared 版同口径——seen_hashes 有界 +
    /// `global_cache` 受 `max_cache_entries` 上限约束（旧实现两者皆无界）。
    #[test]
    fn local_engine_caches_are_bounded() {
        let mut engine = DedupEngine::new(DedupConfig {
            max_seen_hashes: 64,
            max_cache_entries: 8,
            ..DedupConfig::default()
        });
        let mut dict = DictionaryEngine::new();
        let arena = Bump::new();

        // 每行喂两次触发「首次登记 + 二次入缓存」路径。
        for round in 0..2 {
            for i in 0..500 {
                let text = format!("local engine dedup payload line {:06} long enough", i);
                engine.dedup_cross_slice(&text, &mut dict, &arena);
            }
        }
        assert!(
            engine.seen_hashes.len() <= 64,
            "seen_hashes 应有界，实际 {}",
            engine.seen_hashes.len()
        );
        assert!(
            engine.global_cache.len() <= 8,
            "global_cache 应受 max_cache_entries 约束，实际 {}",
            engine.global_cache.len()
        );
    }
}
