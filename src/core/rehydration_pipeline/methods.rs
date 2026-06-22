//! rehydration pipeline 方法实现

use super::types::*;
use crate::core::compression::{CompressionOutput, Token};
use crate::core::dictionary_engine::Dictionary;
use crate::core::plugin_dispatcher::Plugin;
use std::collections::HashMap;

impl RehydrationPipeline {
    /// 创建一个新的 RehydrationPipeline 实例。
    pub fn new(dict: Dictionary, plugins: Vec<Box<dyn Plugin>>, config: RehydrationConfig) -> Self {
        let mut plugin_map = HashMap::new();
        for plugin in plugins {
            plugin_map.insert(plugin.name().to_string(), plugin);
        }

        Self {
            dict,
            plugins: plugin_map,
            config,
        }
    }

    /// 对压缩输出结果执行完整的还原流程。
    pub fn rehydrate(&self, output: &CompressionOutput) -> Result<String, RehydrationError> {
        let text = self.rehydrate_tokens(&output.tokens)?;

        let mut final_text = text;
        // 1. 插件级还原
        for plugin in self.plugins.values() {
            final_text = plugin.decompress(&final_text, &self.dict);
        }

        // 2. 通用元数据还原
        if final_text.contains("$PL") || final_text.contains("$FL") {
            final_text = final_text.replace("$PL ", "[Pipeline] ");
            final_text = final_text.replace("$PL", "[Pipeline]");
            final_text = self.dict.resolve_recursive(&final_text);
        }

        // 3. v2.0: 处理模糊去重还原 (FUZZY_DUP)
        if final_text.contains("// [FUZZY_DUP]") {
            final_text = self.restore_fuzzy_dups(&final_text);
        }

        Ok(final_text)
    }

    /// 为 AI 消费导出特殊格式的文本（保留路径压缩，内联语义宏，丢弃噪音宏）。
    ///
    /// 结合 **上下文感知行过滤 (Context-Aware Line Filtering)**，去除无关噪声（如常规构建信息），
    /// 同时保留包含 Error/Warning/Fail/Fatal/Exception 的上下文窗口（前后1行）。
    /// 在导出的文本首部还会包含 **Base Timestamp Inclusion** 以便计算相对耗时，
    /// 极大优化了 AI 阅读 Token 消耗。
    pub fn rehydrate_for_ai(&self, output: &CompressionOutput) -> Result<String, RehydrationError> {
        let text = self.rehydrate_tokens_for_ai(&output.tokens)?;

        let mut final_text = text;
        // 1. 插件级还原 (AI模式下依然需要还原非路径的特殊编码)
        for plugin in self.plugins.values() {
            final_text = plugin.decompress(&final_text, &self.dict);
        }

        // 2. 通用元数据还原
        if final_text.contains("$PL") || final_text.contains("$FL") {
            final_text = final_text.replace("$PL ", "[Pipeline] ");
            final_text = final_text.replace("$PL", "[Pipeline]");
            final_text = self.dict.resolve_for_ai(&final_text);
        } else {
            // 对最终合并的文本执行一次 AI 过滤解析
            final_text = self.dict.resolve_for_ai(&final_text);
        }

        // 3. v2.0: 处理模糊去重还原 (FUZZY_DUP)
        if final_text.contains("// [FUZZY_DUP]") {
            final_text = self.restore_fuzzy_dups_for_ai(&final_text);
        }

        // 4. 终极降维：基于语义的行级降噪 (Context-Aware Line Filtering)
        final_text = Self::filter_semantic_lines_for_ai(&final_text);

        // 添加 Base Timestamp 说明
        if let Some(ts) = &output.metadata.base_timestamp {
            let prefix = format!("Note: [T+Xms] are relative to Base Timestamp {}\n\n", ts);
            final_text.insert_str(0, &prefix);
        }

        Ok(final_text)
    }

    fn filter_semantic_lines_for_ai(text: &str) -> String {
        let lines: Vec<&str> = text.lines().collect();
        let mut keep = vec![false; lines.len()];

        for (i, line) in lines.iter().enumerate() {
            let lower = line.to_lowercase();

            let is_context_trigger = lower.contains("error")
                || lower.contains("warning")
                || lower.contains("fail")
                || lower.contains("fatal")
                || lower.contains("exception");

            let is_self_only = line.starts_with("==========")
                || line.starts_with("[Directories]")
                || line.starts_with("[Semantic Logs]")
                || line.starts_with("Note: [T+")
                || line.contains("[TokenSlim AI Mode:")
                || (line.starts_with("$D") && line.contains(": "));

            let is_metadata_line = lower.contains("branch")
                || lower.contains("commit")
                || lower.contains("exit code")
                || lower.contains("return code")
                || lower.contains("duration")
                || lower.contains("elapsed")
                || lower.contains("cost time")
                || lower.contains("耗时");

            if is_context_trigger {
                if i > 0 {
                    keep[i - 1] = true;
                }
                keep[i] = true;
                if i + 1 < lines.len() {
                    keep[i + 1] = true;
                }
            } else if is_self_only || is_metadata_line {
                keep[i] = true;
            }
        }

        let mut result = String::with_capacity(text.len() / 4);
        let mut skip_count = 0;

        for (i, line) in lines.iter().enumerate() {
            if keep[i] {
                if skip_count > 0 {
                    result.push_str(&format!(
                        "... [TokenSlim AI Mode: Skipped {} normal build lines] ...\n",
                        skip_count
                    ));
                    skip_count = 0;
                }
                result.push_str(line);
                result.push('\n');
            } else {
                skip_count += 1;
            }
        }

        if skip_count > 0 {
            result.push_str(&format!(
                "... [TokenSlim AI Mode: Skipped {} normal build lines] ...\n",
                skip_count
            ));
        }

        result
    }

    pub fn rehydrate_tokens_for_ai<'a>(
        &self,
        tokens: &[Token<'a>],
    ) -> Result<String, RehydrationError> {
        let mut result = String::new();
        for token in tokens {
            match token {
                Token::Text(text) => result.push_str(text),
                Token::DictRef(dict_ref) => {
                    result.push_str(&self.dict.resolve_for_ai(dict_ref));
                }
                Token::Repeat { token, count } => {
                    let repeated_text = self.rehydrate_tokens_for_ai(&[*token.clone()])?;
                    for _ in 0..*count {
                        result.push_str(&repeated_text);
                    }
                }
                Token::Marker { kind: _, value } => {
                    result.push_str(value);
                }
                Token::Diff { base, patch } => {
                    let base_val = self.dict.resolve_for_ai(base);
                    result.push_str(&self.apply_patch(&base_val, patch));
                }
            }
        }
        Ok(result)
    }

    pub fn rehydrate_tokens<'a>(&self, tokens: &[Token<'a>]) -> Result<String, RehydrationError> {
        let mut result = String::new();
        for token in tokens {
            match token {
                Token::Text(text) => result.push_str(text),
                Token::DictRef(dict_ref) => {
                    result.push_str(&self.dict.resolve_recursive(dict_ref));
                }
                Token::Repeat { token, count } => {
                    let repeated_text = self.rehydrate_tokens(&[*token.clone()])?;
                    for _ in 0..*count {
                        result.push_str(&repeated_text);
                    }
                }
                Token::Marker { kind: _, value } => {
                    result.push_str(value);
                }
                Token::Diff { base, patch } => {
                    let base_val = self.dict.resolve_recursive(base);
                    result.push_str(&self.apply_patch(&base_val, patch));
                }
            }
        }
        Ok(result)
    }

    fn restore_fuzzy_dups_for_ai(&self, text: &str) -> String {
        let mut result = String::with_capacity(text.len());
        for line in text.lines() {
            if line.contains("// [FUZZY_DUP]") {
                if let Some(pos) = line.find("// [FUZZY_DUP]") {
                    let meta = &line[pos + 14..].trim();
                    let parts: Vec<&str> = meta.split(", ").collect();
                    let mut base_val = String::new();
                    let mut patch = "";

                    for p in parts {
                        if p.starts_with("base=") {
                            let token = &p[5..];
                            base_val = self.dict.resolve_for_ai(token);
                        } else if p.starts_with("patch=") {
                            patch = &p[6..];
                        }
                    }

                    if !base_val.is_empty() {
                        result.push_str(&self.apply_patch(&base_val, patch));
                        result.push('\n');
                        continue;
                    }
                }
            }
            result.push_str(line);
            result.push('\n');
        }
        result
    }

    fn restore_fuzzy_dups(&self, text: &str) -> String {
        let mut result = String::with_capacity(text.len());
        for line in text.lines() {
            if line.contains("// [FUZZY_DUP]") {
                if let Some(pos) = line.find("// [FUZZY_DUP]") {
                    let meta = &line[pos + 14..].trim();
                    let parts: Vec<&str> = meta.split(", ").collect();
                    let mut base_val = String::new();
                    let mut patch = "";

                    for p in parts {
                        if p.starts_with("base=") {
                            let token = &p[5..];
                            base_val = self.dict.resolve_recursive(token);
                        } else if p.starts_with("patch=") {
                            patch = &p[6..];
                        }
                    }

                    if !base_val.is_empty() {
                        result.push_str(&self.apply_patch(&base_val, patch));
                        result.push('\n');
                        continue;
                    }
                }
            }
            result.push_str(line);
            result.push('\n');
        }
        result
    }

    fn apply_patch(&self, base: &str, patch: &str) -> String {
        if patch.is_empty() {
            return base.to_string();
        }
        let mut words: Vec<String> = base.split_whitespace().map(|s| s.to_string()).collect();
        for p in patch.split(',') {
            let sub_parts: Vec<&str> = p.splitn(2, ':').collect();
            if sub_parts.len() == 2 {
                let idx: usize = sub_parts[0].parse().unwrap_or(9999);
                let change: Vec<&str> = sub_parts[1].split("->").collect();
                if change.len() == 2 && idx < words.len() {
                    words[idx] = change[1].to_string();
                }
            }
        }
        words.join(" ")
    }
}
