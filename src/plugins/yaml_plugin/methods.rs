//! yaml plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::borrow::Cow;

impl Default for YamlPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl YamlPlugin {
    pub fn new() -> Self {
        YamlPlugin {
            name: "yaml",
            priority: 145,
            config: YamlConfig::default(),
        }
    }
}

impl Plugin for YamlPlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        let lines: Vec<&str> = text.lines().take(20).collect();
        let mut yaml_indicators = 0;
        for line in &lines {
            if line.trim_start().starts_with('-')
                || Regex::new(r"^[a-zA-Z0-9_-]+\s*:")
                    .unwrap()
                    .is_match(line.trim_start())
            {
                yaml_indicators += 1;
            }
        }
        if yaml_indicators > 3 {
            if serde_yaml::from_str::<serde_yaml::Value>(text).is_ok() {
                return Some(0.85);
            }
        }
        None
    }

    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        let compacted = if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(text) {
            let compressed_val = self.compress_yaml_value_recursive(val, dict_engine, 0);
            let compact_yaml = serde_yaml::to_string(&compressed_val).unwrap();
            format!("$YAML|\n{}", compact_yaml)
        } else {
            // YAML 解析失败时，至少做 ANSI/空白对齐的 no-op 返回
            arena.alloc_str(text).to_string()
        };

        // 法则 A ROI 门控：短 YAML 样本加 `$YAML|\n` 前缀会显著扩张
        // （case_006_single_line 从 12B→19B 扩张 80%）。
        // 参考 `docs/prompts/non_vcs_classical_prompts.md` § D.2.2。
        let final_text = crate::core::utils::roi::prefer_non_expanding(text, compacted);

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(final_text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    fn normalize(&self, text: &str) -> String {
        if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(text) {
            return serde_yaml::to_string(&val).unwrap_or_else(|_| text.to_string());
        }
        text.to_string()
    }

    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        if let Some(payload) = compressed.strip_prefix("$YAML|\n") {
            return restore_yaml_string(payload, dict);
        }
        compressed.to_string()
    }

    fn next_plugins(&self) -> Vec<&'static str> {
        vec!["smart_path"]
    }

    fn load_config(&mut self, config: &dyn std::any::Any) -> Result<(), String> {
        if let Some(c) = config.downcast_ref::<YamlConfig>() {
            self.config = c.clone();
            return Ok(());
        }
        Err("Invalid config type".to_string())
    }
}

impl YamlPlugin {
    fn compress_yaml_value_recursive(
        &self,
        val: serde_yaml::Value,
        dict: &mut DictionaryEngine,
        depth: usize,
    ) -> serde_yaml::Value {
        if depth > self.config.max_depth {
            return serde_yaml::Value::String("...depth limit...".to_string());
        }

        match val {
            serde_yaml::Value::Mapping(map) => {
                let mut new_map = serde_yaml::Mapping::new();
                for (k, v) in map.into_iter() {
                    let key = if let Some(k_str) = k.as_str() {
                        if self.config.dictionaryize_keys {
                            serde_yaml::Value::String(dict.add_macro(k_str))
                        } else {
                            k
                        }
                    } else {
                        k
                    };
                    new_map.insert(key, self.compress_yaml_value_recursive(v, dict, depth + 1));
                }
                serde_yaml::Value::Mapping(new_map)
            }
            serde_yaml::Value::Sequence(seq) => {
                let count = seq.len();
                let limit = self.config.max_seq_len;
                let mut new_seq = Vec::new();
                for (i, v) in seq.into_iter().enumerate() {
                    if i >= limit {
                        new_seq.push(serde_yaml::Value::String(format!(
                            "... {} more elements truncated ...",
                            count - limit
                        )));
                        break;
                    }
                    new_seq.push(self.compress_yaml_value_recursive(v, dict, depth + 1));
                }
                serde_yaml::Value::Sequence(new_seq)
            }
            serde_yaml::Value::String(st) => {
                if st.len() > self.config.max_string_val_len {
                    serde_yaml::Value::String(dict.add_path_layered(&st))
                } else {
                    serde_yaml::Value::String(st)
                }
            }
            _ => val,
        }
    }
}

fn restore_yaml_string(payload: &str, dict: &Dictionary) -> String {
    let pattern = Regex::new(r"(\$[MP]\d+)").unwrap();
    pattern
        .replace_all(payload, |caps: &regex::Captures| {
            let token = caps.get(1).unwrap().as_str();
            if let Some(original) = dict.resolve(token) {
                let s: String = original.to_string();
                s
            } else {
                token.to_string()
            }
        })
        .into_owned()
}
