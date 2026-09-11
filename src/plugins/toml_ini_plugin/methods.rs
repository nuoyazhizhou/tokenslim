//! toml_ini plugin 方法实现

//! ## 检测与压缩逻辑
//!
//! TOML 与 INI 共享同一种行结构：`[section]` 段头 + `key = value` / `key=value` 键值对。
//! 本插件用**行级结构探测**（不依赖 toml 解析）来同时覆盖二者——INI 的值常不带引号
//! （`port=3306`、`log_dir = /var/log/acme`），`toml::from_str` 无法解析，故 `detect`
//! 只看「段头/键值行」数量，不做语义解析。
//!
//! `compress` 则尽力而为：能 `toml::from_str` 解析的走「规范化重排 + 去注释 + 紧凑化」；
//! 解析失败的（典型 INI 无引号裸值）原样返回，交给 [`prefer_non_expanding`] 保留更短者。
//! 插件的主要价值在「把 TOML/INI 从 CodeBlock/smart_path 兜底中认领出来，按结构化配置
//! 处理」，避免被通用文本类插件误加工。

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::borrow::Cow;
use std::sync::OnceLock;

/// P3-127 家族：`is_config_like` 每次调用重建 2 个正则（detect 高频热路径），
/// 提升为进程级 `OnceLock` 预编译（对照 `infra_tools_common.rs` 范式）。
static SECTION_RE: OnceLock<Regex> = OnceLock::new();
static KEYVAL_RE: OnceLock<Regex> = OnceLock::new();

impl Default for TomlIniPlugin {
    /// 默认实现：等价于 `new()`。
    fn default() -> Self {
        Self::new()
    }
}

impl TomlIniPlugin {
    /// 创建新实例：名称 `toml_ini`，优先级 145（与 yaml 同级、高于 json 146，
    /// 保证配置类文本优先被结构化配置插件认领），默认配置。
    pub fn new() -> Self {
        TomlIniPlugin {
            name: "toml_ini",
            priority: 145,
            config: TomlIniConfig::default(),
        }
    }
}

impl Plugin for TomlIniPlugin {
    /// 返回插件名称 "toml_ini"。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回插件优先级 145。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：前 20 行中「段头 `[xxx]`」或「键值 `key=...`」行数达标，则认定为配置类
    /// 文本，返回 0.85。纯行级统计，不自造语义解析，覆盖 TOML 与无引号 INI 两种写法。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        if is_config_like(text, self.config.min_keyval_lines) {
            Some(0.85)
        } else {
            None
        }
    }

    /// 压缩切片：能 toml 解析则规范化紧凑化；否则原样返回（交给 ROI 门控择小保留）。
    /// 键值不字典化——键是配置的核心语义标识，字典化会破坏可读性且 toml 键不允许 `$` token。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        let compacted = if let Ok(val) = toml::from_str::<toml::Value>(text) {
            match toml::to_string(&val) {
                Ok(normalized) => format!("$TOML|\n{}", normalized),
                Err(_) => arena.alloc_str(text).to_string(),
            }
        } else {
            arena.alloc_str(text).to_string()
        };

        // ROI 门控：短配置加前缀会显著扩张，保留更短者。
        let final_text = crate::core::utils::roi::prefer_non_expanding(text, compacted);

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(final_text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 归一化：能解析则重新序列化为规范格式（用于 diff 比对）。
    fn normalize(&self, text: &str) -> String {
        if let Ok(val) = toml::from_str::<toml::Value>(text) {
            return toml::to_string(&val).unwrap_or_else(|_| text.to_string());
        }
        text.to_string()
    }

    /// 解压：剥离 `$TOML|` 前缀（`compress` 未字典化键，还原即原样返回）。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        if let Some(payload) = compressed.strip_prefix("$TOML|\n") {
            return payload.to_string();
        }
        compressed.to_string()
    }

    /// 推荐后续插件：smart_path。
    fn next_plugins(&self) -> Vec<&'static str> {
        vec!["smart_path"]
    }
}

/// 行级结构判定：计数「段头 / 键值行」。命中条件为「至少 1 个段头 且 至少 1 个键值行」
/// 或「键值行 ≥ `min_keyval`」。校验键值行时排除注释与空行。
fn is_config_like(text: &str, min_keyval: usize) -> bool {
    let re_section =
        SECTION_RE.get_or_init(|| Regex::new(r"^\s*\[[a-zA-Z0-9_.\-]+\]\s*(#.*)?$").unwrap());
    let re_keyval = KEYVAL_RE.get_or_init(|| Regex::new(r"^\s*[a-zA-Z0-9_.\-]+\s*(=\s*)").unwrap());
    let mut sections = 0usize;
    let mut keyvals = 0usize;
    for line in text.lines().take(40) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        if re_section.is_match(line) {
            sections += 1;
        } else if re_keyval.is_match(line) {
            keyvals += 1;
        }
    }
    // 逐行 `is_match` 配合 `^\s*` 锚点，避免对整段文本做多行匹配引发的误判。
    (sections >= 1 && keyvals >= 1) || keyvals >= min_keyval
}
