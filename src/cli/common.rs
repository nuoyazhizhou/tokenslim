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


pub(crate) fn tracking_bytes(output: &CompressionOutput) -> (usize, usize) {
    let input_bytes = output.metadata.original_size;
    let output_bytes = if output.metadata.compressed_size > 0 {
        output.metadata.compressed_size
    } else {
        output.tokens.iter().map(|t| t.estimated_size()).sum()
    };
    (input_bytes, output_bytes)
}


pub(crate) fn record_tracking_event(
    command: &str,
    filter_name: Option<&str>,
    output: &CompressionOutput,
    exit_code: i32,
) {
    let (input_bytes, output_bytes) = tracking_bytes(output);
    let tracking_event = crate::core::tracking::TrackingEvent::new(
        command,
        filter_name,
        input_bytes,
        output_bytes,
        exit_code,
    );
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


pub(crate) fn argv_has_long_flag(args: &[String], long: &str) -> bool {
    let eq_prefix = format!("{}=", long);
    args.iter()
        .any(|arg| arg == long || arg.starts_with(&eq_prefix))
}


pub(crate) fn argv_has_format_flag(args: &[String]) -> bool {
    args.iter().any(|arg| {
        arg == "-f"
            || arg == "--format"
            || arg.starts_with("--format=")
            || (arg.starts_with("-f") && arg.len() > 2)
    })
}


pub(crate) fn argv_has_output_flag(args: &[String]) -> bool {
    args.iter().any(|arg| {
        arg == "-o"
            || arg == "--output"
            || arg.starts_with("--output=")
            || (arg.starts_with("-o") && arg.len() > 2)
    })
}


pub(crate) fn normalize_for_compare(s: &str) -> String {
    s.replace("\r\n", "\n").trim_end().to_string()
}


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


pub(crate) fn flatten_tokens(tokens: &[Token<'_>]) -> String {
    let mut out = String::new();
    for token in tokens {
        match token {
            Token::Text(s) => out.push_str(s.as_ref()),
            Token::DictRef(s) => out.push_str(s.as_ref()),
            Token::Marker { value, .. } => out.push_str(value.as_ref()),
            Token::Repeat { token, count } => {
                let inner = flatten_tokens(std::slice::from_ref(token));
                for _ in 0..*count {
                    out.push_str(&inner);
                }
            }
            Token::Diff { base, patch } => {
                out.push_str(base.as_ref());
                out.push_str(patch.as_ref());
            }
        }
    }
    out
}

