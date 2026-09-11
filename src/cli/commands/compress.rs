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

/// 执行 compress 子命令：非流式时读取输入并经管道压缩，记录 tracking 后输出压缩结果与尺寸统计；
/// 当无参数启动且输入为空时仅打印全局用法(quick usage)。
pub(crate) fn run_compress_mode(
    args: &CliArgs,
    pipeline: &mut CompressionPipeline,
    launched_without_args: bool,
    program: &str,
) -> Result<(), CliError> {
    if args.stream {
        return run_compress_stream_mode(args, pipeline);
    }

    let (input_text, is_stdin_input, decoded_lossy, source_enc) = read_compress_input(&args.input)?;

    // P2-08：真 lossy（无可用解码候选，字节被 U+FFFD 替换）不再静默——记录指标并给用户明确提示。
    if decoded_lossy {
        let msg =
            "非 UTF-8 输入无可用解码候选，已退回 lossy 替换（U+FFFD）——部分原始字节被不可逆降级";
        log::warn!("{}", msg);
        pipeline.record_encoding_lossy(msg);
    }

    if should_show_compress_quick_usage(launched_without_args, is_stdin_input, &input_text) {
        println!("{}", render_global_usage(program));
        return Ok(());
    }

    // P2-47：压缩前起表，压缩耗时经 with_filter_time 写入 tracking（修复耗时恒 0）。
    let start = std::time::Instant::now();
    let mut output = pipeline
        .compress_str(&input_text)
        .map_err(|e| CliError::Pipeline(e))?;
    let filter_time_ms = start.elapsed().as_millis() as i64;

    // P1-08：源编码随产物 metadata 落盘。UTF-8 输入不写入——
    // `skip_serializing_if` 使产物字节与历史完全一致（冻结基线零漂移）。
    // P2-89 负收益守门：产物不小于原文时回退原文透传，绝不产出比原文更大的压缩结果。
    let (mut output, guarded) = guard_negative_savings(output, &input_text);
    if guarded {
        eprintln!("{}", t("cli_warn_no_compress_gain"));
    }
    if source_enc != "utf-8" {
        output.metadata.source_encoding = Some(source_enc.to_string());
    }

    // 统一到 tracking：compress 模式也进入同一统计账本
    record_tracking_event(
        "tokenslim compress",
        Some("pipeline_compress"),
        &output,
        0,
        filter_time_ms,
    );

    let (original_size, compressed_size) = tracking_bytes(&output);
    let stats = json!({
        "original_size": original_size,
        "compressed_size": compressed_size,
    });
    args.emit_serializable(&output, Some(stats))?;

    // P2-64：ModuleTiming snapshot 输出接线——主 CLI 之前全程打点后 `snapshot()` 无人调用，
    // 用户看不到模块耗时/插件统计（数据采完即弃）。此处仅当指标启用且设了 `TS_MON_VERBOSE`
    // 才 dump 到 stderr，不污染 stdout 的 JSON 输出面。
    dump_pipeline_metrics_if_verbose(pipeline);
    Ok(())
}

/// P2-64：在压缩收尾处把 `MetricsCollector::snapshot()` 的模块耗时/插件统计渲染到 stderr。
/// 门控双重：`MetricsConfig.enabled` 与 `TS_MON_VERBOSE` 环境变量同时满足才输出。
fn dump_pipeline_metrics_if_verbose(pipeline: &CompressionPipeline) {
    if std::env::var("TS_MON_VERBOSE").is_err() {
        return;
    }
    let metrics = pipeline.get_metrics();
    if !metrics.config.enabled {
        return;
    }
    let s = metrics.snapshot();
    eprintln!(
        "[tokenslim metrics] input={}B output={}B ratio={:.3} slices={} elapsed={:?}",
        s.total_input_size,
        s.total_output_size,
        s.compression_ratio,
        s.slice_count,
        s.processing_time
    );
    let m = &s.module_timings;
    eprintln!("[tokenslim metrics] module(ms): pipeline={:.1} dispatcher={:.1} rehydrate={:.1} slicer={:.1} analyzer={:.1} stream={:.1} dict={:.1} dedup={:.1}",
        m.compression_pipeline.as_secs_f64() * 1e3,
        m.plugin_dispatcher.as_secs_f64() * 1e3,
        m.rehydration_pipeline.as_secs_f64() * 1e3,
        m.text_slicer.as_secs_f64() * 1e3,
        m.content_analyzer.as_secs_f64() * 1e3,
        m.stream_reader.as_secs_f64() * 1e3,
        m.dictionary_engine.as_secs_f64() * 1e3,
        m.dedup_engine.as_secs_f64() * 1e3);
    if !s.plugin_stats.is_empty() {
        let mut plugins: Vec<_> = s.plugin_stats.iter().collect();
        plugins.sort_by(|a, b| a.0.cmp(b.0));
        eprintln!("[tokenslim metrics] plugin(calls:detect/compress/decompress, ms, panic/timeout/fallback):");
        for (name, p) in plugins {
            eprintln!(
                "  {}: {}/{}/{} == {:.1}ms panic={} timeout={} fallback={}",
                name,
                p.detect_calls,
                p.compress_calls,
                p.decompress_calls,
                (p.total_detect_time + p.total_compress_time + p.total_decompress_time)
                    .as_secs_f64()
                    * 1e3,
                p.panic_count,
                p.timeout_count,
                p.fallback_count
            );
        }
    }
    // P2-08：暴露运行时降级错误（含编码 lossy 替换）到可观测出口。
    if !s.errors.is_empty() {
        for e in &s.errors {
            eprintln!(
                "[tokenslim metrics] error type={} module={} msg={}",
                e.error_type, e.module, e.message
            );
        }
    }
}

/// 读取压缩输入源：文件按字节读取并转 UTF-8 文本(标记为非 stdin)，
/// 标准输入读取全部字节，返回 (文本, 是否_stdin, 是否真 lossy 降级, 源编码名)。
/// P1-08：源编码名随产物 metadata 落盘（`source_encoding`），供解压侧按原编码回写字节。
pub(crate) fn read_compress_input(
    input: &InputSource,
) -> Result<(String, bool, bool, &'static str), CliError> {
    match input {
        InputSource::File(path) => {
            let bytes = std::fs::read(path).map_err(CliError::Io)?;
            // P1-08：统一解码入口——UTF-8 输入零变化，GBK/UTF-16 等编码
            // 不再被 from_utf8_lossy 替换为 U+FFFD 不可逆损坏。
            let (text, enc) = crate::core::encoding_fallback::decode_with_fallback(&bytes);
            let lossy = enc == "utf-8-lossy";
            Ok((text, false, lossy, enc))
        }
        InputSource::Stdin => {
            let mut buffer = Vec::new();
            io::stdin().read_to_end(&mut buffer).map_err(CliError::Io)?;
            let (text, enc) = crate::core::encoding_fallback::decode_with_fallback(&buffer);
            let lossy = enc == "utf-8-lossy";
            Ok((text, true, lossy, enc))
        }
    }
}

/// 执行 compress 流式模式：从标准输入分块读取(8KB 缓冲)，按行边界攒批，
/// 达到 64KB 阈值或超时/EOF 时 flush 分块；--merge 模式下汇总多块输出并写出统计。
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

    // P2-18：通道改传 `io::Result<Vec<u8>>`——读取线程不再吞掉 stdin 错误
    // （管道中断/权限问题等原被 `Err(_) => break` 静默吞掉，主线程无法区分
    // 「正常 EOF」与「读取失败」）；JoinHandle 保留，收尾 join 检查线程 panic。
    let (tx, rx) = mpsc::channel::<io::Result<Vec<u8>>>();
    let reader = thread::spawn(move || {
        let mut stdin = io::stdin();
        let mut buf = [0u8; 8192];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    if tx.send(Ok(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                    break;
                }
            }
        }
    });

    let flush_interval = Duration::from_millis(args.flush_interval);
    let mut pending_bytes = Vec::new();
    let mut chunk_text = String::new();
    let mut chunk_outputs = Vec::new();
    // P2-18：读取线程回传的 I/O 错误（None = 正常 EOF）。
    let mut read_error: Option<io::Error> = None;
    // P1-08：跨块源编码收集——首个非 UTF-8 块的编码名；块间不一致时记为
    // "mixed-auto"（与解码子系统同名语义：单一回写必然失真，须显式拒绝）。
    let mut stream_source_enc: Option<String> = None;

    loop {
        let msg = rx.recv_timeout(flush_interval);
        match msg {
            Ok(Ok(bytes)) => {
                pending_bytes.extend(bytes);
                if let Some(last_nl) = pending_bytes.iter().rposition(|&b| b == b'\n') {
                    let complete_part = &pending_bytes[..=last_nl];
                    // P1-08：统一解码入口（同 read_compress_input）。
                    let (complete_str, enc) =
                        crate::core::encoding_fallback::decode_with_fallback(complete_part);
                    collect_stream_source_enc(&mut stream_source_enc, enc);
                    chunk_text.push_str(&complete_str);
                    pending_bytes = pending_bytes[last_nl + 1..].to_vec();
                }

                if chunk_text.len() >= 64 * 1024 {
                    flush_chunk(
                        &chunk_text,
                        pipeline,
                        args,
                        &mut chunk_outputs,
                        stream_source_enc.as_deref(),
                    )?;
                    chunk_text.clear();
                }
            }
            Ok(Err(e)) => {
                // P2-18：读取失败——把已有输入按正常 EOF 口径冲刷出去（尽量
                // 保留部分产出），随后以明确错误 + 非 0 退出码结束。
                log::error!("流式读取 stdin 失败: {e}");
                read_error = Some(e);
                if !pending_bytes.is_empty() {
                    let (remaining_str, enc) =
                        crate::core::encoding_fallback::decode_with_fallback(&pending_bytes);
                    collect_stream_source_enc(&mut stream_source_enc, enc);
                    chunk_text.push_str(&remaining_str);
                    pending_bytes.clear();
                }
                if !chunk_text.is_empty() {
                    flush_chunk(
                        &chunk_text,
                        pipeline,
                        args,
                        &mut chunk_outputs,
                        stream_source_enc.as_deref(),
                    )?;
                    chunk_text.clear();
                }
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !chunk_text.is_empty() {
                    flush_chunk(
                        &chunk_text,
                        pipeline,
                        args,
                        &mut chunk_outputs,
                        stream_source_enc.as_deref(),
                    )?;
                    chunk_text.clear();
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if !pending_bytes.is_empty() {
                    // P1-08：统一解码入口（同 read_compress_input）。
                    let (remaining_str, enc) =
                        crate::core::encoding_fallback::decode_with_fallback(&pending_bytes);
                    collect_stream_source_enc(&mut stream_source_enc, enc);
                    // P2-08：剩余字节无解码候选时记录可观测信号，不再静默。
                    if enc == "utf-8-lossy" {
                        log::warn!("流式剩余输入无可用解码候选字节，已 lossy 替换（U+FFFD）");
                        pipeline.record_encoding_lossy(
                            "流式剩余输入无可用解码候选字节，已 lossy 替换（U+FFFD）",
                        );
                    }
                    chunk_text.push_str(&remaining_str);
                    pending_bytes.clear();
                }
                if !chunk_text.is_empty() {
                    flush_chunk(
                        &chunk_text,
                        pipeline,
                        args,
                        &mut chunk_outputs,
                        stream_source_enc.as_deref(),
                    )?;
                    chunk_text.clear();
                }
                break;
            }
        }
    }

    // P2-18：收尾 join 读取线程——线程内 panic 不再被静默忽略。
    if reader.join().is_err() {
        return Err(CliError::Io(io::Error::other("stdin 读取线程发生 panic")));
    }
    if let Some(e) = read_error {
        return Err(CliError::Io(e));
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

/// P1-08：流式跨块源编码收集——记录首个非 UTF-8 块的编码名；
/// 后续块编码与其不一致时退化为 "mixed-auto"（混合编码，不可逆回写语义）。
fn collect_stream_source_enc(slot: &mut Option<String>, enc: &'static str) {
    if enc == "utf-8" {
        return;
    }
    match slot {
        None => *slot = Some(enc.to_string()),
        Some(prev) if prev != enc => *slot = Some("mixed-auto".to_string()),
        _ => {}
    }
}

/// 压缩并 flush 单个流式分块：调用管道压缩并记入 tracking 事件；
/// 合并模式下入列待汇总，否则按 --json 包装或直接写出该块压缩结果。
/// `source_encoding`：P1-08 跨块源编码（None = 全程 UTF-8，不写入 metadata）。
fn flush_chunk(
    text: &str,
    pipeline: &mut CompressionPipeline,
    args: &CliArgs,
    chunk_outputs: &mut Vec<CompressionOutput>,
    source_encoding: Option<&str>,
) -> Result<(), CliError> {
    // P2-47：压缩前起表，流式分块同样记录真实压缩耗时。
    let start = std::time::Instant::now();
    let mut output = pipeline.compress_str(text).map_err(CliError::Pipeline)?;
    let filter_time_ms = start.elapsed().as_millis() as i64;
    // P2-89 负收益守门：按块保证产物不大于块原文（逐块成立 ⇒ 合并后亦成立）。
    let (mut output, guarded) = guard_negative_savings(output, text);
    if guarded {
        eprintln!("{}", t("cli_warn_no_compress_gain"));
    }
    // P1-08：源编码随产物 metadata 落盘（UTF-8 不写入，冻结基线零漂移）。
    if let Some(enc) = source_encoding {
        output.metadata.source_encoding = Some(enc.to_string());
    }
    record_tracking_event(
        "tokenslim compress stream",
        Some("pipeline_compress_stream"),
        &output,
        0,
        filter_time_ms,
    );

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

/// 写出流式分块负载：按输出目标将单个分块的 JSON 文本追加写入文件(append)，或打印到标准输出。
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

/// 合并多段压缩输出：聚合所有 tokens 与字典(路径/包/宏/文件/目录/标志/别名)，
/// 累加原始/压缩尺寸与 token 统计，重建 CompressionMetadata 并返回合并结果(空列表返回 None)。
/// 拆分词典 token 为 (数字前缀, 尾部编号)；无尾部数字（或整串皆数字/皆非数字）时返回 None。
/// 例：`$P22` → `("$P", 22)`，`$PK3` → `("$PK", 3)`，`$FL` → `None`。
fn split_numbered_token(token: &str) -> Option<(&str, u64)> {
    let bytes = token.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i == 0 || i == bytes.len() {
        return None;
    }
    let num = token[i..].parse::<u64>().ok()?;
    Some((&token[..i], num))
}

/// 取 Map 中同前缀数值型 token 的最大编号；无同族 token 时返回 None。
fn max_numbered_suffix(
    map: &std::collections::HashMap<String, String>,
    prefix: &str,
) -> Option<u64> {
    map.keys()
        .filter_map(|k| {
            split_numbered_token(k)
                .filter(|(p, _)| *p == prefix)
                .map(|(_, n)| n)
        })
        .max()
}

/// 对单个 Token 递归做边界感知的 token 改写（覆盖 Text/DictRef/Marker 全部字符串位）。
fn rewrite_token_strings(token: &mut Token, old: &str, new: &str) {
    match token {
        Token::Text(s) => *s = Cow::Owned(replace_path_token_boundary(s, old, new)),
        Token::DictRef(s) => *s = Cow::Owned(replace_path_token_boundary(s, old, new)),
        Token::Marker { value, .. } => {
            *value = Cow::Owned(replace_path_token_boundary(value, old, new))
        }
    }
}

/// 对六个扁平词典 Map 的全部**值**做边界感知改写（值内可能嵌套引用其他族 token，如 $P 值内嵌 $D 前缀）。
fn rewrite_dictionary_values(
    dict: &mut crate::core::dictionary_engine::Dictionary,
    old: &str,
    new: &str,
) {
    for map in [
        &mut dict.paths,
        &mut dict.packages,
        &mut dict.macros,
        &mut dict.files,
        &mut dict.directories,
        &mut dict.flags,
    ] {
        for v in map.values_mut() {
            if v.contains(old) {
                *v = replace_path_token_boundary(v, old, new);
            }
        }
    }
}

/// 为单个扁平 Map 规划冲突条目：与 merged 键同值异时，
/// 数值型 token 重映射到同族空闲编号（返回新键条目并登记 planned），非数值型保留首值并记录丢弃。
/// 键同值同视为跨块重复，跳过；键不存在时原样收录。
fn plan_flat_map_entries(
    incoming: &std::collections::HashMap<String, String>,
    merged: &std::collections::HashMap<String, String>,
    used: &mut std::collections::HashSet<String>,
    log: &mut Vec<String>,
    planned: &mut Vec<(String, String)>,
) -> Vec<(String, String)> {
    let mut keys: Vec<&String> = incoming.keys().collect();
    keys.sort();
    let mut entries = Vec::with_capacity(keys.len());
    for key in keys {
        let value = &incoming[key];
        match merged.get(key) {
            Some(existing) if existing == value => {}
            Some(_) => match split_numbered_token(key) {
                Some((prefix, num)) => {
                    let mut next = max_numbered_suffix(merged, prefix).unwrap_or(0).max(num) + 1;
                    let mut candidate = format!("{}{}", prefix, next);
                    while used.contains(&candidate) {
                        next += 1;
                        candidate = format!("{}{}", prefix, next);
                    }
                    used.insert(candidate.clone());
                    log.push(format!("{} -> {}", key, candidate));
                    planned.push((key.clone(), candidate.clone()));
                    entries.push((candidate, value.clone()));
                }
                None => {
                    log.push(format!(
                        "{} kept first value (non-numeric token, conflicting later value dropped)",
                        key
                    ));
                }
            },
            None => entries.push((key.clone(), value.clone())),
        }
    }
    entries
}

/// 合并单块词典进 merged：六数值族 Map 走冲突重映射（P2-80），aliases/custom 键为语义名不可重映射，
/// 冲突时保留首值并记录丢弃（P3-09：禁止静默覆盖）。
fn merge_chunk_dictionary(
    chunk_tokens: &mut Vec<Token>,
    chunk_dict: &mut crate::core::dictionary_engine::Dictionary,
    merged_dict: &mut crate::core::dictionary_engine::Dictionary,
    log: &mut Vec<String>,
) {
    // 冲突候选编号必须避开：已合并键 ∪ 本块全部键（防重映射撞上本块既有 token）
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    let merged_maps = [
        &merged_dict.paths,
        &merged_dict.packages,
        &merged_dict.macros,
        &merged_dict.files,
        &merged_dict.directories,
        &merged_dict.flags,
    ];
    let chunk_maps = [
        &chunk_dict.paths,
        &chunk_dict.packages,
        &chunk_dict.macros,
        &chunk_dict.files,
        &chunk_dict.directories,
        &chunk_dict.flags,
    ];
    for m in merged_maps.iter() {
        used.extend(m.keys().cloned());
    }
    for m in chunk_maps.iter() {
        used.extend(m.keys().cloned());
    }

    // Phase A：逐 Map 规划冲突重映射，产出待插入条目（键已换新）
    let mut planned: Vec<(String, String)> = Vec::new();
    let paths_entries = plan_flat_map_entries(
        &chunk_dict.paths,
        &merged_dict.paths,
        &mut used,
        log,
        &mut planned,
    );
    let packages_entries = plan_flat_map_entries(
        &chunk_dict.packages,
        &merged_dict.packages,
        &mut used,
        log,
        &mut planned,
    );
    let macros_entries = plan_flat_map_entries(
        &chunk_dict.macros,
        &merged_dict.macros,
        &mut used,
        log,
        &mut planned,
    );
    let files_entries = plan_flat_map_entries(
        &chunk_dict.files,
        &merged_dict.files,
        &mut used,
        log,
        &mut planned,
    );
    let directories_entries = plan_flat_map_entries(
        &chunk_dict.directories,
        &merged_dict.directories,
        &mut used,
        log,
        &mut planned,
    );
    let flags_entries = plan_flat_map_entries(
        &chunk_dict.flags,
        &merged_dict.flags,
        &mut used,
        log,
        &mut planned,
    );

    // Phase B：把全部重映射同步改写进本块 tokens 与词典值（含嵌套引用）
    for (old, new) in &planned {
        for t in chunk_tokens.iter_mut() {
            rewrite_token_strings(t, old, new);
        }
        rewrite_dictionary_values(chunk_dict, old, new);
    }

    // Phase C：提交
    for (k, v) in paths_entries {
        merged_dict.paths.insert(k, v);
    }
    for (k, v) in packages_entries {
        merged_dict.packages.insert(k, v);
    }
    for (k, v) in macros_entries {
        merged_dict.macros.insert(k, v);
    }
    for (k, v) in files_entries {
        merged_dict.files.insert(k, v);
    }
    for (k, v) in directories_entries {
        merged_dict.directories.insert(k, v);
    }
    for (k, v) in flags_entries {
        merged_dict.flags.insert(k, v);
    }

    // aliases：键是语义别名（内容本身），不可重映射；冲突保留首值并告警
    for (alias, target) in chunk_dict.aliases.iter() {
        match merged_dict.aliases.get(alias) {
            Some(existing) if existing == target => {}
            Some(_) => log.push(format!(
                "alias {} kept first value (conflicting later value dropped)",
                alias
            )),
            None => {
                merged_dict.aliases.insert(alias.clone(), target.clone());
            }
        }
    }

    // custom：命名空间内冲突保留首值并告警
    for (ns, entries) in chunk_dict.custom.iter() {
        let slot = merged_dict.custom.entry(ns.clone()).or_default();
        for (k, v) in entries {
            match slot.get(k) {
                Some(existing) if existing == v => {}
                Some(_) => log.push(format!(
                    "custom[{}] {} kept first value (conflicting later value dropped)",
                    ns, k
                )),
                None => {
                    slot.insert(k.clone(), v.clone());
                }
            }
        }
    }
}

pub(crate) fn merge_compression_outputs(
    outputs: Vec<CompressionOutput>,
) -> Option<CompressionOutput> {
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
    // P1-08：跨块源编码传播——取首个非 None；块间不一致记 "mixed-auto"
    // （与解码子系统同名语义：单一回写必然失真，解压侧须显式拒绝）。
    let mut source_encoding: Option<String> = None;
    // P2-80/P3-09：跨块词典冲突检测与重映射记录（"旧 -> 新" 或丢弃说明）
    let mut merge_log: Vec<String> = Vec::new();

    for out in outputs {
        let mut chunk_tokens = out.tokens;
        let mut chunk_dict = out.dictionary;

        // P2-80 根因修：流式 --merge 时各块词典独立编号（如 $P1 每块从 1 重启），
        // 直接 extend 会后块覆盖前块 → decompress 错向还原。
        // 修法：合并前对六个扁平 Map 做冲突检测，数值型 token 重映射到同族空闲编号，
        // 并同步改写该块 tokens 与词典值（边界感知替换，防 $P1 误伤 $P12）。
        merge_chunk_dictionary(
            &mut chunk_tokens,
            &mut chunk_dict,
            &mut merged_dict,
            &mut merge_log,
        );

        merged_tokens.extend(chunk_tokens);

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
        // P1-08：源编码传播（首个非 None 优先，块间不一致退化为 mixed-auto）。
        if let Some(enc) = &out.metadata.source_encoding {
            match &source_encoding {
                None => source_encoding = Some(enc.clone()),
                Some(prev) if prev != enc => source_encoding = Some("mixed-auto".to_string()),
                _ => {}
            }
        }
    }

    // P3-09：冲突必须显式告警，禁止静默覆盖/静默丢弃
    if !merge_log.is_empty() {
        eprintln!(
            "[tokenslim merge] dictionary conflicts detected and resolved: {}",
            merge_log.join("; ")
        );
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
        source_encoding,
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

    /// 校验空输出列表合并时返回 None。
    #[test]
    fn test_merge_compression_outputs_empty() {
        assert!(merge_compression_outputs(vec![]).is_none());
    }

    /// P2-89 负路径回归：产物估算字节不小于原文时守门触发——
    /// 回退为单 Token::Text 原文透传，metadata 诚实记录等尺寸/零收益。
    #[test]
    fn test_guard_negative_savings_triggers_passthrough() {
        let original = "git diff --cached --name-only";
        let out = CompressionOutput {
            tokens: vec![crate::core::compression::Token::Text(std::borrow::Cow::Owned(
                format!("{original}!!!!"), // 产物比原文长（模拟小输入字典标记开销）
            ))],
            dictionary: Dictionary::default(),
            metadata: CompressionMetadata {
                original_size: original.len(),
                compressed_size: original.len() + 4,
                original_tokens: 89,
                compressed_tokens: 95,
                token_savings: 0,
                compression_ratio: 1.0672,
                token_ratio: 1.0674,
                ..Default::default()
            },
        };
        let (guarded_out, triggered) = guard_negative_savings(out, original);
        assert!(triggered);
        assert_eq!(guarded_out.tokens.len(), 1);
        match &guarded_out.tokens[0] {
            crate::core::compression::Token::Text(s) => assert_eq!(s.as_ref(), original),
            other => panic!("期望 Text 透传 token，实际 {:?}", other),
        }
        let m = &guarded_out.metadata;
        assert_eq!(m.compressed_size, m.original_size);
        assert_eq!(m.compressed_tokens, m.original_tokens);
        assert_eq!(m.token_savings, 0);
        assert_eq!(m.compression_ratio, 1.0);
        assert_eq!(m.token_ratio, 1.0);
    }

    /// P2-89 正路径回归：产物小于原文时守门不干预，原样返回。
    #[test]
    fn test_guard_negative_savings_passes_through_profitable_output() {
        let original = "long input text with many compressible repeated tokens";
        let out = CompressionOutput {
            tokens: vec![crate::core::compression::Token::Text(std::borrow::Cow::Borrowed(
                "compressed",
            ))],
            dictionary: Dictionary::default(),
            metadata: CompressionMetadata {
                original_size: original.len(),
                compressed_size: 10,
                compression_ratio: 0.18,
                ..Default::default()
            },
        };
        let (out2, triggered) = guard_negative_savings(out, original);
        assert!(!triggered);
        assert_eq!(out2.metadata.compressed_size, 10);
    }

    /// P2-89 边界回归：空原文不守门（避免对空输入产生任何写动作）。
    #[test]
    fn test_guard_negative_savings_empty_input_noop() {
        let out = CompressionOutput {
            tokens: vec![],
            dictionary: Dictionary::default(),
            metadata: CompressionMetadata::default(),
        };
        let (_, triggered) = guard_negative_savings(out, "");
        assert!(!triggered);
    }

    /// 校验两段有效输出合并后，尺寸/token/字典字段均被正确累加。
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

    /// 构造一个最小 CompressionOutput（仅 tokens 与词典 paths，metadata 走默认）。
    fn make_output<'a>(tokens: Vec<Token<'a>>, paths: Vec<(&str, &str)>) -> CompressionOutput {
        let mut dict = Dictionary::default();
        for (k, v) in paths {
            dict.paths.insert(k.to_string(), v.to_string());
        }
        CompressionOutput {
            tokens: tokens.into_iter().map(|t| t.into_owned()).collect(),
            dictionary: dict,
            metadata: CompressionMetadata::default(),
        }
    }

    /// P2-80 负路径回归：两块都分配了 $P1 但指向不同路径——合并后块 1 的 $P1 语义不得被块 2 覆盖，
    /// 块 2 的 $P1 必须被重映射为空闲编号（$P2）且其 tokens 内引用同步改写，decompress 不再错向还原。
    #[test]
    fn test_merge_token_conflict_is_remapped_not_overwritten() {
        let out1 = make_output(
            vec![
                Token::Text(Cow::Borrowed("M $P1/a.rs\n")),
                Token::Text(Cow::Borrowed("paths: $P1=dir1\n")),
            ],
            vec![("$P1", "dir1")],
        );
        let out2 = make_output(
            vec![
                Token::Text(Cow::Borrowed("M $P1/b.rs\n")),
                Token::Text(Cow::Borrowed("paths: $P1=dir2\n")),
            ],
            vec![("$P1", "dir2")],
        );

        let merged = merge_compression_outputs(vec![out1, out2]).unwrap();

        // 词典：块 1 的 $P1 保持 dir1，块 2 的路径以新编号 $P2 存在
        assert_eq!(merged.dictionary.paths.get("$P1").unwrap(), "dir1");
        assert_eq!(merged.dictionary.paths.get("$P2").unwrap(), "dir2");

        // tokens：块 1 引用不变；块 2 的 $P1 全部改写为 $P2（含页脚行）
        let joined: String = merged
            .tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.to_string(),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("M $P1/a.rs"), "chunk1 unchanged: {joined}");
        assert!(
            joined.contains("$P1=dir1"),
            "chunk1 footer unchanged: {joined}"
        );
        assert!(joined.contains("M $P2/b.rs"), "chunk2 remapped: {joined}");
        assert!(
            joined.contains("$P2=dir2"),
            "chunk2 footer remapped: {joined}"
        );
        // 不允许残留任何仍指向 dir2 的 $P1 引用
        assert!(
            !joined.contains("$P1/b.rs"),
            "no stale chunk2 ref: {joined}"
        );
    }

    /// P2-80 负路径回归：键同值同的跨块重复不触发重映射（幂等合并）。
    #[test]
    fn test_merge_same_key_same_value_is_idempotent() {
        let out1 = make_output(vec![], vec![("$P1", "dir1")]);
        let out2 = make_output(vec![], vec![("$P1", "dir1")]);
        let merged = merge_compression_outputs(vec![out1, out2]).unwrap();
        assert_eq!(merged.dictionary.paths.len(), 1);
        assert_eq!(merged.dictionary.paths.get("$P1").unwrap(), "dir1");
    }

    /// P2-80 边界：重映射新编号必须避开两块已占用的全部编号（$P2 被块 2 占用时跳到 $P3），
    /// 且改写 `$P1` 时不得误伤 token-like 片段（`$P12`、`$P1-notes`）。
    #[test]
    fn test_merge_remap_avoids_used_numbers_and_respects_boundaries() {
        let out1 = make_output(
            vec![Token::Text(Cow::Borrowed("M $P12/keep.rs\nM $P1/one.rs\n"))],
            vec![("$P1", "dir1"), ("$P12", "dir12")],
        );
        // 块 2 复用 $P1（冲突），且自身还占用 $P2 与 $P1-notes 字面量
        let out2 = make_output(
            vec![Token::Text(Cow::Borrowed(
                "M $P1/two.rs\nM $P2/other.rs\nW $P1-notes/literal.md\n",
            ))],
            vec![("$P1", "dir2"), ("$P2", "dir2b")],
        );

        let merged = merge_compression_outputs(vec![out1, out2]).unwrap();

        // 块 2 的 $P1 重映射：分配规则 = 同族最大编号+1（$P12 已占用）→ 落到 $P13
        assert_eq!(merged.dictionary.paths.get("$P1").unwrap(), "dir1");
        assert_eq!(merged.dictionary.paths.get("$P12").unwrap(), "dir12");
        assert_eq!(merged.dictionary.paths.get("$P2").unwrap(), "dir2b");
        assert_eq!(merged.dictionary.paths.get("$P13").unwrap(), "dir2");

        let joined: String = merged
            .tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.to_string(),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("M $P13/two.rs"),
            "remapped to free id: {joined}"
        );
        assert!(
            joined.contains("M $P2/other.rs"),
            "existing $P2 untouched: {joined}"
        );
        // 边界保护（与 token_boundary 既有语法一致）：`-` 为续字符，$P1-notes 字面量不被误改写
        assert!(
            joined.contains("W $P1-notes/literal.md"),
            "literal suffix segment preserved: {joined}"
        );
        assert!(
            !joined.contains("M $P1/two.rs"),
            "no stale conflicting ref: {joined}"
        );
    }

    /// P3-09 回归：aliases 冲突（键同值异）不得静默覆盖——保留首值（本测试经由重映射日志路径验证丢弃行为，
    /// 走 eprintln 的告警在 stdout 无断言，这里验证合并结果语义）。
    #[test]
    fn test_merge_alias_conflict_keeps_first_value() {
        let mut d1 = Dictionary::default();
        d1.aliases
            .insert("src".to_string(), "source dir".to_string());
        let mut d2 = Dictionary::default();
        d2.aliases
            .insert("src".to_string(), "another meaning".to_string());
        let out1 = CompressionOutput {
            tokens: vec![],
            dictionary: d1,
            metadata: CompressionMetadata::default(),
        };
        let out2 = CompressionOutput {
            tokens: vec![],
            dictionary: d2,
            metadata: CompressionMetadata::default(),
        };
        let merged = merge_compression_outputs(vec![out1, out2]).unwrap();
        assert_eq!(merged.dictionary.aliases.get("src").unwrap(), "source dir");
    }
}
