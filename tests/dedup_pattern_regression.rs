use bumpalo::Bump;
use tokenslim::core::dedup_engine::{DedupConfig, DedupEngine};
use tokenslim::core::dictionary_engine::DictionaryEngine;

/// 跨切片去重回归：相同文本首次出现返回 None，第二次出现触发去重
/// 且命中计数为 1（高频模式替换为字典 token）。
#[test]
fn dedup_cross_slice_replaces_frequent_matches() {
    let mut engine = DedupEngine::new(DedupConfig {
        pattern_threshold: 2,
    ..Default::default()
    });
    let mut dict = DictionaryEngine::new();
    let arena = Bump::new();

    let text = "error REQ-ABCDEF0123456789WXYZ in module-a which is long enough";

    // First time seeing this text, returns None
    let result1 = engine.dedup_cross_slice(text, &mut dict, &arena);
    assert!(result1.is_none());

    // Second time seeing this text, should deduplicate
    let result2 = engine.dedup_cross_slice(text, &mut dict, &arena);
    assert!(result2.is_some());
    let dedup = result2.unwrap();
    assert_eq!(dedup.count, 1);
}

/// 跨切片去重降级回归：短文本不满足去重收益阈值（pattern_threshold=3）
/// 时返回 None，确保不过度压缩。
#[test]
fn dedup_cross_slice_skips_when_not_beneficial() {
    let mut engine = DedupEngine::new(DedupConfig {
        pattern_threshold: 3,
    ..Default::default()
    });
    let mut dict = DictionaryEngine::new();
    let arena = Bump::new();

    let text = "short text";
    let result = engine.dedup_cross_slice(text, &mut dict, &arena);
    assert!(result.is_none());
}
