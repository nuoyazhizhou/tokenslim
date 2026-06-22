//! dotnet plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

static STACK_FRAME_RE: Lazy<Regex> = Lazy::new(|| {
    // 修复：多行样本里的 `at <method>(...)` 可能不在首行。使用 `(?m)` 让 `^` 匹配每行行首，
    // 使 detect 能识别真实 .NET stack trace（多行）样本；不改变单行语义。
    Regex::new(r"(?m)^\s*at\s+(?P<method>[\w\.<>]+)\((?P<args>.*)\)(?:\s+in\s+(?P<file>.*?):line\s+(?P<line>\d+))?").unwrap()
});
static MSBUILD_ERR_RE: Lazy<Regex> = Lazy::new(|| {
    // 修复：与 STACK_FRAME_RE 同理。多行 MSBuild 日志里 `error CSxxxx` 行可能不在首行。
    Regex::new(
        r"(?m)^(?P<file>.*)\((?P<line>\d+),(?P<col>\d+)\):\s+error\s+(?P<code>CS\d+):\s+(?P<msg>.*)",
    )
    .unwrap()
});

impl DotNetPlugin {
    /// 创建一个新的 .NET 插件实例。
    ///
    /// 专注于处理 .NET 运行时异常堆栈（Stack Trace）和 MSBuild 编译日志。
    pub fn new() -> Self {
        DotNetPlugin {
            name: "dotnet",
            priority: 120,
            config: DotNetConfig::default(),
        }
    }

    /// 压缩 .NET 异常堆栈信息。
    ///
    /// 会对方法名中的常用命名空间进行折叠，并对源码文件路径进行分层压缩。
    fn compress_stack_trace(&self, text: &str, _dict: &mut DictionaryEngine) -> String {
        let mut result = String::new();
        for line in text.lines() {
            if let Some(caps) = STACK_FRAME_RE.captures(line) {
                let method = &caps["method"];
                let _args = caps.name("args").map(|m| m.as_str()).unwrap_or("");
                let file = caps.name("file").map(|m| m.as_str()).unwrap_or("");
                let line_no = caps.name("line").map(|m| m.as_str()).unwrap_or("");

                let file_ref = if !file.is_empty() && !line_no.is_empty() {
                    format!(" in {file}:line {line_no}")
                } else if !file.is_empty() {
                    format!(" in {file}")
                } else {
                    String::new()
                };

                // 简化参数列表，但保留完整方法名和源码定位。
                result.push_str(&format!("  at {}(...){file_ref}\n", method));
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }
        result
    }

    /// 压缩 MSBuild 编译错误输出。
    ///
    /// 将重复的文件路径引用替换为字典 Token。
    fn compress_msbuild(&self, text: &str, _dict: &mut DictionaryEngine) -> String {
        MSBUILD_ERR_RE
            .replace_all(text, |caps: &regex::Captures| {
                let file = &caps["file"];
                let line = &caps["line"];
                let col = &caps["col"];
                let code = &caps["code"];
                let msg = &caps["msg"];
                format!("{}({},{}): error {}: {}", file, line, col, code, msg)
            })
            .into_owned()
    }
}

impl Plugin for DotNetPlugin {
    /// 返回插件的唯一标识名称，用于日志记录和监控。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回插件的执行优先级。数值越小，执行调度越靠前。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 根据常见的 .NET 命名空间和堆栈特征识别切片。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        let mut score: f32 = 0.0;

        if text.contains("System.") || text.contains("Microsoft.") {
            score += 0.3;
        }
        if STACK_FRAME_RE.is_match(text) {
            score += 0.4;
        }
        if MSBUILD_ERR_RE.is_match(text) {
            score += 0.5;
        }

        if score > 0.3 {
            Some(score.min(1.0))
        } else {
            None
        }
    }

    /// 执行 .NET 相关的文本优化处理。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();

        let mut processed = text.to_string();

        if self.config.fold_stack_traces && STACK_FRAME_RE.is_match(text) {
            processed = self.compress_stack_trace(&processed, dict_engine);
        }

        if self.config.clean_msbuild_output && MSBUILD_ERR_RE.is_match(text) {
            processed = self.compress_msbuild(&processed, dict_engine);
        }

        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(processed))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 对文本进行归一化处理（用于日志比对）。消除时间戳、随机 Hash、乱序参数等 Diff 噪音。
    fn normalize(&self, text: &str) -> String {
        let mut result = text.to_string();
        // 抹除 MSBuild 的路径 (e.g., C:\Users\name\ -> C:\[USER]\)
        let user_re = regex::Regex::new(r"[a-zA-Z]:\\Users\\[^\\]+\\").unwrap();
        result = user_re
            .replace_all(&result, r"C:\Users\[USER]\")
            .to_string();

        result
    }

    /// .NET 插件当前的优化属于文本重写类，具备一定的不可逆性（如简化参数并删除行号信息）。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn load_config(&mut self, config: &dyn std::any::Any) -> Result<(), String> {
        if let Some(new_config) = config.downcast_ref::<DotNetConfig>() {
            self.config = new_config.clone();
            Ok(())
        } else {
            Err("Invalid config type".to_string())
        }
    }
}
