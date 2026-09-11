//! 受控冻结契约生成器：android_gradle case_009_gradle_d8 的 compact 展示文本。
//!
//! 冻结契约（独立复审裁决 2026-08-19，路径 2）：
//! 冻结 `compact.txt` 的规范产物 = **AndroidGradlePlugin::compress 直接输出的展示文本**
//! （`Token::Text` 拼接），与全部既有冻结用例格式一致；**不是** CLI `compress` 的
//! JSON 序列化 tokens（后者含 dictionary/协议结构，格式不同，如另立契约须单独审批）。
//!
//! 生成命令（可在 crate 根目录执行，亦可经 `tokenslim run cargo` 包裹）：
//! ```sh
//! cargo run --release --example gen_frozen_compact_case009 \
//!   > docs/audit/android_gradle_plugin/cases/case_009_gradle_d8/compact.txt
//! ```
//! 本生成器不读取 `target/` 报告（`target/` 被 .gitignore 排除、不可审计）；权威输入
//! 一律取自版本控制的 `samples/android_gradle_plugin/case_009_gradle_d8.log`。
//! 运行时自断言：权威输入 427B；压缩产物精确 138B、SHA-256 = b4772f75…（不匹配即 panic）。
//! 对应集成测试：`cargo test --release case_009_gradle_d8_compact_frozen_baseline`。
//! 源码 HEAD、生成命令与二进制 SHA 以重采集证据（recollect_t009.py / 工单）记录。
use std::borrow::Cow;

use sha2::{Digest, Sha256};

use tokenslim::core::compression::Token;
use tokenslim::core::dedup_engine::{DedupConfig, DedupEngine};
use tokenslim::core::dictionary_engine::DictionaryEngine;
use tokenslim::core::plugin_dispatcher::Plugin;
use tokenslim::core::text_slicer::{Slice, SliceType};
use tokenslim::plugins::android_gradle_plugin::AndroidGradlePlugin;

/// 与 `src/plugins/android_gradle_plugin/showcase.rs` 的 `compress_text` 完全一致。
fn compress_text(plugin: &AndroidGradlePlugin, text: &str) -> String {
    let slice = Slice {
        id: 1,
        text: Cow::Borrowed(text),
        slice_type: SliceType::LogBlock,
        offset: 0,
        line_start: 1,
        line_end: text.lines().count().max(1),
        file_metadata: None,
        flags: Default::default(),
    };
    let mut dict = DictionaryEngine::new();
    let mut dedup = DedupEngine::new(DedupConfig::default());
    let arena = bumpalo::Bump::new();
    let result = plugin.compress(&slice, &mut dict, &mut dedup, &arena);
    result
        .tokens
        .iter()
        .filter_map(|t| match t {
            Token::Text(s) => Some(s.as_ref()),
            _ => None,
        })
        .collect::<String>()
}

fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    h.finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest_dir)
        .join("samples")
        .join("android_gradle_plugin")
        .join("case_009_gradle_d8.log");
    let raw = std::fs::read_to_string(&path).expect("读取权威样本 case_009_gradle_d8.log");
    assert_eq!(raw.len(), 427, "权威输入应为 427B，实际 {}", raw.len());
    assert!(!raw.contains('\r'), "权威输入应为 LF-only");

    let plugin = AndroidGradlePlugin::new();
    let compacted = compress_text(&plugin, &raw);

    let digest = sha256_hex(&compacted);
    assert_eq!(
        compacted.len(),
        138,
        "冻结契约：压缩产物应精确 138B，实际 {}",
        compacted.len()
    );
    assert_eq!(
        digest, "b4772f758b53090be2d927ede51cd2722c1cc9096ae06b11889f4021eb3c3a09",
        "冻结契约：压缩产物 SHA-256 应等于冻结哈希 b4772f75…"
    );
    eprintln!(
        "[gen_frozen_compact_case009] compact={}B sha256={}",
        compacted.len(),
        digest
    );
    // 输出 compact（含尾换行）到 stdout，供重定向写入 compact.txt
    print!("{}", compacted);
}
