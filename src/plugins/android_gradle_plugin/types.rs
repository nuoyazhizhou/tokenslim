//! Android/Gradle 插件类型定义

use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::borrow::Cow;
use std::sync::Arc;

/// Android/Gradle 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AndroidGradleConfig {
    pub aggregate_resource_warnings: bool,
    pub fold_tasks: bool,
}

impl Default for AndroidGradleConfig {
    fn default() -> Self {
        Self {
            aggregate_resource_warnings: true,
            fold_tasks: true,
        }
    }
}

/// Android/Gradle 构建日志 analysis 插件
pub struct AndroidGradlePlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) config: AndroidGradleConfig,
    pub(crate) task_pattern: Arc<Regex>,
}

impl AndroidGradlePlugin {
    pub fn new() -> Self {
        Self {
            name: "android_gradle",
            priority: 80,
            config: AndroidGradleConfig::default(),
            task_pattern: Arc::new(Regex::new(r"(:[\w:]+:\w+)").unwrap()),
        }
    }
}

impl Plugin for AndroidGradlePlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }

    fn detect<'a>(&self, slice: &Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        if text.contains("Task :") || text.contains("android") || text.contains("gradle") {
            return Some(0.8);
        }
        None
    }

    fn compress<'a>(
        &self,
        slice: &Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();
        let optimized = self.optimize_gradle_tasks(text, dict_engine);
        let res_optimized = self.optimize_resource_warnings(&optimized, dict_engine, arena);
        let gradle_optimized = self.optimize_generic_gradle(&res_optimized);
        let final_text = self.optimize_jenkins_env(&gradle_optimized, dict_engine);

        // 法则 A ROI 门控：优化结果若反而扩张（case_012_gradle_no_compress 类场景），
        // 回退原文。参考 `docs/prompts/non_vcs_classical_prompts.md` § A.2.5。
        let final_text = crate::core::utils::roi::prefer_non_expanding(text, final_text);

        CompressResult {
            tokens: vec![crate::core::compression::Token::Text(Cow::Owned(
                final_text,
            ))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    fn load_config(&mut self, config: &dyn Any) -> Result<(), String> {
        if let Some(c) = config.downcast_ref::<AndroidGradleConfig>() {
            self.config = c.clone();
            return Ok(());
        }
        Err("Invalid config".to_string())
    }
}

impl Clone for AndroidGradlePlugin {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            priority: self.priority,
            config: self.config.clone(),
            task_pattern: self.task_pattern.clone(),
        }
    }
}
