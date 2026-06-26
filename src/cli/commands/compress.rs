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
    if args.stream {
        return run_compress_stream_mode(args, pipeline);
    }

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


pub(crate) fn run_compress_stream_mode(
    args: &CliArgs,
    pipeline: &mut CompressionPipeline,
) -> Result<(), CliError> {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    // 如果是文件输出且非合并模式，先清空/截断文件
    if !args.merge {
        if let OutputTarget::File(path) = &args.output {
            std::fs::File::create(path).map_err(CliError::Io)?;
        }
    }

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        let mut stdin = io::stdin();
        let mut buf = [0u8; 8192];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let flush_interval = Duration::from_millis(args.flush_interval);
    let mut pending_bytes = Vec::new();
    let mut chunk_text = String::new();
    let mut chunk_outputs = Vec::new();

    loop {
        let msg = rx.recv_timeout(flush_interval);
        match msg {
            Ok(bytes) => {
                pending_bytes.extend(bytes);
                if let Some(last_nl) = pending_bytes.iter().rposition(|&b| b == b'\n') {
                    let complete_part = &pending_bytes[..=last_nl];
                    let complete_str = String::from_utf8_lossy(complete_part);
                    chunk_text.push_str(&complete_str);
                    pending_bytes = pending_bytes[last_nl + 1..].to_vec();
                }

                if chunk_text.len() >= 64 * 1024 {
                    flush_chunk(&chunk_text, pipeline, args, &mut chunk_outputs)?;
                    chunk_text.clear();
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !chunk_text.is_empty() {
                    flush_chunk(&chunk_text, pipeline, args, &mut chunk_outputs)?;
                    chunk_text.clear();
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if !pending_bytes.is_empty() {
                    let remaining_str = String::from_utf8_lossy(&pending_bytes);
                    chunk_text.push_str(&remaining_str);
                    pending_bytes.clear();
                }
                if !chunk_text.is_empty() {
                    flush_chunk(&chunk_text, pipeline, args, &mut chunk_outputs)?;
                    chunk_text.clear();
                }
                break;
            }
        }
    }

    if args.merge {
        if let Some(merged) = merge_compression_outputs(chunk_outputs) {
            let (original_size, compressed_size) = tracking_bytes(&merged);
            let stats = json!({
                "original_size": original_size,
                "compressed_size": compressed_size,
            });
            args.emit_serializable(&merged, Some(stats))?;
        }
    }

    Ok(())
}


fn flush_chunk(
    text: &str,
    pipeline: &mut CompressionPipeline,
    args: &CliArgs,
    chunk_outputs: &mut Vec<CompressionOutput>,
) -> Result<(), CliError> {
    let output = pipeline.compress_str(text).map_err(CliError::Pipeline)?;
    record_tracking_event("tokenslim compress stream", Some("pipeline_compress_stream"), &output, 0);

    if args.merge {
        chunk_outputs.push(output);
    } else {
        let json_str = serde_json::to_string(&output)?;
        if args.json {
            let mut obj = serde_json::Map::new();
            obj.insert("status".to_string(), "success".into());
            obj.insert("data".to_string(), serde_json::to_value(&output)?);
            write_stream_chunk_output(args, &serde_json::to_string(&obj)?)?;
        } else {
            write_stream_chunk_output(args, &json_str)?;
        }
    }
    Ok(())
}


fn write_stream_chunk_output(args: &CliArgs, payload: &str) -> Result<(), CliError> {
    match &args.output {
        OutputTarget::File(path) => {
            use std::fs::OpenOptions;
            use std::io::Write;
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(CliError::Io)?;
            writeln!(file, "{}", payload).map_err(CliError::Io)?;
            Ok(())
        }
        OutputTarget::Stdout => {
            println!("{}", payload);
            Ok(())
        }
    }
}


pub(crate) fn merge_compression_outputs(outputs: Vec<CompressionOutput>) -> Option<CompressionOutput> {
    if outputs.is_empty() {
        return None;
    }

    let mut merged_tokens = Vec::new();
    let mut merged_dict = crate::core::dictionary_engine::Dictionary::default();
    let mut original_size = 0;
    let mut compressed_size = 0;
    let mut original_tokens = 0;
    let mut compressed_tokens = 0;
    let mut slice_count = 0;
    let mut processing_time_ms = 0;
    let mut base_timestamp = None;
    let mut order_info = None;

    for out in outputs {
        merged_tokens.extend(out.tokens);

        merged_dict.paths.extend(out.dictionary.paths);
        merged_dict.packages.extend(out.dictionary.packages);
        merged_dict.macros.extend(out.dictionary.macros);
        merged_dict.files.extend(out.dictionary.files);
        merged_dict.directories.extend(out.dictionary.directories);
        merged_dict.flags.extend(out.dictionary.flags);
        merged_dict.aliases.extend(out.dictionary.aliases);
        for (k, v) in out.dictionary.custom {
            merged_dict.custom.entry(k).or_default().extend(v);
        }

        original_size += out.metadata.original_size;
        compressed_size += out.metadata.compressed_size;
        original_tokens += out.metadata.original_tokens;
        compressed_tokens += out.metadata.compressed_tokens;
        slice_count += out.metadata.slice_count;
        processing_time_ms += out.metadata.processing_time_ms;

        if base_timestamp.is_none() {
            base_timestamp = out.metadata.base_timestamp;
        }
        if order_info.is_none() {
            order_info = out.metadata.order_info;
        }
    }

    let token_savings = if original_tokens > compressed_tokens {
        original_tokens - compressed_tokens
    } else {
        0
    };

    let compression_ratio = if original_size > 0 {
        compressed_size as f32 / original_size as f32
    } else {
        1.0
    };

    let token_ratio = if original_tokens > 0 {
        compressed_tokens as f32 / original_tokens as f32
    } else {
        1.0
    };

    let metadata = CompressionMetadata {
        original_size,
        compressed_size,
        original_tokens,
        compressed_tokens,
        token_savings,
        compression_ratio,
        token_ratio,
        slice_count,
        processing_time_ms,
        order_info,
        base_timestamp,
    };

    Some(CompressionOutput {
        tokens: merged_tokens,
        dictionary: merged_dict,
        metadata,
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::compression::{CompressionMetadata, CompressionOutput};
    use crate::core::dictionary_engine::Dictionary;

    #[test]
    fn test_merge_compression_outputs_empty() {
        assert!(merge_compression_outputs(vec![]).is_none());
    }

    #[test]
    fn test_merge_compression_outputs_valid() {
        let mut dict1 = Dictionary::default();
        dict1.paths.insert("p1".to_string(), "v1".to_string());
        let out1 = CompressionOutput {
            tokens: vec![],
            dictionary: dict1,
            metadata: CompressionMetadata {
                original_size: 100,
                compressed_size: 30,
                original_tokens: 50,
                compressed_tokens: 15,
                slice_count: 1,
                processing_time_ms: 10,
                ..Default::default()
            },
        };

        let mut dict2 = Dictionary::default();
        dict2.paths.insert("p2".to_string(), "v2".to_string());
        let out2 = CompressionOutput {
            tokens: vec![],
            dictionary: dict2,
            metadata: CompressionMetadata {
                original_size: 200,
                compressed_size: 60,
                original_tokens: 100,
                compressed_tokens: 30,
                slice_count: 2,
                processing_time_ms: 20,
                ..Default::default()
            },
        };

        let merged = merge_compression_outputs(vec![out1, out2]).unwrap();
        assert_eq!(merged.metadata.original_size, 300);
        assert_eq!(merged.metadata.compressed_size, 90);
        assert_eq!(merged.metadata.original_tokens, 150);
        assert_eq!(merged.metadata.compressed_tokens, 45);
        assert_eq!(merged.metadata.token_savings, 105);
        assert_eq!(merged.metadata.slice_count, 3);
        assert_eq!(merged.metadata.processing_time_ms, 30);
        assert_eq!(merged.dictionary.paths.get("p1").unwrap(), "v1");
        assert_eq!(merged.dictionary.paths.get("p2").unwrap(), "v2");
    }
}



