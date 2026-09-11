//! Terraform 插件方法实现。

use super::types::TerraformPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use crate::plugins::infra_tools_common::{
    contains_any, decompress_with_dict, fallback_if_anchor_only, is_error_line, keep_error_signal,
    push_anchor,
};
use bumpalo::Bump;
use regex::Regex;
use std::borrow::Cow;
use std::sync::OnceLock;

impl TerraformPlugin {
    /// 创建 TerraformPlugin 实例（名称 terraform，优先级 90）。
    pub fn new() -> Self {
        Self {
            name: "terraform",
            priority: 90,
        }
    }
}

impl Plugin for TerraformPlugin {
    /// 返回插件名称 "terraform"。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回插件优先级 90。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：含 terraform will perform/plan/apply 或 Plan: 特征得 0.9。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let lower = slice.text.to_ascii_lowercase();
        contains_any(
            &lower,
            &[
                "terraform will perform",
                "terraform plan",
                "terraform apply",
                "plan:",
            ],
        )
        .then_some(0.9)
    }

    /// 压缩切片：剥离 ANSI，压缩 terraform 计划为摘要，保留错误信号并做 ROI 门控。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let raw = slice.text.as_ref();
        let cleaned = crate::core::utils::strip_ansi(raw);
        let compacted = keep_error_signal(&cleaned, compact_terraform(&cleaned, dict_engine));
        let final_text = crate::core::utils::roi::prefer_non_expanding(raw, compacted);
        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(final_text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 解压：将压缩文本中的字典 token 用词典还原。
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        decompress_with_dict(compressed, dict)
    }
}

/// 核心压缩逻辑：解析资源变更行（# 注释行）与 Plan 汇总，资源名字典化，
/// 无压缩收益时回退原文；针对无变化/导入/工作区/版本场景输出单行摘要。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_terraform(text: &str, dict_engine: &mut DictionaryEngine) -> String {
    static RES_RE: OnceLock<Regex> = OnceLock::new();
    static PLAN_RE: OnceLock<Regex> = OnceLock::new();
    let res_re = RES_RE.get_or_init(|| {
        Regex::new(r#"^\s*#\s+([A-Za-z0-9_./\-\[\]"]+)\s+will be\s+(.+)$"#).unwrap()
    });
    let plan_re = PLAN_RE.get_or_init(|| {
        Regex::new(r"Plan:\s+(\d+)\s+to add,\s+(\d+)\s+to change,\s+(\d+)\s+to destroy").unwrap()
    });

    let mut lines = Vec::new();
    push_anchor(&mut lines, text);
    let mut resources = Vec::new();
    let mut computed = 0usize;
    let mut summary = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(caps) = res_re.captures(trimmed) {
            let token = dict_engine.add_path_layered(caps.get(1).unwrap().as_str());
            let action = terraform_action_code(caps.get(2).unwrap().as_str());
            resources.push(format!("{action} {token}"));
        } else if trimmed.contains("(known after apply)") {
            computed += 1;
        } else if let Some(caps) = plan_re.captures(trimmed) {
            summary = Some(format!("PLAN: +{} ~{} -{}", &caps[1], &caps[2], &caps[3]));
        } else if is_error_line(trimmed) {
            resources.push(trimmed.to_string());
        }
    }

    if let Some(s) = summary {
        lines.push(s);
    }
    lines.extend(resources);
    if computed > 0 {
        lines.push(format!("computed: {computed} attrs"));
    }
    if lines.len() <= 1 {
        let lower = text.to_ascii_lowercase();
        if lower.contains("no changes.") {
            lines.push("ST:[CLEAN]".to_string());
        } else if lower.contains("import successful") {
            lines.push("IMPORT: successful".to_string());
        } else if lower.contains("switched to workspace") {
            if let Some(active) = text.lines().find(|line| line.trim_start().starts_with('*')) {
                lines.push(format!("WS: {}", active.trim()));
            }
        } else if let Some(version) = text.lines().find(|line| line.starts_with("Terraform v")) {
            lines.push(version.to_string());
        }
    }
    fallback_if_anchor_only(lines, text)
}

/// 将资源操作描述映射为动作码（created→A、destroyed→D、updated→M、replaced→R）。
fn terraform_action_code(action: &str) -> &'static str {
    let lower = action.to_ascii_lowercase();
    if lower.contains("created") {
        "A"
    } else if lower.contains("destroyed") {
        "D"
    } else if lower.contains("updated") || lower.contains("changed") {
        "M"
    } else if lower.contains("replaced") {
        "R"
    } else {
        "*"
    }
}
