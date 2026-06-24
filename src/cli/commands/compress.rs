//! cli compress 子命令

use crate::cli::app::{render_global_usage, should_show_compress_quick_usage};
use crate::cli::common::*;
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
use serde_json::json;
use std::borrow::Cow;
use std::io::{self, IsTerminal, Read};


pub(crate) fn run_compress_mode(
    args: &CliArgs,
    pipeline: &mut CompressionPipeline,
    launched_without_args: bool,
    program: &str,
) -> Result<(), CliError> {
    let (input_text, is_stdin_input) = read_compress_input(&args.input)?;

    if should_show_compress_quick_usage(launched_without_args, is_stdin_input, &input_text) {
        println!("{}", render_global_usage(program));
        return Ok(());
    }

    let output = pipeline
        .compress_str(&input_text)
        .map_err(|e| CliError::Pipeline(e))?;

    // 统一到 tracking：compress 模式也进入同一统计账本
    record_tracking_event("tokenslim compress", Some("pipeline_compress"), &output, 0);

    let (original_size, compressed_size) = tracking_bytes(&output);
    let stats = json!({
        "original_size": original_size,
        "compressed_size": compressed_size,
    });
    args.emit_serializable(&output, Some(stats))?;
    Ok(())
}


pub(crate) fn read_compress_input(input: &InputSource) -> Result<(String, bool), CliError> {
    match input {
        InputSource::File(path) => {
            let bytes = std::fs::read(path).map_err(CliError::Io)?;
            Ok((String::from_utf8_lossy(&bytes).into_owned(), false))
        }
        InputSource::Stdin => {
            let mut buffer = Vec::new();
            io::stdin().read_to_end(&mut buffer).map_err(CliError::Io)?;
            Ok((String::from_utf8_lossy(&buffer).into_owned(), true))
        }
    }
}



