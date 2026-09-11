//! cli 公共逻辑

use crate::cli::types::*;
use crate::core::compression::{CompressionMetadata, CompressionOutput, Token};
use crate::core::compression_context::CompressionContext;
use crate::core::compression_pipeline::{CompressionPipeline, PipelineConfig};
use crate::core::dedup_engine::{DedupConfig, DedupEngine};
use crate::core::dictionary_engine::DictionaryEngine;
use crate::core::metrics::{MetricsCollector, MetricsConfig};
use crate::core::path_optimizer::methods::{
    optimize_path_dictionary_blocks_with_options, PathDictionaryOptions,
};
use crate::core::path_optimizer::token_boundary::{
    is_path_token_boundary_next, replace_path_token_boundary,
};
use crate::core::plugin_config_loader::{self, RunRouteCapability};
use crate::core::plugin_dispatcher::Plugin;
use crate::core::text_slicer::{Slice, SliceFlags, SliceType};
use crate::utils::i18n::{render_user_facing_terminal_message, t, t1, t2, UserFacingMessage};
use bumpalo::Bump;
use serde::Serialize;
use serde_json::Value;
use std::borrow::Cow;
use std::io::{self, IsTerminal, Read};

/// 构造面向用户的非法参数提示消息：渲染多语言(zh/en)正文与可选 hint 为终端可读文本。
pub(crate) fn format_invalid_args_message(
    code: &'static str,
    zh: impl Into<String>,
    en: impl Into<String>,
    hint_zh: Option<String>,
    hint_en: Option<String>,
) -> String {
    render_user_facing_terminal_message(UserFacingMessage {
        code,
        message_zh: zh.into(),
        message_en: en.into(),
        hint_zh,
        hint_en,
    })
}

/// 计算压缩输出的输入与输出字节数：优先用 compressed_size，为 0 时按 token 估算累加。
pub(crate) fn tracking_bytes(output: &CompressionOutput) -> (usize, usize) {
    let input_bytes = output.metadata.original_size;
    let output_bytes = if output.metadata.compressed_size > 0 {
        output.metadata.compressed_size
    } else {
        output.tokens.iter().map(|t| t.estimated_size()).sum()
    };
    (input_bytes, output_bytes)
}

/// 负收益守门（审计数据驱动优化，2026-09-10 登记 P2-89）。
///
/// 审计数据（1663 条 compression.jsonl 实测）显示：小输入的字典标记开销可使压缩产物
/// 大于原文——153 条 byte_ratio>1.0，其中 42 条 token 维度同步变差，最差 `git diff
/// --cached --name-only` 357B→381B（+6.7%）。压缩的意义是「不比原文更差」，故当
/// 产物估算字节不小于原文字节时回退为原文透传（单 `Token::Text`），metadata 诚实
/// 记录为等尺寸/等 token、零收益。
///
/// 返回 `(output, guard_triggered)`；流式分块路径不适用（跨块合并后再判，调用方自行取舍）。
pub(crate) fn guard_negative_savings(
    output: CompressionOutput,
    original_text: &str,
) -> (CompressionOutput, bool) {
    let out_bytes: usize = output.tokens.iter().map(|t| t.estimated_size()).sum();
    if out_bytes < original_text.len() || original_text.is_empty() {
        return (output, false);
    }
    let mut metadata = output.metadata;
    let original_size = if metadata.original_size > 0 {
        metadata.original_size
    } else {
        original_text.len()
    };
    let original_tokens = if metadata.original_tokens > 0 {
        metadata.original_tokens
    } else {
        original_text.len().div_ceil(4)
    };
    metadata.compressed_size = original_size;
    metadata.compressed_tokens = original_tokens;
    metadata.token_savings = 0;
    metadata.compression_ratio = 1.0;
    metadata.token_ratio = 1.0;
    let passthrough = CompressionOutput {
        tokens: vec![Token::Text(Cow::Owned(original_text.to_string()))],
        dictionary: output.dictionary,
        metadata,
    };
    (passthrough, true)
}

/// 记录一次命令执行的 tracking 事件：计算输入/输出字节并写入 Tracker，
/// 打开/清理/写入任一环节失败仅告警不中断。
///
/// P2-47：新增 `filter_time_ms` 参数，把压缩真实耗时经
/// `with_filter_time` 写入，避免耗时指标恒 0 的「活代码死数据」。
pub(crate) fn record_tracking_event(
    command: &str,
    filter_name: Option<&str>,
    output: &CompressionOutput,
    exit_code: i32,
    filter_time_ms: i64,
) {
    let (input_bytes, output_bytes) = tracking_bytes(output);
    let tracking_event = crate::core::tracking::TrackingEvent::new(
        command,
        filter_name,
        input_bytes,
        output_bytes,
        exit_code,
    )
    .with_filter_time(filter_time_ms);
    match crate::core::tracking::Tracker::open_default() {
        Ok(tracker) => {
            if let Err(e) = tracker.auto_cleanup() {
                log::warn!("{}", t1("tracking_cleanup_skipped", e));
            }
            if let Err(e) = tracker.record(&tracking_event) {
                log::warn!("{}", t1("tracking_record_failed", e));
            }
        }
        Err(e) => {
            log::warn!("{}", t1("tracking_open_failed", e));
        }
    }
}

/// 判断命令行参数是否包含指定长标志(精确匹配或 `--long=` 前缀形式)。
pub(crate) fn argv_has_long_flag(args: &[String], long: &str) -> bool {
    let eq_prefix = format!("{}=", long);
    args.iter()
        .any(|arg| arg == long || arg.starts_with(&eq_prefix))
}

/// 判断命令行参数是否包含格式标志(-f/--format 及其 = 形式)。
pub(crate) fn argv_has_format_flag(args: &[String]) -> bool {
    args.iter().any(|arg| {
        arg == "-f"
            || arg == "--format"
            || arg.starts_with("--format=")
            || (arg.starts_with("-f") && arg.len() > 2)
    })
}

/// 判断命令行参数是否包含输出标志(-o/--output 及其 = 形式)。
pub(crate) fn argv_has_output_flag(args: &[String]) -> bool {
    args.iter().any(|arg| {
        arg == "-o"
            || arg == "--output"
            || arg.starts_with("--output=")
            || (arg.starts_with("-o") && arg.len() > 2)
    })
}

/// 归一化用于比较的字符串：将 CRLF 转为 LF 并去除尾部空白。
pub(crate) fn normalize_for_compare(s: &str) -> String {
    s.replace("\r\n", "\n").trim_end().to_string()
}

/// 校验压缩实际输出与期望文本是否一致：先归一化(CRLF→LF+去尾空白)再比较，
/// 不一致时返回首个差异字符位置及两侧内容明细。
pub(crate) fn verify_text_pair(actual: &str, expected: &str) -> Result<(), String> {
    let actual = normalize_for_compare(actual);
    let expected = normalize_for_compare(expected);

    if actual == expected {
        return Ok(());
    }

    let prefix_len = actual
        .chars()
        .zip(expected.chars())
        .take_while(|(a, b)| a == b)
        .count();
    Err(format!(
        "[verify] FAIL at char {prefix_len}\n--- expected ---\n{expected}\n--- actual ---\n{actual}"
    ))
}

/// 将 token 序列展平为字符串：递归处理 Text/DictRef/Marker 各变体。
pub(crate) fn flatten_tokens(tokens: &[Token<'_>]) -> String {
    let mut out = String::new();
    for token in tokens {
        match token {
            Token::Text(s) => out.push_str(s.as_ref()),
            Token::DictRef(s) => out.push_str(s.as_ref()),
            Token::Marker { value, .. } => out.push_str(value.as_ref()),
        }
    }
    out
}
