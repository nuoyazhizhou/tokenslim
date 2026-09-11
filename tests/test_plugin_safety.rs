use bumpalo::Bump;
use tokenslim::core::compression::Token;
use tokenslim::core::dedup_engine::DedupEngine;
use tokenslim::core::dictionary_engine::{Dictionary, DictionaryEngine};
use tokenslim::core::plugin_dispatcher::{CompressResult, Plugin};
use tokenslim::core::text_slicer::Slice;

/// 测试桩插件：compress 时故意 panic，用于验证插件分发器对插件异常的隔离行为。
struct PanicPlugin;
impl Plugin for PanicPlugin {
    /// 返回插件标识名 "panic"。
    fn name(&self) -> &'static str {
        "panic"
    }
    /// 返回固定优先级 10。
    fn priority(&self) -> u8 {
        10
    }
    /// 恒返回 1.0 置信度，任何输入都判定归属本插件。
    fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
        Some(1.0)
    }
    /// 故意触发 panic，检验上层对插件 panic 的捕获/隔离路径。
    fn compress<'a>(
        &self,
        _slice: &'a Slice<'a>,
        _dict: &mut DictionaryEngine,
        _dedup: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        panic!("intentional panic");
    }
    /// 原样返回压缩串（PanicPlugin 不参与解压路径）。
    fn decompress(&self, c: &str, _d: &Dictionary) -> String {
        c.to_string()
    }
}

/// 测试桩插件：compress 时睡眠 2 秒后正常返回，用于验证分发器超时/阻塞路径。
struct TimeoutPlugin;
impl Plugin for TimeoutPlugin {
    /// 返回插件标识名 "timeout"。
    fn name(&self) -> &'static str {
        "timeout"
    }
    /// 返回固定优先级 10。
    fn priority(&self) -> u8 {
        10
    }
    /// 恒返回 1.0 置信度，任何输入都判定归属本插件。
    fn detect<'a>(&self, _slice: &'a Slice<'a>) -> Option<f32> {
        Some(1.0)
    }
    /// 睡眠 2 秒模拟慢插件，检验分发器对阻塞插件的超时/降级处理。
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
    /// 原样返回压缩串（TimeoutPlugin 不参与解压路径）。
    fn decompress(&self, c: &str, _d: &Dictionary) -> String {
        c.to_string()
    }
}

/// 验证 PanicPlugin 元数据稳定（name="panic"、priority=10）。
#[test]
fn panic_plugin_metadata_is_stable() {
    let p = PanicPlugin;
    assert_eq!(p.name(), "panic");
    assert_eq!(p.priority(), 10);
}

/// 验证 TimeoutPlugin 元数据稳定（name="timeout"、priority=10）。
#[test]
fn timeout_plugin_metadata_is_stable() {
    let p = TimeoutPlugin;
    assert_eq!(p.name(), "timeout");
    assert_eq!(p.priority(), 10);
}
