//! Bazel 插件方法实现。

use super::types::BazelPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use crate::plugins::infra_tools_common::{
    compact_spaces, contains_any, decompress_with_dict, fallback_if_anchor_only, is_error_line,
    keep_error_signal, push_anchor,
};
use bumpalo::Bump;
use std::borrow::Cow;

impl BazelPlugin {
    /// 创建 BazelPlugin 实例（名称 bazel，优先级 95）。
    pub fn new() -> Self {
        Self {
            name: "bazel",
            priority: 95,
        }
    }
}

impl Plugin for BazelPlugin {
    /// 返回插件名称 "bazel"。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回插件优先级 95。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测切片是否含 bazel build/test 或分析/构建完成信息，命中返回 0.9 置信度。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let lower = slice.text.to_ascii_lowercase();
        contains_any(
            &lower,
            &[
                "bazel build",
                "bazel test",
                "info: analyzed",
                "build completed",
            ],
        )
        .then_some(0.9)
    }

    /// 压缩切片：剥离 ANSI 码，压缩 bazel 日志为摘要行，保留错误信号并做 ROI 门控。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let raw = slice.text.as_ref();
        let cleaned = crate::core::utils::strip_ansi(raw);
        let compacted = keep_error_signal(&cleaned, compact_bazel(&cleaned));
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

/// 核心压缩逻辑：提取版本/query 目标/分析信息/测试结果等关键行，
/// 无压缩收益时回退原文；针对 version/query/sync/clean 场景输出单行摘要。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_bazel(text: &str) -> String {
    let mut lines = Vec::new();
    push_anchor(&mut lines, text);
    let lower_text = text.to_ascii_lowercase();
    if lower_text.contains("bazel version") {
        let label = text
            .lines()
            .find_map(|line| line.trim().strip_prefix("Build label: "));
        let target = text
            .lines()
            .find_map(|line| line.trim().strip_prefix("Build target: "));
        if let Some(label) = label {
            if let Some(target) = target {
                lines.push(format!("VER {label} target={target}"));
            } else {
                lines.push(format!("VER {label}"));
            }
            return fallback_if_anchor_only(lines, text);
        }
    }
    if lower_text.contains("bazel query") {
        let targets = text
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("//"))
            .collect::<Vec<_>>();
        if !targets.is_empty() {
            lines.push(format!(
                "TARGETS[{}]: {}",
                targets.len(),
                targets.join(", ")
            ));
            return fallback_if_anchor_only(lines, text);
        }
    }
    // 进入成功目标输出块（如 `Target //... up-to-date:`）后，连带保留紧随后续的
    // 非空缩进产物路径行，避免零 action 构建丢失目标状态与产物位置（SAP-0071）。
    let mut in_output_block = false;
    for line in text.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        let is_target_header = trimmed.starts_with("Target ")
            && (trimmed.contains(" up-to-date") || trimmed.ends_with(" built"));
        if is_target_header {
            in_output_block = true;
            lines.push(compact_spaces(trimmed));
            continue;
        }
        if in_output_block {
            if trimmed.is_empty() || lower.starts_with("info:") {
                // 输出块到此结束；INFO 汇总行回落普通白名单，避免丢失耗时/完成信息。
                in_output_block = false;
            } else if line.starts_with(char::is_whitespace) {
                lines.push(compact_spaces(trimmed));
                continue;
            } else {
                continue;
            }
        }
        if lower.starts_with("info: analyzed")
            || lower.starts_with("info: found")
            || lower.starts_with("info: elapsed time")
            || lower.starts_with("info: build completed")
            || lower.starts_with("info: build did not complete")
            || lower.contains("processes:")
            || lower.contains("undefined:")
            || lower.starts_with("build label:")
            || lower.starts_with("build target:")
            || (trimmed.starts_with("//")
                && (trimmed.contains(" PASSED ")
                    || trimmed.contains(" FAILED ")
                    || trimmed.ends_with(" PASSED")
                    || trimmed.ends_with(" FAILED")))
            || is_error_line(trimmed)
        {
            lines.push(compact_spaces(trimmed));
        }
    }
    if lines.len() <= 1 {
        let lower = text.to_ascii_lowercase();
        if lower.contains("bazel query") {
            let targets = text.lines().filter(|line| line.starts_with("//")).count();
            lines.push(format!("TARGETS: {targets}"));
        } else if lower.contains("bazel sync") {
            lines.push("SYNC: repositories up-to-date".to_string());
        } else if lower.contains("bazel clean") {
            lines.push("CLEAN: output base removed".to_string());
        } else if let Some(version) = text.lines().find(|line| line.starts_with("Build label:")) {
            lines.push(version.to_string());
        }
    }
    fallback_if_anchor_only(lines, text)
}
