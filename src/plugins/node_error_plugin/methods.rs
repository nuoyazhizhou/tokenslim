//! node error plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_config_loader::CompiledPluginConfig;
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::any::Any;
use std::sync::Arc;

impl Default for NodeErrorPlugin {
    /// 提供该插件类型的默认配置实现。
    fn default() -> Self {
        Self::new()
    }
}

impl NodeErrorPlugin {
    /// 实例化并返回该插件的默认配置对象。
    pub fn new() -> Self {
        let exception_pattern =
            Regex::new(r"^(?P<class>[A-Z]\w*(?:Error|Exception)): (?P<msg>.*)$").unwrap();
        let frame_pattern = Regex::new(
            r"^\s*at (?P<func>[^\s]+) \((?P<file>[^:]+)(?::(?P<line>\d+):(?P<col>\d+))?\)$",
        )
        .unwrap();

        NodeErrorPlugin {
            name: "node_error",
            priority: 85,
            exception_pattern: Arc::new(exception_pattern),
            frame_pattern: Arc::new(frame_pattern),
            config: None,
        }
    }

    /// 使用给定的自定义配置结构体来初始化一个新的插件实例。
    pub fn with_config(config: CompiledPluginConfig) -> Self {
        let exception_pattern =
            Regex::new(r"^(?P<class>[A-Z]\w*(?:Error|Exception)): (?P<msg>.*)$").unwrap();
        let frame_pattern = Regex::new(
            r"^\s*at (?P<func>[^\s]+) \((?P<file>[^:]+)(?::(?P<line>\d+):(?P<col>\d+))?\)$",
        )
        .unwrap();

        NodeErrorPlugin {
            name: Box::leak(config.name.clone().into_boxed_str()) as &'static str,
            priority: config.priority,
            exception_pattern: Arc::new(exception_pattern),
            frame_pattern: Arc::new(frame_pattern),
            config: Some(config),
        }
    }

    /// 判断异常类名是否在「字面量保留白名单」之内。
    ///
    /// 法则 D 防失忆：任何「错误/异常」类关键词压缩后必须可识别保留。
    /// Node.js / JavaScript 常见异常类名应直接以字面量进入 compact 输出，
    /// 严禁被 `dict_engine.add_package` 字典化为 `$PKn` — 否则 LLM 无法识别异常类型。
    ///
    /// 覆盖的类名取自 ECMAScript 标准内置错误类 + 常见派生类（`SomeError` / `SomeException` 模式）。
    fn should_preserve_class_name(class_name: &str) -> bool {
        // 完全匹配的高频内置类
        const BUILTIN: &[&str] = &[
            "Error",
            "SyntaxError",
            "TypeError",
            "ReferenceError",
            "RangeError",
            "URIError",
            "EvalError",
            "AssertionError",
            "AggregateError",
            "InternalError",
            "Exception",
            "UnhandledPromiseRejectionWarning",
            "DeprecationWarning",
        ];
        if BUILTIN.contains(&class_name) {
            return true;
        }

        // 命名模式兜底：以 `Error` 或 `Exception` 结尾的自定义类也需保留字面量，
        // 例如 `CustomValidationError` / `HttpException`。
        class_name.ends_with("Error") || class_name.ends_with("Exception")
    }
}

impl Plugin for NodeErrorPlugin {
    /// 返回插件的唯一标识名称，用于日志记录和监控。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回插件的执行优先级。数值越小，执行调度越靠前。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 分析输入的文本切片，检测是否符合当前插件的处理特征，并返回一个 0.0 到 1.0 的置信度（Confidence）。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        let lines: Vec<&str> = text.lines().take(10).collect();

        if lines.is_empty() {
            return None;
        }

        let mut match_count = 0;
        let mut has_header = false;

        for line in &lines {
            if self.exception_pattern.is_match(line) {
                has_header = true;
                match_count += 1;
            } else if self.frame_pattern.is_match(line) {
                match_count += 1;
            }
        }

        if has_header && match_count > 1 {
            return Some(0.8);
        }

        let ratio = match_count as f32 / lines.len() as f32;
        if ratio >= 0.4 {
            Some(ratio)
        } else {
            None
        }
    }

    /// 执行核心的压缩与特征提取逻辑。将输入文本中的重复长字符串、路径、包名等转换为紧凑的 Token，并存入字典引擎。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let mut tokens = Vec::new();
        let text = slice.text.as_ref();

        for line in text.lines() {
            if let Some(caps) = self.exception_pattern.captures(line) {
                let class_name = caps.name("class").unwrap().as_str();
                let msg = caps.name("msg").unwrap().as_str();

                // 法则 D 防失忆：异常类名必须以字面量进入 compact；
                // 只有非白名单的类名才允许字典化。
                let class_token = if Self::should_preserve_class_name(class_name) {
                    class_name.to_string()
                } else {
                    dict_engine.add_package(class_name)
                };
                let encoded = format!("$ND|EX|{}|{}", class_token, msg);
                tokens.push(Token::Text(encoded.into()));
            } else if let Some(caps) = self.frame_pattern.captures(line) {
                let func = caps.name("func").unwrap().as_str();
                let file = caps.name("file").unwrap().as_str();
                let line_num = caps.name("line").map_or("", |m| m.as_str());
                let col = caps.name("col").map_or("", |m| m.as_str());

                let file_token = if file == "<anonymous>" {
                    file.to_string()
                } else {
                    dict_engine.add_path_layered(file)
                };

                let indent = line
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .collect::<String>();
                let indent_token = if indent == "    " {
                    "4".to_string()
                } else {
                    format!("\"{}\"", indent)
                };

                let encoded = if line_num.is_empty() {
                    format!("$ND|FL|{}|{}|{}", indent_token, func, file_token)
                } else {
                    format!(
                        "$ND|FL|{}|{}|{}|{}|{}",
                        indent_token, func, file_token, line_num, col
                    )
                };
                tokens.push(Token::Text(encoded.into()));
            } else {
                tokens.push(Token::Text(line.to_string().into()));
            }
            tokens.push(Token::Text("\n".into()));
        }

        // 法则 A ROI 门控：若压缩后体积反而变大（典型小样本 / 单行异常场景），
        // 回退原文整段直接透传，避免违反「不增反降」约束。
        // 参考 `docs/prompts/non_vcs_classical_prompts.md` § 1.3。
        let compacted: String = tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.as_ref(),
                _ => "",
            })
            .collect();
        let final_text = crate::core::utils::roi::prefer_non_expanding(text, compacted);

        CompressResult {
            tokens: vec![Token::Text(final_text.into())],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 执行反向的还原逻辑。利用字典引擎中存储的上下文，将压缩后的 Token 流重新展开为完整、人类可读的原始文本。
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        let mut result = String::new();

        for line in compressed.lines() {
            if line.starts_with("$ND|EX|") {
                let parts: Vec<&str> = line.splitn(4, '|').collect();
                if parts.len() == 4 {
                    let class_token = parts[2];
                    let msg = parts[3];
                    let class_name = dict.resolve_or_self(class_token);
                    result.push_str(&format!("{}: {}\n", class_name, msg));
                    continue;
                }
            } else if line.starts_with("$ND|FL|") {
                let parts: Vec<&str> = line.splitn(7, '|').collect();
                if parts.len() >= 5 {
                    let indent_val = parts[2];
                    let indent = if indent_val == "4" {
                        "    ".to_string()
                    } else {
                        indent_val.trim_matches('"').to_string()
                    };

                    let func = parts[3];
                    let file_token = parts[4];
                    let file = if file_token == "<anonymous>" {
                        file_token.to_string()
                    } else {
                        dict.resolve_or_self(file_token)
                    };

                    if parts.len() == 7 {
                        let line_num = parts[5];
                        let col = parts[6];
                        result.push_str(&format!(
                            "{}at {} ({}:{}:{})\n",
                            indent, func, file, line_num, col
                        ));
                    } else {
                        result.push_str(&format!("{}at {} ({})\n", indent, func, file));
                    }
                    continue;
                }
            }

            result.push_str(line);
            result.push('\n');
        }

        result
    }

    /// 从外部的配置文件或数据源加载并覆盖当前插件的配置项。
    fn load_config(&mut self, config: &dyn Any) -> Result<(), String> {
        if let Some(compiled_config) = config.downcast_ref::<CompiledPluginConfig>() {
            let new_plugin = NodeErrorPlugin::with_config(compiled_config.clone());
            self.name = new_plugin.name;
            self.priority = new_plugin.priority;
            self.exception_pattern = new_plugin.exception_pattern;
            self.frame_pattern = new_plugin.frame_pattern;
            self.config = new_plugin.config;
            return Ok(());
        }
        Err("Invalid config type".to_string())
    }

    /// 返回当前插件执行完毕后，推荐调度器优先尝试执行的后续插件列表（构建处理管道）。
    fn next_plugins(&self) -> Vec<&'static str> {
        vec!["smart_path"]
    }
}
