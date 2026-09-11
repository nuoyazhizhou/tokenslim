//! Protobuf 插件方法实现。

use super::types::ProtobufPlugin;
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
use regex::Regex;
use std::borrow::Cow;
use std::sync::OnceLock;

impl ProtobufPlugin {
    /// 创建 ProtobufPlugin 实例（名称 protobuf，优先级 96）。
    pub fn new() -> Self {
        Self {
            name: "protobuf",
            priority: 96,
        }
    }
}

impl Plugin for ProtobufPlugin {
    /// 返回插件名称 "protobuf"。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回插件优先级 96。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：含 protoc/.proto:/warning: field/type " 等特征得 0.9。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let lower = slice.text.to_ascii_lowercase();
        contains_any(&lower, &["protoc", ".proto:", "warning: field", "type \""]).then_some(0.9)
    }

    /// 压缩切片：剥离 ANSI，压缩 protoc 诊断为摘要，保留错误信号并做 ROI 门控。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let raw = slice.text.as_ref();
        let cleaned = crate::core::utils::strip_ansi(raw);
        let compacted = keep_error_signal(&cleaned, compact_protobuf(&cleaned, dict_engine));
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

/// 核心压缩逻辑：解析 .proto 诊断行（error/warning），统计计数并保留诊断详情，
/// 无压缩收益时回退原文；针对生成/描述符/版本场景输出单行摘要。
#[tracing::instrument(level = "debug", skip_all)]
fn compact_protobuf(text: &str, dict_engine: &mut DictionaryEngine) -> String {
    static DIAG_RE: OnceLock<Regex> = OnceLock::new();
    let diag_re = DIAG_RE.get_or_init(|| {
        Regex::new(r"^([^:\s]+\.proto):(\d+):(?:(\d+):)?\s*(warning|error):\s*(.+)$").unwrap()
    });
    let mut lines = Vec::new();
    push_anchor(&mut lines, text);
    let mut errors = 0usize;
    let mut warnings = 0usize;
    let mut diagnostics = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(caps) = diag_re.captures(trimmed) {
            let file = dict_engine.add_path_layered(caps.get(1).unwrap().as_str());
            let level = caps.get(4).unwrap().as_str();
            let message = compact_spaces(caps.get(5).unwrap().as_str());
            if level == "error" {
                errors += 1;
                diagnostics.push(format!("error {file}:{}: {message}", &caps[2]));
            } else {
                warnings += 1;
                if diagnostics.len() < 6 {
                    diagnostics.push(format!("warning {file}:{}: {message}", &caps[2]));
                }
            }
        } else if is_error_line(trimmed) {
            diagnostics.push(trimmed.to_string());
        }
    }

    if errors + warnings > 0 {
        lines.push(format!("PROTOC: {errors} errors, {warnings} warnings"));
    }
    lines.extend(diagnostics);
    if lines.len() <= 1 {
        let generated = text
            .lines()
            .filter(|line| line.starts_with("Generated "))
            .count();
        if generated > 0 {
            lines.push(format!("GENERATED: {generated} files"));
        } else if text.contains("Writing descriptor set") {
            let included = text
                .lines()
                .find_map(|line| line.strip_prefix("Included files: "))
                .unwrap_or("?");
            lines.push(format!("DESCRIPTOR: files={included}"));
        } else if let Some(version) = text.lines().find(|line| line.starts_with("libprotoc ")) {
            lines.push(version.to_string());
        }
    }
    fallback_if_anchor_only(lines, text)
}
