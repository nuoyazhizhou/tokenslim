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

/// P3-139（P3-127 家族扩展）：`compress` 的 while 循环体内每行与 `normalize` 均
/// 重建 GUID 正则（最重一例）+ `normalize` 重建内存地址正则。提升为进程级
/// `OnceLock` 预编译（对照 `infra_tools_common.rs` 范式）。
static GUID_RE: OnceLock<Regex> = OnceLock::new();
static ADDR_RE: OnceLock<Regex> = OnceLock::new();

impl UnityUnrealPlugin {
    /// 创建 UnityUnrealPlugin 实例：初始化名称(unity_unreal)与优先级(88)。
    pub fn new() -> Self {
        Self {
            name: "unity_unreal",
            priority: 88,
        }
    }
}

impl Plugin for UnityUnrealPlugin {
    /// 返回插件名称标识 "unity_unreal"。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级(88)，用于压缩调度排序。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测切片文本是否属于 Unity/Unreal 引擎日志：命中 LogUObject/LogHAL/LogLinker/FAndroidApp 或 Unity 资源特征时返回 0.9，通用资源加载(.uasset/.prefab/.mat)返回 0.8，否则 None。
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

    /// 压缩 Unity/Unreal 引擎日志：聚合连续资源加载噪音为 [ENGINE_ASSETS: N objects loaded]，将 GUID 注册为宏、路径经路径压缩器归一化，返回无损压缩结果(单 Text token)。
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
            let guid_re = GUID_RE.get_or_init(|| Regex::new(r"\b[A-F0-9]{32}\b").unwrap());
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

    /// 解压引擎日志：本插件压缩无损，直接返回原字符串。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    /// 归一化文本：将 32 位十六进制 GUID 替换为 [GUID]、内存地址(0x...)替换为 0x[ADDR]，便于去重。
    fn normalize(&self, text: &str) -> String {
        let mut result = text.to_string();
        // 抹除 GUID
        let guid_re = GUID_RE.get_or_init(|| Regex::new(r"\b[A-F0-9]{32}\b").unwrap());
        result = guid_re.replace_all(&result, "[GUID]").to_string();

        // 抹除内存地址
        let addr_re = ADDR_RE.get_or_init(|| Regex::new(r"0x[0-9a-fA-F]{8,16}").unwrap());
        result = addr_re.replace_all(&result, "0x[ADDR]").to_string();

        result
    }
}

impl Clone for UnityUnrealPlugin {
    /// 克隆插件实例：复制名称与优先级。
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            priority: self.priority,
        }
    }
}
