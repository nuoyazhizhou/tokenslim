//! dedup engine 测试模块

#[cfg(test)]
mod tests {
    use super::super::types::*;
    use crate::core::compression::Token;
    use crate::core::dedup_engine::SharedDedupEngine;
    use crate::core::dictionary_engine::DictionaryEngine;
    use bumpalo::Bump;
    use std::collections::HashMap;

    fn rehydrate(tokens: &[Token]) -> String {
        let mut res = String::new();
        for t in tokens {
            match t {
                Token::Text(s) => res.push_str(s),
                Token::Repeat { token, count } => {
                    let s = rehydrate(&[*token.clone()]);
                    for _ in 0..*count {
                        res.push_str(&s);
                    }
                }
                Token::DictRef(s) => res.push_str(s),
                Token::Diff { base, patch } => {
                    res.push_str(&format!("base={} patch={}", base, patch))
                }
                _ => {}
            }
        }
        res
    }

    #[test]
    fn test_new() {
        let config = DedupConfig::default();
        let engine = SharedDedupEngine::new(config);
        assert_eq!(engine.config.line_threshold, 3);
    }

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
}
