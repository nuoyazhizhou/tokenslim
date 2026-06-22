//! ansi cleaner plugin 方法实现

/// # 方法概述

/// 本模块实现了 ansi cleaner plugin 模块的主要业务逻辑。
/// 包含所有公共 API 的实现，以及内部辅助函数。
use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_config_loader::CompiledPluginConfig;
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use regex::Regex;
use std::any::Any;
use std::sync::Arc;

impl Default for AnsiCleanerPlugin {
    /// 提供该插件类型的默认配置实现。
    fn default() -> Self {
        Self::new()
    }
}

impl AnsiCleanerPlugin {
    /// 创建一个新的 ANSI 控制码清理插件。
    ///
    /// 默认具有最高优先级（255），确保在其他语义分析插件之前清除视觉噪点。
    pub fn new() -> Self {
        // 匹配标准的 ANSI escape codes, 例如 \x1b[31m
        let ansi_pattern = Regex::new(r"\x1B(?:[@-Z\-_]|\[[0-?]*[ -/]*[@-~])").unwrap();

        AnsiCleanerPlugin {
            name: "ansi_cleaner",
            priority: 255, // 最高优先级，最先运行
            ansi_pattern: Arc::new(ansi_pattern),
            config: None,
        }
    }

    /// 根据外部配置创建一个插件实例。
    pub fn with_config(config: CompiledPluginConfig) -> Self {
        let ansi_pattern = Regex::new(r"\x1B(?:[@-Z\-_]|\[[0-?]*[ -/]*[@-~])").unwrap();

        AnsiCleanerPlugin {
            name: Box::leak(config.name.clone().into_boxed_str()) as &'static str,
            priority: config.priority,
            ansi_pattern: Arc::new(ansi_pattern),
            config: Some(config),
        }
    }
}

impl Plugin for AnsiCleanerPlugin {
    /// 返回插件的唯一标识名称，用于日志记录和监控。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回插件的执行优先级。数值越小，执行调度越靠前。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 探测当前切片是否包含 ANSI 控制码。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        if self.ansi_pattern.is_match(text) {
            // ANSI 是辅助插件，返回较低置信度以便其他更具体的插件也有机会被选中
            Some(0.1)
        } else {
            None
        }
    }

    /// 执行 ANSI 清理和进度条塌陷算法。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a bumpalo::Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();
        let cleaned = self.ansi_pattern.replace_all(text, "");

        // 处理回车符 \r 导致的进度条覆盖。只保留最后一行有效状态。
        let mut final_text = String::new();
        for line in cleaned.lines() {
            if line.contains('\r') {
                let parts: Vec<&str> = line.split('\r').collect();
                if let Some(last) = parts.last() {
                    let trimmed = last.trim();
                    if !trimmed.is_empty() {
                        final_text.push_str(trimmed);
                        final_text.push('\n');
                    }
                }
            } else {
                final_text.push_str(line);
                final_text.push('\n');
            }
        }

        // 法则 D 防失忆 / G4 非空输出守卫：若原文非空但剥完 ANSI 后仅剩空白，
        // 输出一个语义标记让 LLM 知道「这里原本有 ANSI 控制码已被净化」，
        // 避免下游认为该切片是空输入（case_009_ansi_escape_only）。
        if !text.trim().is_empty() && final_text.trim().is_empty() {
            let ansi_bytes = text.len() - final_text.len();
            final_text = format!("[stripped: {} ANSI bytes]\n", ansi_bytes);
        }

        // 法则 A ROI 门控：纯文本无 ANSI 样本在 trim + 行间重构后可能反而扩张
        // （多加一个末尾 \n 或 trim 掉尾空白但多出 1 字节）。
        // 参考 `docs/prompts/non_vcs_classical_prompts.md` § E.2.1。
        let final_text = crate::core::utils::roi::prefer_non_expanding(text, final_text);

        CompressResult {
            tokens: vec![Token::Text(final_text.into())],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 还原经过清理后的文本（由于 ANSI 清理是不可逆的，此处原样返回）。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    /// 从外部的配置文件或数据源加载并覆盖当前插件的配置项。
    fn load_config(&mut self, config: &dyn Any) -> Result<(), String> {
        if let Some(compiled_config) = config.downcast_ref::<CompiledPluginConfig>() {
            let new_plugin = AnsiCleanerPlugin::with_config(compiled_config.clone());
            self.name = new_plugin.name;
            self.priority = new_plugin.priority;
            self.ansi_pattern = new_plugin.ansi_pattern;
            self.config = new_plugin.config;
            return Ok(());
        }
        Err("Failed".to_string())
    }

    /// 获取当前插件的内部配置项引用，可用于动态调整插件行为。

    /// 返回当前插件执行完毕后，推荐调度器优先尝试执行的后续插件列表（构建处理管道）。
    fn next_plugins(&self) -> Vec<&'static str> {
        vec!["smart_path"]
    }

    /// 对文本进行归一化处理（用于日志比对）。消除时间戳、随机 Hash、乱序参数等 Diff 噪音。
    fn normalize(&self, text: &str) -> String {
        self.ansi_pattern.replace_all(text, "").into_owned()
    }
}
