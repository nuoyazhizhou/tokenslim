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
use std::sync::OnceLock;

/// P3-127 家族：草解压主路径 `restore_yaml_string` 每次调用重建 `(\$[MP]\d+)`，
/// 提升为进程级 `OnceLock` 预编译（对照 `infra_tools_common.rs` 范式）。
static RESTORE_TOKEN_RE: OnceLock<Regex> = OnceLock::new();

impl Default for YamlPlugin {
    /// YamlPlugin 默认实现：等价于 new()。
    fn default() -> Self {
        Self::new()
    }
}

impl YamlPlugin {
    /// 创建 YamlPlugin 实例（名称 yaml，优先级 145，默认配置）。
    pub fn new() -> Self {
        YamlPlugin {
            name: "yaml",
            priority: 145,
            config: YamlConfig::default(),
        }
    }
}

impl Plugin for YamlPlugin {
    /// 返回插件名称 "yaml"。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级 145。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：前 20 行中 YAML 指示符（列表项/键冒号）>3 且整体可被 serde_yaml 解析时得 0.85。
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

    /// 压缩切片：解析 YAML 并递归压缩（键字典化、长字符串路径化、序列截断），$YAML| 前缀，ROI 门控。
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

    /// 归一化：将 YAML 重新序列化为规范格式（用于 diff 比对）。
    fn normalize(&self, text: &str) -> String {
        if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(text) {
            return serde_yaml::to_string(&val).unwrap_or_else(|_| text.to_string());
        }
        text.to_string()
    }

    /// 解压：剥离 $YAML| 前缀，将 $M/$P token 用词典还原为原始 YAML。
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        if let Some(payload) = compressed.strip_prefix("$YAML|\n") {
            return restore_yaml_string(payload, dict);
        }
        compressed.to_string()
    }

    /// 返回后续插件列表（smart_path）。
    fn next_plugins(&self) -> Vec<&'static str> {
        vec!["smart_path"]
    }
}

impl YamlPlugin {
    /// 递归压缩 YAML 值：映射键按配置字典化、超长字符串路径化、序列超过限制截断。
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

/// 将压缩后的 YAML 文本中的 $M/$P token 用词典还原为原文。
fn restore_yaml_string(payload: &str, dict: &Dictionary) -> String {
    let pattern = RESTORE_TOKEN_RE.get_or_init(|| Regex::new(r"(\$[MP]\d+)").unwrap());
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
