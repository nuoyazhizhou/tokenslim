use bumpalo::Bump;
use tokenslim::core::dedup_engine::{DedupConfig, DedupEngine};
use tokenslim::core::dictionary_engine::DictionaryEngine;

#[test]
fn dedup_cross_slice_replaces_frequent_matches() {
    let mut engine = DedupEngine::new(DedupConfig {
        line_threshold: 2,
        stack_frame_threshold: 2,
        path_threshold: 2,
        pattern_threshold: 2,
        fuzzy_threshold: 0.9,
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

#[test]
fn dedup_cross_slice_skips_when_not_beneficial() {
    let mut engine = DedupEngine::new(DedupConfig {
        line_threshold: 2,
        stack_frame_threshold: 2,
        path_threshold: 2,
        pattern_threshold: 3,
        fuzzy_threshold: 0.9,
    });
    let mut dict = DictionaryEngine::new();
    let arena = Bump::new();

    let text = "short text";
    let result = engine.dedup_cross_slice(text, &mut dict, &arena);
    assert!(result.is_none());
}
