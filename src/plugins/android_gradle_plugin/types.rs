//! Android/Gradle 插件类型定义

use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use regex::Regex;
use std::borrow::Cow;
use std::sync::Arc;

/// Android/Gradle 构建日志 analysis 插件
pub struct AndroidGradlePlugin {
    pub(crate) name: &'static str,
    pub(crate) priority: u8,
    pub(crate) task_pattern: Arc<Regex>,
}

impl AndroidGradlePlugin {
    /// 创建 Android/Gradle 插件实例（标识名 `android_gradle`，优先级 80，含任务名正则）。
    pub fn new() -> Self {
        Self {
            name: "android_gradle",
            priority: 80,
            task_pattern: Arc::new(Regex::new(r"(:[\w:]+:\w+)").unwrap()),
        }
    }
}

impl Plugin for AndroidGradlePlugin {
    /// 返回插件标识名（来自实例字段）。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级（来自实例字段）。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测文本是否含 `Task :`/`android`/`gradle` 关键字，命中返回 0.8 置信度。
    fn detect<'a>(&self, slice: &Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();
        if text.contains("Task :") || text.contains("android") || text.contains("gradle") {
            return Some(0.8);
        }
        None
    }

    /// 依次执行资源告警聚合、通用 Gradle 压缩与环境变量折叠，并经 ROI 门控回退避免扩张。
    fn compress<'a>(
        &self,
        slice: &Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();
        let res_optimized = self.optimize_resource_warnings(text, dict_engine, arena);
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

    /// 解压占位实现：Gradle 压缩为可逆文本重写，直接返回原串。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }
}

impl Clone for AndroidGradlePlugin {
    /// 克隆插件实例，复制标识、优先级与任务名正则（正则 `Arc` 共享）。
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            priority: self.priority,
            task_pattern: self.task_pattern.clone(),
        }
    }
}
