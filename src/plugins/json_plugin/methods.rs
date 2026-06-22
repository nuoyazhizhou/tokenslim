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
use std::any::Any;
use std::borrow::Cow;
use std::sync::Arc;

impl JsonPlugin {
    pub fn new() -> Self {
        Self {
            name: "json",
            priority: 146,
            key_pattern: Arc::new(Regex::new(r#""([^"]+)":\s*"#).unwrap()),
            json_detect_pattern: Arc::new(Regex::new(r#"[\{\[]\s*"[^"]+"\s*:"#).unwrap()),
            config: JsonConfig::default(),
        }
    }

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
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }

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

    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        if let Some(payload) = compressed.strip_prefix("$JSON|") {
            let pattern = Regex::new(r"(\$[MP]\d+)").unwrap();
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

    fn load_config(&mut self, config: &dyn Any) -> Result<(), String> {
        if let Some(c) = config.downcast_ref::<JsonConfig>() {
            self.config = c.clone();
            return Ok(());
        }
        Err("Invalid config".to_string())
    }
}

impl Clone for JsonPlugin {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            priority: self.priority,
            key_pattern: self.key_pattern.clone(),
            json_detect_pattern: self.json_detect_pattern.clone(),
            config: self.config.clone(),
        }
    }
}
