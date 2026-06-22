use bumpalo::Bump;
use tokenslim::core::compression::Token;
use tokenslim::core::dedup_engine::DedupEngine;
use tokenslim::core::dictionary_engine::{Dictionary, DictionaryEngine};
use tokenslim::core::plugin_dispatcher::{CompressResult, Plugin};
use tokenslim::core::text_slicer::Slice;

struct PanicPlugin;
impl Plugin for PanicPlugin {
    fn name(&self) -> &'static str {
        "panic"
    }
    fn priority(&self) -> u8 {
        10
    }
    fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
        Some(1.0)
    }
    fn compress<'a>(
        &self,
        _slice: &'a Slice<'a>,
        _dict: &mut DictionaryEngine,
        _dedup: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        panic!("intentional panic");
    }
    fn decompress(&self, c: &str, _d: &Dictionary) -> String {
        c.to_string()
    }
}

struct TimeoutPlugin;
impl Plugin for TimeoutPlugin {
    fn name(&self) -> &'static str {
        "timeout"
    }
    fn priority(&self) -> u8 {
        10
    }
    fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
        Some(1.0)
    }
    fn compress<'a>(
        &self,
        _slice: &'a Slice<'a>,
        _dict: &mut DictionaryEngine,
        _dedup: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        std::thread::sleep(std::time::Duration::from_millis(2000));
        CompressResult {
            tokens: vec![Token::Text("timeout finished".into())],
            metadata: None,
            plugin_name: None,
        }
    }
    fn decompress(&self, c: &str, _d: &Dictionary) -> String {
        c.to_string()
    }
}

#[test]
fn panic_plugin_metadata_is_stable() {
    let p = PanicPlugin;
    assert_eq!(p.name(), "panic");
    assert_eq!(p.priority(), 10);
}

#[test]
fn timeout_plugin_metadata_is_stable() {
    let p = TimeoutPlugin;
    assert_eq!(p.name(), "timeout");
    assert_eq!(p.priority(), 10);
}
