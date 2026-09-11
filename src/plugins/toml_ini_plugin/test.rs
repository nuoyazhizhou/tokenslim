//! toml_ini 插件单元测试

use super::types::*;
use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::{Slice, SliceType};
use std::borrow::Cow;
use std::path::PathBuf;

/// 构造轻量 `Slice`（沿用 content_classifier/holdout 的探测范式）。
fn fixt(text: &str) -> crate::core::text_slicer::Slice<'_> {
    Slice {
        id: 0,
        text: Cow::Borrowed(text),
        slice_type: SliceType::LogBlock,
        offset: 0,
        line_start: 1,
        line_end: text.lines().count().max(1),
        file_metadata: None,
        flags: Default::default(),
    }
}

/// 读取 `classifier_holdout/structured/gap_probes/{toml|ini}/` 的物理探针样本
/// （红线：测试必须加载物理文件，禁止手写 mock）。
fn read_probe(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("classifier_holdout/structured/gap_probes")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取探针 {rel} 失败: {e}"))
}

#[test]
fn detects_toml_manifest() {
    let p = TomlIniPlugin::new();
    let text = read_probe("toml/case_001_cargo_manifest.toml");
    let slice = fixt(&text);
    assert_eq!(p.name(), "toml_ini");
    assert_eq!(p.priority(), 145);
    assert!(
        p.detect(&slice).is_some(),
        "cargo manifest.toml 应被识别为配置类"
    );
}

#[test]
fn detects_ini_unquoted_values() {
    let p = TomlIniPlugin::new();
    let text = read_probe("ini/case_001_mysql_cfg.ini");
    let slice = fixt(&text);
    assert!(p.detect(&slice).is_some(), "mysql_cfg.ini 应被识别为配置类");
}

#[test]
fn compress_large_pure_toml() {
    let p = TomlIniPlugin::new();
    let text = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("classifier_holdout/structured/mixed/case_002_large_pure_toml.conf"),
    )
    .expect("读取纯 TOML 样本失败");
    let slice = fixt(&text);
    assert!(p.detect(&slice).is_some(), "大尺寸纯 TOML 应被识别为配置类");
    // 直接验证 toml 解析能力（压缩质量归 roi 门控，这里只确认能解析归一化）
    let parsed = toml::from_str::<toml::Value>(&text);
    assert!(
        parsed.is_ok(),
        "大尺寸纯 TOML 应能被 toml 解析: {:?}",
        parsed.err()
    );
    // 对比归一化尺寸：确认 `$TOML|` 前缀下仍有压缩收益（否则 roi 会退回原文，压缩无效）
    let normalized = p.normalize(&text);
    eprintln!(
        "纯 TOML 尺寸: 原始={} 归一化={} 归一化+前缀={}",
        text.len(),
        normalized.len(),
        normalized.len() + "$TOML|\n".len(),
    );
    assert!(
        normalized.len() + "$TOML|\n".len() < text.len(),
        "归一化应比原文更紧凑，否则 roi 门控退回原文、压缩无收益"
    );
}

#[test]
fn rejects_plain_shell_env_snippet() {
    let p = TomlIniPlugin::new();
    let text = "export FOO=1\nexport BAR=2\nrun_cmd";
    let slice = fixt(text);
    assert!(
        p.detect(&slice).is_none(),
        "无结构的 shell 片段不应被误判为配置"
    );
}
