//! cli decompress 子命令

use crate::cli::common::*;
use crate::cli::get_plugins;
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
use std::borrow::Cow;
use std::io::{self, IsTerminal, Read};


pub(crate) fn run_decompress_mode(args: &CliArgs) -> Result<(), CliError> {
    let input_text = match &args.input {
        InputSource::File(path) => std::fs::read_to_string(path).map_err(|e| CliError::Io(e))?,
        InputSource::Stdin => {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|e| CliError::Io(e))?;
            buffer
        }
    };

    let output: crate::core::compression::CompressionOutput =
        serde_json::from_str(&input_text).map_err(|e| CliError::Serialization(e))?;

    let rehydrator = crate::core::rehydration_pipeline::RehydrationPipeline::new(
        output.dictionary.clone(),
        get_plugins(),
        crate::core::rehydration_pipeline::RehydrationConfig::default(),
    );

    let decompressed = if args.ai_signal {
        rehydrator
            .rehydrate_for_ai(&output)
            .map_err(|e| CliError::Decompression(e.to_string()))?
    } else if args.ai_export {
        let mut result = String::new();
        result.push_str("========== TokenSlim AI Export Context ==========\n");

        // Export Structural Directories for AI Context
        result.push_str("[Directories]\n");
        let mut dirs: Vec<_> = output.dictionary.directories.iter().collect();
        dirs.sort_by_key(|(k, _)| k[2..].parse::<usize>().unwrap_or(0));
        for (k, v) in dirs {
            result.push_str(&format!("{}: {}\n", k, v));
        }

        result.push_str("\n[Semantic Logs]\n");
        let rehydrated = rehydrator
            .rehydrate_for_ai(&output)
            .map_err(|e| CliError::Decompression(e.to_string()))?;
        result.push_str(&rehydrated);

        result
    } else {
        rehydrator
            .rehydrate(&output)
            .map_err(|e| CliError::Decompression(e.to_string()))?
    };

    args.emit_text(&decompressed, None)?;
    Ok(())
}

