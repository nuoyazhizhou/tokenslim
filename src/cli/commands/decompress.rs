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

/// 执行 decompress 子命令：读取已压缩的 JSON 输入(文件或标准输入)，反序列化为 CompressionOutput，
/// 经 RehydrationPipeline 还原压缩时替换的字典/插件宏，并按指定输出格式写回结果。
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

    // P2-63：解压路径注入指标采集器，使 decompress_calls 在真实解压场景非 0。
    // P2-65：`--strict-rehydrate` 时关闭宽松降级（fallback_on_error=false），
    // 解压终态残留字典键 token 将构造 RehydrationError 上报为 CliError::Decompression。
    let mut rehydrator = crate::core::rehydration_pipeline::RehydrationPipeline::new(
        output.dictionary.clone(),
        get_plugins(),
        crate::core::rehydration_pipeline::RehydrationConfig {
            fallback_on_error: !args.strict_rehydrate,
        },
    );
    rehydrator.with_metrics(crate::core::metrics::MetricsCollector::new(
        crate::core::metrics::MetricsConfig::default(),
    ));

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
        dirs.sort_by_key(|(k, _)| {
            // P3-10（D-1）：键形如 `$Dn`，改 trim_start_matches 提取数字，
            // 避免 `k[2..]` 对非 `$D` 前缀键（外部 JSON 消费）panic
            k.trim_start_matches("$D").parse::<usize>().unwrap_or(0)
        });
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

    // P1-08：源编码回写——`--source-encoding-write` 时按产物 metadata 记录的源编码
    // 回写原始字节（而非默认的 UTF-8 文本输出）。裁决口径（2026-09-10）：
    // ① 仅显式开关触发，默认行为不变；② 不可逆编码（utf-8-lossy/mixed-auto/UTF-32）
    // metadata 照记但回写显式报错，绝不静默产出假可逆结果；③ AI 导出/信号模式产物
    // 是加工后的上下文而非原始文本，回写无意义，直接拒绝。
    if args.source_encoding_write {
        if args.ai_export || args.ai_signal {
            return Err(CliError::Decompression(
                "AI 导出/信号模式的产物为加工后上下文，源编码回写不适用".to_string(),
            ));
        }
        match output.metadata.source_encoding.as_deref() {
            None => {
                return Err(CliError::Decompression(
                    "产物 metadata 未记录源编码（输入可能未经 CLI 解码入口或为纯 UTF-8），无源编码可回写"
                        .to_string(),
                ));
            }
            Some(enc) => match crate::core::encoding_fallback::encode_to_source_encoding(
                &decompressed, enc,
            ) {
                Some(bytes) => {
                    args.write_output_bytes(&bytes)?;
                    return Ok(());
                }
                None => {
                    return Err(CliError::Decompression(format!(
                        "源编码 `{enc}` 不可逆回写（有损替换/混合编码/无编码器），拒绝产出可能失真的字节"
                    )));
                }
            },
        }
    }

    args.emit_text(&decompressed, None)?;
    Ok(())
}
