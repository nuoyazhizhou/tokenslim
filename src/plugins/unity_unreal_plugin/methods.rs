use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::borrow::Cow;

impl UnityUnrealPlugin {
    pub fn new() -> Self {
        Self {
            name: "unity_unreal",
            priority: 88,
            config: UnityUnrealConfig::default(),
        }
    }
}

impl Plugin for UnityUnrealPlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();

        // 1. Unreal 特征
        if text.contains("LogUObject")
            || text.contains("LogHAL")
            || text.contains("LogLinker")
            || text.contains("FAndroidApp")
        {
            return Some(0.9);
        }

        // 2. Unity 特征
        if text.contains("Unloading ")
            || text.contains("Building AssetBundle")
            || text.contains("Shader compilation")
        {
            return Some(0.9);
        }

        // 3. 通用资源加载特征
        if text.contains("Loading")
            && (text.contains(".uasset") || text.contains(".prefab") || text.contains(".mat"))
        {
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
        let mut result = String::with_capacity(text.len());
        let lines: Vec<&str> = text.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];

            // 聚合资源加载噪音 (e.g., LogUObjectGlobals: [CC715D28] Loading Object ...)
            if line.contains("Loading Object")
                || line.contains("Loading ") && line.contains(".uasset")
            {
                let mut count = 1;
                while i + count < lines.len() {
                    let next = lines[i + count];
                    if next.contains("Loading Object")
                        || next.contains("Loading ") && next.contains(".uasset")
                    {
                        count += 1;
                        continue;
                    }
                    break;
                }

                if count > 5 {
                    result.push_str(&format!("[ENGINE_ASSETS: {} objects loaded]\n", count));
                    i += count;
                    continue;
                }
            }

            // 路径和 GUID 压缩
            let mut processed_line = line.to_string();
            // 匹配 GUID: [A-F0-9]{32}
            let guid_re = Regex::new(r"\b[A-F0-9]{32}\b").unwrap();
            processed_line = guid_re
                .replace_all(&processed_line, |caps: &regex::Captures| {
                    dict_engine.add_macro(caps.get(0).unwrap().as_str())
                })
                .to_string();

            // 借用路径原子函数
            let optimized = crate::core::path_compressor::methods::replace_paths_in_text(
                &processed_line,
                dict_engine,
            )
            .into_owned();

            result.push_str(&optimized);
            result.push('\n');
            i += 1;
        }

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(result))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn normalize(&self, text: &str) -> String {
        let mut result = text.to_string();
        // 抹除 GUID
        let guid_re = Regex::new(r"\b[A-F0-9]{32}\b").unwrap();
        result = guid_re.replace_all(&result, "[GUID]").to_string();

        // 抹除内存地址
        let addr_re = Regex::new(r"0x[0-9a-fA-F]{8,16}").unwrap();
        result = addr_re.replace_all(&result, "0x[ADDR]").to_string();

        result
    }

    fn load_config(&mut self, config: &dyn std::any::Any) -> Result<(), String> {
        if let Some(new_config) = config.downcast_ref::<UnityUnrealConfig>() {
            self.config = new_config.clone();
            Ok(())
        } else {
            Err("Invalid config type".to_string())
        }
    }
}

impl Clone for UnityUnrealPlugin {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            priority: self.priority,
            config: self.config.clone(),
        }
    }
}
