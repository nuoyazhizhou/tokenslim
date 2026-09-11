//! json plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use crate::core::utils::json::extract_json_object;
use bumpalo::Bump;
use regex::Regex;
use std::borrow::Cow;
use std::sync::Arc;
use std::sync::OnceLock;

/// P3-127 家族：`decompress` 每次调用重建 `(\$[MP]\d+)`，提升为进程级
/// `OnceLock` 预编译（对照 `infra_tools_common.rs` 范式）。
static RESTORE_TOKEN_RE: OnceLock<Regex> = OnceLock::new();

impl JsonPlugin {
    /// 创建 JsonPlugin 实例（名称 json，优先级 146），预编译键名与 JSON 检测正则。
    pub fn new() -> Self {
        Self {
            name: "json",
            priority: 146,
            json_detect_pattern: Arc::new(Regex::new(r#"[\{\[]\s*"[^"]+"\s*:"#).unwrap()),
            config: JsonConfig::default(),
        }
    }

    /// 递归压缩 JSON 值：对象键按配置字典化，超长字符串值用路径字典 token 替换。
    fn compress_json_value_recursive(
        &self,
        val: serde_json::Value,
        dict: &mut DictionaryEngine,
    ) -> serde_json::Value {
        match val {
            serde_json::Value::Object(map) => {
                let mut new_map = serde_json::Map::new();
                for (k, v) in map.into_iter() {
                    let key = if self.config.dictionaryize_keys {
                        dict.add_macro(&k)
                    } else {
                        k
                    };
                    new_map.insert(key, self.compress_json_value_recursive(v, dict));
                }
                serde_json::Value::Object(new_map)
            }
            serde_json::Value::Array(vec) => {
                let new_vec = vec
                    .into_iter()
                    .map(|v| self.compress_json_value_recursive(v, dict))
                    .collect();
                serde_json::Value::Array(new_vec)
            }
            serde_json::Value::String(s) => {
                if s.len() > self.config.max_string_val_len {
                    serde_json::Value::String(dict.add_path_layered(&s))
                } else {
                    serde_json::Value::String(s)
                }
            }
            _ => val,
        }
    }
}

impl Plugin for JsonPlugin {
    /// 返回插件名称 "json"。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级 146。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：完整 JSON 对象/数组得 1.0，括号平衡的嵌入对象 0.85，正则形态 0.8。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.trim();
        if (text.starts_with('{') && text.ends_with('}'))
            || (text.starts_with('[') && text.ends_with(']'))
        {
            return Some(1.0);
        }
        if extract_json_object(text).is_some() {
            return Some(0.85);
        }
        if self.json_detect_pattern.is_match(text) {
            return Some(0.8);
        }
        None
    }

    /// 压缩切片：解析 JSON（含从噪声中提取），递归压缩后加 $JSON| 前缀，ROI 门控。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        let parsed = serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .or_else(|| {
                extract_json_object(text)
                    .and_then(|chunk| serde_json::from_str::<serde_json::Value>(chunk.raw).ok())
            });

        let compacted = if let Some(val) = parsed {
            let compressed_val = self.compress_json_value_recursive(val, dict_engine);
            let compressed_string = serde_json::to_string(&compressed_val).unwrap_or_default();
            format!("$JSON|{}\n", compressed_string)
        } else {
            text.to_string()
        };

        // 法则 A ROI 门控：短 JSON 样本（单行 `{}` / `{"x":1}` 等）加 `$JSON|` 前缀必扩张。
        // 参考 `docs/prompts/non_vcs_classical_prompts.md` § D.2.1。
        let final_text = crate::core::utils::roi::prefer_non_expanding(text, compacted);

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(final_text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 解压：剥离 $JSON| 前缀，将 $M/$P token 用词典还原为原始 JSON。
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        if let Some(payload) = compressed.strip_prefix("$JSON|") {
            let pattern = RESTORE_TOKEN_RE.get_or_init(|| Regex::new(r"(\$[MP]\d+)").unwrap());
            let restored = pattern
                .replace_all(payload, |caps: &regex::Captures| {
                    let token = caps.get(1).unwrap().as_str();
                    if let Some(original) = dict.resolve(token) {
                        let s: String = original.to_string();
                        s
                    } else {
                        token.to_string()
                    }
                })
                .into_owned();
            return restored;
        }
        compressed.to_string()
    }
}

impl Clone for JsonPlugin {
    /// 克隆插件实例：复制名称、优先级、正则与配置。
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            priority: self.priority,
            json_detect_pattern: self.json_detect_pattern.clone(),
            config: self.config.clone(),
        }
    }
}
