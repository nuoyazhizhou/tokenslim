//! Encoding fallback decoder and UTF-8 write utilities
//!
//! Tries UTF-8 first, then falls back to common codepages based on locale detection.
//! This prevents `from_utf8_lossy` from silently corrupting non-UTF-8 text.
//!
//! ## Encoding Policy
//!
//! All file writes from TokenSlim use UTF-8 without BOM. This ensures:
//! - Consistent behavior across platforms (Windows/Linux/macOS)
//! - No BOM prefix that could break parsers or scripts
//! - Maximum compatibility with tools and LLMs

use std::path::Path;
use std::process::Command;

use chardetng::EncodingDetector;
use encoding_rs::{
    Encoding, BIG5, EUC_JP, EUC_KR, GB18030, GBK, IBM866, SHIFT_JIS, UTF_16BE, UTF_16LE, UTF_8,
    WINDOWS_1250, WINDOWS_1251, WINDOWS_1252, WINDOWS_1253, WINDOWS_1254, WINDOWS_1255,
    WINDOWS_1256, WINDOWS_1258, WINDOWS_874,
};

/// Write a string to a file as UTF-8 without BOM.
///
/// This is the preferred write method for all TokenSlim file output.
/// It guarantees:
/// - UTF-8 encoding (Rust strings are already UTF-8)
/// - No BOM prefix
/// - Atomic-ish behavior (writes complete content or fails)
pub fn write_utf8(path: &Path, content: &str) -> std::io::Result<()> {
    // Strip any accidental BOM from the content
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    std::fs::write(path, content.as_bytes())
}

/// Write a string to a file as UTF-8 with BOM.
///
/// Only use this when explicitly required by legacy tools.
/// Default to [`write_utf8`] (no BOM) for all new code.
pub fn write_utf8_bom(path: &Path, content: &str) -> std::io::Result<()> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut bytes = Vec::with_capacity(content.len() + 3);
    bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]); // UTF-8 BOM
    bytes.extend_from_slice(content.as_bytes());
    std::fs::write(path, bytes)
}

/// Try to decode bytes with UTF-8 first, then fallback to common codepages.
/// Returns the decoded string and the encoding name that succeeded.
pub fn decode_with_fallback(bytes: &[u8]) -> (String, &'static str) {
    decode_with_fallback_internal(bytes, forced_encoding_hint())
}

/// Decode bytes and apply lightweight display-oriented repair steps.
/// Returns repaired text, detected encoding, and applied repair step names.
pub fn decode_and_repair_for_display(bytes: &[u8]) -> (String, &'static str, Vec<String>) {
    let (decoded, enc) = decode_with_fallback(bytes);
    if is_probable_binary_bytes(bytes) {
        return (decoded, enc, vec!["binary-guard-skip-repair".to_string()]);
    }
    let (repaired, steps) = repair_text_for_display(&decoded);
    (repaired, enc, steps)
}

/// Best-effort text repair for common display issues:
/// - strips accidental UTF-8 BOM marker
/// - normalizes CRLF to LF
/// - repairs common mojibake chains (windows-1252 -> utf-8), iterative
pub fn repair_text_for_display(input: &str) -> (String, Vec<String>) {
    let (mut text, mut steps) = normalize_display_text(input);
    let cjk_mojibake_like = contains_cjk_mojibake_signature(&text);
    let mut likely_mojibake = is_probable_mojibake_text(&text);
    for pass in 1..=3 {
        let Some(pass_result) = run_mojibake_repair_pass(
            pass,
            &text,
            likely_mojibake,
            cjk_mojibake_like,
            steps.is_empty(),
        ) else {
            break;
        };
        text = pass_result.next_text;
        steps.push(pass_result.step);
        likely_mojibake = pass_result.next_likely_mojibake;
        if !likely_mojibake && pass >= 2 {
            break;
        }
    }

    (text, steps)
}

struct MojibakeRepairPass {
    next_text: String,
    step: String,
    next_likely_mojibake: bool,
}

/// 执行一轮乱码修复；先尝试 cp932/windows-31j 专用首轮修复，否则选择最佳重解释候选并验证其确实改善了文本（marker/replacement 下降或乱码启发式消失），返回下一轮文本与修复步骤名。
fn run_mojibake_repair_pass(
    pass: usize,
    current: &str,
    likely_mojibake: bool,
    cjk_mojibake_like: bool,
    no_cleanup_steps: bool,
) -> Option<MojibakeRepairPass> {
    if should_skip_repair_pass(likely_mojibake, cjk_mojibake_like, no_cleanup_steps) {
        return None;
    }

    if let Some(cp932_fixed) = try_cp932_repair_first_pass(cjk_mojibake_like, pass, current) {
        let next_likely_mojibake = is_probable_mojibake_text(&cp932_fixed);
        return Some(MojibakeRepairPass {
            next_text: cp932_fixed,
            step: "mojibake-repair-pass-1:windows-31j(cp932)->utf8".to_string(),
            next_likely_mojibake,
        });
    }

    let Some((next_text, chain_label)) = best_reinterpretation_candidate(current, likely_mojibake)
    else {
        return None;
    };
    let next_likely_mojibake = is_probable_mojibake_text(&next_text);
    if !is_repair_candidate_improved(current, &next_text, likely_mojibake, next_likely_mojibake) {
        return None;
    }
    Some(MojibakeRepairPass {
        next_text,
        step: format!("mojibake-repair-pass-{pass}:{chain_label}"),
        next_likely_mojibake,
    })
}

/// 判断本轮修复是否应跳过；当文本既不像乱码、也无 CJK 乱码特征、且此前无任何清理步骤时返回 true（无需也无法继续修复）。
fn should_skip_repair_pass(
    likely_mojibake: bool,
    cjk_mojibake_like: bool,
    no_cleanup_steps: bool,
) -> bool {
    !likely_mojibake && !cjk_mojibake_like && no_cleanup_steps
}

/// cp932/windows-31j 专用首轮修复；仅在第一轮且文本具 CJK 乱码特征时，将原文按 SHIFT_JIS 重新解释为 UTF-8，并确认结果不再含 CJK 乱码特征。
fn try_cp932_repair_first_pass(cjk_mojibake_like: bool, pass: usize, text: &str) -> Option<String> {
    if !cjk_mojibake_like || pass != 1 {
        return None;
    }
    let next = try_reinterpret_as_utf8(text, SHIFT_JIS)?;
    if next != text && !contains_cjk_mojibake_signature(&next) {
        return Some(next);
    }
    None
}

/// 评估修复候选是否优于当前文本；比较解码评分、乱码标记数、替换字符数及乱码启发式状态，任一指标改善即视为改进。
fn is_repair_candidate_improved(
    current: &str,
    next: &str,
    likely_mojibake: bool,
    next_mojibake: bool,
) -> bool {
    let old_score = score_decoded_text(current, false);
    let new_score = score_decoded_text(next, false);
    let old_markers = mojibake_marker_count(current);
    let new_markers = mojibake_marker_count(next);
    let old_repl = replacement_marker_count(current);
    let new_repl = replacement_marker_count(next);

    // 重解释若引入了更多替换字符（U+FFFD），说明把原本有效的字节流损坏了（如 GB18030 整段重编）
    if new_repl > old_repl {
        return false;
    }

    new_markers < old_markers
        || new_repl < old_repl
        || new_score >= old_score + 4
        || (likely_mojibake && !next_mojibake)
}

/// 规范化展示用文本并返回所执行的清理步骤；依次剥离前导 BOM、将 CRLF/CR 归一为 LF、剥离行内 BOM、移除不可见控制字符，并在分数不下降时剥离 NUL。
fn normalize_display_text(input: &str) -> (String, Vec<String>) {
    let mut text = input.to_string();
    let mut steps = Vec::<String>::new();

    if let Some(stripped) = text.strip_prefix('\u{feff}') {
        text = stripped.to_string();
        steps.push("strip-leading-bom".to_string());
    }

    if text.contains("\r\n") {
        text = text.replace("\r\n", "\n");
        steps.push("normalize-crlf".to_string());
    }
    if text.contains('\r') {
        text = text.replace('\r', "\n");
        steps.push("normalize-cr".to_string());
    }
    if text.contains('\u{feff}') {
        text = text.replace('\u{feff}', "");
        steps.push("strip-inline-bom".to_string());
    }
    let (stripped_controls, removed_controls) = strip_invisible_control_chars(&text);
    if removed_controls > 0 {
        text = stripped_controls;
        steps.push(format!("strip-invisible-controls:{removed_controls}"));
    }
    if text.contains('\0') {
        let candidate = text.replace('\0', "");
        let old_score = score_decoded_text(&text, false);
        let new_score = score_decoded_text(&candidate, false);
        if new_score >= old_score {
            text = candidate;
            steps.push("strip-nul".to_string());
        }
    }
    (text, steps)
}

/// 启发式判断字节流是否为二进制；排除已知 BOM 后，按样本中 NUL 与控制字符占比判定（带 UTF-16/UTF-32 无 BOM 的豁免），避免把 UTF-16/32 误判为二进制。
pub fn is_probable_binary_bytes(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF])
        || bytes.starts_with(&[0xFF, 0xFE])
        || bytes.starts_with(&[0xFE, 0xFF])
        || bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00])
        || bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF])
    {
        return false;
    }

    let sample_len = bytes.len().min(8192);
    let sample = &bytes[..sample_len];
    let nul_count = sample.iter().filter(|b| **b == 0).count();
    let control_count = sample
        .iter()
        .filter(|b| matches!(**b, 0x01..=0x06 | 0x0E..=0x1A | 0x1C..=0x1F))
        .count();

    let nul_ratio = nul_count as f64 / sample.len() as f64;
    let control_ratio = control_count as f64 / sample.len() as f64;

    if nul_ratio >= 0.30 {
        return !looks_like_utf16_or_utf32_without_bom(sample);
    }
    nul_ratio >= 0.20 || control_ratio >= 0.22
}

/// 评估修复置信度；对比修复前后的乱码标记、替换字符、NUL 与乱码启发式，结合步骤数给出 high/medium/low 等级及证据列表。
pub fn evaluate_repair_confidence(
    original: &str,
    repaired: &str,
    steps: &[String],
) -> (String, Vec<String>) {
    let mut evidence = Vec::<String>::new();
    let old_markers = mojibake_marker_count(original);
    let new_markers = mojibake_marker_count(repaired);
    let old_repl = replacement_marker_count(original);
    let new_repl = replacement_marker_count(repaired);
    let old_nul = nul_marker_count(original);
    let new_nul = nul_marker_count(repaired);
    let old_bad = is_probable_mojibake_text(original);
    let new_bad = is_probable_mojibake_text(repaired);

    let mut score = 0i32;
    if !steps.is_empty() {
        evidence.push(format!("repair-steps={}", steps.join(", ")));
        score += 1;
    } else {
        evidence.push("repair-steps=none".to_string());
    }
    if new_markers < old_markers {
        evidence.push(format!("mojibake-markers:{}->{}", old_markers, new_markers));
        score += 2;
    }
    if new_repl < old_repl {
        evidence.push(format!("replacement-chars:{}->{}", old_repl, new_repl));
        score += 2;
    }
    if new_nul < old_nul {
        evidence.push(format!("nul-chars:{}->{}", old_nul, new_nul));
        score += 1;
    }
    if old_bad && !new_bad {
        evidence.push("mojibake-heuristic:recovered".to_string());
        score += 2;
    } else if old_bad && new_bad {
        evidence.push("mojibake-heuristic:still-suspicious".to_string());
        score -= 1;
    }
    if original != repaired {
        evidence.push("content-changed=true".to_string());
        score += 1;
    } else {
        evidence.push("content-changed=false".to_string());
    }

    let confidence = if score >= 5 {
        "high"
    } else if score >= 2 {
        "medium"
    } else {
        "low"
    };
    (confidence.to_string(), evidence)
}

/// 启发式判断文本是否为乱码；统计替换字符（U+FFFD）、典型乱码标记字符（Ã/Â/Ð 等）与可疑符号占比，综合判定。
pub fn is_probable_mojibake_text(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }

    let mut total = 0usize;
    let mut marker = 0usize;
    let mut replacement = 0usize;
    let mut suspicious_symbol = 0usize;

    for ch in text.chars() {
        total += 1;
        if ch == '\u{fffd}' {
            replacement += 1;
        }
        if matches!(ch, 'Ã' | 'Â' | 'Ð' | 'Ñ' | 'â' | '¤' | '¥' | '¦' | '§') {
            marker += 1;
        }
        if matches!(ch, '¸' | '–' | '‡' | '™' | 'œ' | 'ž') {
            suspicious_symbol += 1;
        }
    }

    if replacement >= 2 {
        return true;
    }
    if total < 8 {
        return marker >= 2 && suspicious_symbol >= 1;
    }

    let marker_ratio = marker as f64 / total as f64;
    marker >= 2 && marker_ratio >= 0.08
}

/// 统计文本中典型乱码标记字符（Ã/Â/Ð/Ñ/â 等 Latin-1→UTF-8 双重编码特征字符）的数量。
fn mojibake_marker_count(text: &str) -> usize {
    text.chars()
        .filter(|ch| {
            matches!(
                ch,
                'Ã' | 'Â'
                    | 'Ð'
                    | 'Ñ'
                    | 'â'
                    | '¤'
                    | '¥'
                    | '¦'
                    | '§'
                    | '¸'
                    | '–'
                    | '‡'
                    | '™'
                    | 'œ'
                    | 'ž'
            )
        })
        .count()
}

/// 统计文本中 Unicode 替换字符 U+FFFD 的数量。
fn replacement_marker_count(text: &str) -> usize {
    text.chars().filter(|ch| *ch == '\u{fffd}').count()
}

/// 统计文本中 NUL（\0）字符的数量。
fn nul_marker_count(text: &str) -> usize {
    text.chars().filter(|ch| *ch == '\0').count()
}

/// 返回用于乱码重解释的编码链；依次尝试 windows-1252/gbk/gb18030/shift_jis，并将 SHIFT_JIS 额外以 windows-31j(cp932) 名义再试一次。
fn reinterpretation_chains() -> [(&'static Encoding, &'static str); 5] {
    [
        (WINDOWS_1252, "windows-1252->utf8"),
        (GBK, "gbk->utf8"),
        (GB18030, "gb18030->utf8"),
        (SHIFT_JIS, "shift_jis->utf8"),
        (SHIFT_JIS, "windows-31j(cp932)->utf8"),
    ]
}

/// 遍历重解释编码链，对每个候选按解码评分与乱码特征变化计算增益，挑出增益最大的重解释结果（文本 + 标签）。
fn best_reinterpretation_candidate(input: &str, likely_mojibake: bool) -> Option<(String, String)> {
    let old_score = score_decoded_text(input, false);
    let old_markers = mojibake_marker_count(input);
    let old_repl = replacement_marker_count(input);
    let old_cjk_sig = contains_cjk_mojibake_signature(input);
    let mut best: Option<(String, String, i32)> = None;

    for (encoding, label) in reinterpretation_chains() {
        let Some(next) = try_reinterpret_as_utf8(input, encoding) else {
            continue;
        };
        if next == input {
            continue;
        }
        let new_score = score_decoded_text(&next, false);
        let new_markers = mojibake_marker_count(&next);
        let new_repl = replacement_marker_count(&next);
        let new_bad = is_probable_mojibake_text(&next);
        if likely_mojibake && new_bad && new_markers >= old_markers && new_repl >= old_repl {
            continue;
        }

        let gain = reinterpretation_gain(
            old_score,
            new_score,
            old_markers,
            new_markers,
            old_repl,
            new_repl,
            old_cjk_sig,
            &next,
            likely_mojibake,
            new_bad,
        );
        if gain <= 0 {
            continue;
        }
        let replace = best.as_ref().map(|(_, _, bg)| gain > *bg).unwrap_or(true);
        if replace {
            best = Some((next, label.to_string(), gain));
        }
    }

    best.map(|(next, label, _)| (next, label))
}

/// 计算一次重解释相较原文的增益分数；marker/replacement 下降、CJK 乱码特征消失、乱码启发式恢复均加分，结果取非负。
fn reinterpretation_gain(
    old_score: i32,
    new_score: i32,
    old_markers: usize,
    new_markers: usize,
    old_repl: usize,
    new_repl: usize,
    old_cjk_sig: bool,
    next: &str,
    likely_mojibake: bool,
    new_bad: bool,
) -> i32 {
    let mut gain = (new_score - old_score).max(0);
    if new_markers < old_markers {
        gain += 8;
    }
    if new_repl < old_repl {
        gain += 8;
    }
    if old_cjk_sig && !contains_cjk_mojibake_signature(next) {
        gain += 12;
    }
    if likely_mojibake && !new_bad {
        gain += 10;
    }
    gain
}

/// 将文本按指定源编码重新编码回字节，再以 UTF-8 无损解码；若源编码无法表示原文（编码出错）则返回 None。
fn try_reinterpret_as_utf8(text: &str, source_encoding: &'static Encoding) -> Option<String> {
    let (bytes, _, had_errors) = source_encoding.encode(text);
    if had_errors {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// 解码回退的核心实现；空输入直接返回 UTF-8，否则先尝试 BOM/UTF-16/UTF-32/纯 UTF-8 直接路径，再转入候选编码评分与混合解码。
fn decode_with_fallback_internal(
    bytes: &[u8],
    preferred_hint: Option<&'static Encoding>,
) -> (String, &'static str) {
    if bytes.is_empty() {
        return (String::new(), "utf-8");
    }

    if let Some(decoded) = try_direct_unicode_decoding(bytes) {
        return decoded;
    }

    let candidates = build_decode_candidates(bytes, preferred_hint);
    decode_with_candidates_or_lossy(bytes, &candidates)
}

/// 尝试不依赖统计检测的直接 Unicode 解码路径；依次尝试 BOM 解码、无 BOM 的 UTF-16/UTF-32 解码，最后尝试纯 UTF-8。
fn try_direct_unicode_decoding(bytes: &[u8]) -> Option<(String, &'static str)> {
    if let Some((decoded, enc_name)) = decode_by_bom(bytes) {
        return Some((decoded, enc_name));
    }
    if let Some((decoded, enc_name)) = decode_utf16_without_bom(bytes) {
        return Some((decoded, enc_name));
    }
    if let Some((decoded, enc_name)) = decode_utf32_without_bom(bytes) {
        return Some((decoded, enc_name));
    }
    if let Ok(s) = String::from_utf8(bytes.to_vec()) {
        return Some((s, "utf-8"));
    }
    None
}

/// 从候选编码中选出最佳单编码结果；若混合解码（按行/按块）能显著超过单编码，则返回 "mixed-auto" 结果。
fn select_candidate_or_mixed_result(
    bytes: &[u8],
    candidates: &[DecodedCandidate],
) -> Option<(String, &'static str)> {
    let best_single = decode_best_candidate(bytes, candidates)?;
    if let Some(mixed_best) = try_promote_mixed_decode(bytes, candidates, &best_single) {
        return Some((mixed_best, "mixed-auto"));
    }
    if best_single.score >= 0 {
        return Some((best_single.decoded, best_single.enc.name()));
    }
    None
}

/// 用候选编码解码字节；若无合适候选或全部失败，则退回 `from_utf8_lossy` 并标记为 "utf-8-lossy"。
fn decode_with_candidates_or_lossy(
    bytes: &[u8],
    candidates: &[DecodedCandidate],
) -> (String, &'static str) {
    if let Some(decoded) = select_candidate_or_mixed_result(bytes, candidates) {
        return decoded;
    }
    (String::from_utf8_lossy(bytes).into_owned(), "utf-8-lossy")
}

/// 在单编码结果基础上尝试混合解码；按行与按块两种方式分别计算，仅当混合结果分数超过单编码阈值且确实切换了编码时才采用。
fn try_promote_mixed_decode(
    bytes: &[u8],
    candidates: &[DecodedCandidate],
    best_single: &DecodedResult,
) -> Option<String> {
    if !should_try_mixed_decode(bytes) {
        return None;
    }

    let mixed_threshold = best_single.score + 10;
    if let Some(mixed) = decode_mixed_by_lines(bytes, candidates, best_single.enc) {
        if mixed.score >= mixed_threshold && mixed.switched {
            return Some(mixed.decoded);
        }
    }
    if let Some(mixed) = decode_mixed_by_chunks(bytes, candidates, best_single.enc) {
        if mixed.score >= mixed_threshold && mixed.switched {
            return Some(mixed.decoded);
        }
    }
    None
}

/// 用 chardetng 对字节流做统计编码检测，返回最可能的编码（猜测，不强制）。
fn detect_statistical_encoding(bytes: &[u8]) -> Option<&'static Encoding> {
    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    Some(detector.guess(None, true))
}

#[derive(Clone)]
struct DecodedCandidate {
    enc: &'static Encoding,
    source_bonus: i32,
}

struct DecodedResult {
    decoded: String,
    enc: &'static Encoding,
    score: i32,
}

struct MixedDecodeResult {
    decoded: String,
    score: i32,
    switched: bool,
}

/// 构建解码候选编码列表并按优先级赋分；依次纳入强制编码提示、统计检测结果、首选代码页、locale 提示与通用回退编码，且列表内去重。
fn build_decode_candidates(
    bytes: &[u8],
    preferred_hint: Option<&'static Encoding>,
) -> Vec<DecodedCandidate> {
    let mut candidates: Vec<DecodedCandidate> = Vec::new();

    if let Some(enc) = preferred_hint {
        push_unique_candidate(&mut candidates, enc, 80);
    }

    if let Some(enc) = detect_statistical_encoding(bytes) {
        push_unique_candidate(&mut candidates, enc, 70);
    }

    if let Some(cp) = detect_preferred_codepage() {
        if let Some(enc) = codepage_to_encoding(&cp) {
            push_unique_candidate(&mut candidates, enc, 60);
        }
    }

    if let Some(enc) = locale_hint_encoding() {
        push_unique_candidate(&mut candidates, enc, 50);
    }

    for enc in common_fallback_encodings() {
        push_unique_candidate(&mut candidates, enc, 20);
    }

    candidates
}

/// 用各候选编码解码并评分（解码质量 + 来源加分），返回分数最高的单一解码结果。
fn decode_best_candidate(bytes: &[u8], candidates: &[DecodedCandidate]) -> Option<DecodedResult> {
    let mut best: Option<DecodedResult> = None;
    for candidate in candidates {
        let (decoded, _, had_errors) = candidate.enc.decode(bytes);
        let decoded = decoded.into_owned();
        let score = score_decoded_text(&decoded, had_errors) + candidate.source_bonus;
        let replace = match &best {
            Some(existing) => score > existing.score,
            None => true,
        };
        if replace {
            best = Some(DecodedResult {
                decoded,
                enc: candidate.enc,
                score,
            });
        }
    }
    best
}

/// 按行混合解码；逐行挑选最佳候选编码，支持沿用上一行编码以降低切换，整段为空时退回默认编码；返回解码文本、总分与是否发生编码切换。
fn decode_mixed_by_lines(
    bytes: &[u8],
    candidates: &[DecodedCandidate],
    default_enc: &'static Encoding,
) -> Option<MixedDecodeResult> {
    let mut decoded = String::with_capacity(bytes.len());
    let mut total_score = 0i32;
    let mut prev_enc: Option<&'static Encoding> = None;
    let mut switched = false;

    for segment in bytes.split_inclusive(|b| *b == b'\n') {
        if segment.is_empty() {
            continue;
        }
        let best_for_line = decode_best_segment_candidate(segment, candidates, prev_enc)?;
        if let Some(prev) = prev_enc {
            if prev != best_for_line.enc {
                switched = true;
            }
        }
        prev_enc = Some(best_for_line.enc);
        total_score += best_for_line.score;
        decoded.push_str(&best_for_line.decoded);
    }

    if decoded.is_empty() {
        let (fallback, _, had_errors) = default_enc.decode(bytes);
        let fallback = fallback.into_owned();
        return Some(MixedDecodeResult {
            score: score_decoded_text(&fallback, had_errors),
            decoded: fallback,
            switched: false,
        });
    }

    Some(MixedDecodeResult {
        decoded,
        score: total_score,
        switched,
    })
}

/// 按块混合解码；先按分隔符切分，不足一个块时退化为定长切分，逐块挑选最佳候选编码并累计分数与切换状态。
fn decode_mixed_by_chunks(
    bytes: &[u8],
    candidates: &[DecodedCandidate],
    default_enc: &'static Encoding,
) -> Option<MixedDecodeResult> {
    let segments = split_chunk_segments(bytes);
    if segments.len() <= 1 {
        return None;
    }

    let mut decoded = String::with_capacity(bytes.len());
    let mut total_score = 0i32;
    let mut prev_enc: Option<&'static Encoding> = None;
    let mut switched = false;

    for segment in segments {
        if segment.is_empty() {
            continue;
        }
        let best_for_segment = decode_best_segment_candidate(segment, candidates, prev_enc)?;
        if let Some(prev) = prev_enc {
            if prev != best_for_segment.enc {
                switched = true;
            }
        }
        prev_enc = Some(best_for_segment.enc);
        total_score += best_for_segment.score;
        decoded.push_str(&best_for_segment.decoded);
    }

    if decoded.is_empty() {
        let (fallback, _, had_errors) = default_enc.decode(bytes);
        let fallback = fallback.into_owned();
        return Some(MixedDecodeResult {
            score: score_decoded_text(&fallback, had_errors),
            decoded: fallback,
            switched: false,
        });
    }

    Some(MixedDecodeResult {
        decoded,
        score: total_score,
        switched,
    })
}

/// 为单个片段（行/块）挑选最佳候选编码；统计检测命中该编码时加权，并在分数接近时优先沿用上一片段的编码以保持一致性。
fn decode_best_segment_candidate(
    segment: &[u8],
    candidates: &[DecodedCandidate],
    prev_enc: Option<&'static Encoding>,
) -> Option<DecodedResult> {
    let line_detected = detect_statistical_encoding(segment);
    let mut best: Option<DecodedResult> = None;
    let mut prev_choice: Option<DecodedResult> = None;

    for candidate in candidates {
        let (decoded, _, had_errors) = candidate.enc.decode(segment);
        let decoded = decoded.into_owned();
        let mut score = score_decoded_text(&decoded, had_errors) + candidate.source_bonus;
        if line_detected == Some(candidate.enc) {
            score += 35;
        }

        if prev_enc.is_some() && prev_enc == Some(candidate.enc) {
            prev_choice = Some(DecodedResult {
                decoded: decoded.clone(),
                enc: candidate.enc,
                score,
            });
        }

        let replace = match &best {
            Some(existing) => score > existing.score,
            None => true,
        };
        if replace {
            best = Some(DecodedResult {
                decoded,
                enc: candidate.enc,
                score,
            });
        }
    }

    match (best, prev_choice) {
        (Some(best), Some(prev)) if best.score - prev.score <= 12 => Some(prev),
        (Some(best), _) => Some(best),
        _ => None,
    }
}

/// 判断是否值得尝试混合解码；输入长度过小（<10）或过大（>1MB）时返回 false，避免无意义开销。
fn should_try_mixed_decode(bytes: &[u8]) -> bool {
    if bytes.len() < 10 || bytes.len() > 1_000_000 {
        return false;
    }
    true
}

/// 将字节流按分隔符切分为片段；若分隔符不足（仅一个片段），退化使用 24 字节定长切分以便混合解码。
fn split_chunk_segments(bytes: &[u8]) -> Vec<&[u8]> {
    let mut segments: Vec<&[u8]> = Vec::new();
    let mut start = 0usize;

    for i in 0..bytes.len() {
        if is_chunk_delimiter(bytes[i]) {
            if start < i {
                segments.push(&bytes[start..i]);
            }
            segments.push(&bytes[i..i + 1]);
            start = i + 1;
        }
    }

    if start < bytes.len() {
        segments.push(&bytes[start..]);
    }

    if segments.len() <= 1 {
        return fixed_size_segments(bytes, 24);
    }
    segments
}

/// 将字节流按固定 chunk_size 切分为等长片段；输入为空或 chunk_size 为 0 时返回空。
fn fixed_size_segments(bytes: &[u8], chunk_size: usize) -> Vec<&[u8]> {
    if chunk_size == 0 || bytes.is_empty() {
        return vec![];
    }
    let mut out = Vec::new();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let end = (idx + chunk_size).min(bytes.len());
        out.push(&bytes[idx..end]);
        idx = end;
    }
    out
}

/// 判断单个字节是否为混合解码的分隔符（空格/制表/回车换行/逗号/分号/竖线/斜杠/反斜杠/冒号/点）。
fn is_chunk_delimiter(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t' | b'\r' | b'\n' | b',' | b';' | b'|' | b'/' | b'\\' | b':' | b'.'
    )
}

/// 读取 `TOKENSLIM_ENCODING_HINT` / `TOKENSLIM_FORCE_ENCODING` 环境变量，归一化后解析为强制编码提示（若有）。
fn forced_encoding_hint() -> Option<&'static Encoding> {
    let raw = std::env::var("TOKENSLIM_ENCODING_HINT")
        .or_else(|_| std::env::var("TOKENSLIM_FORCE_ENCODING"))
        .ok()?;
    let normalized = raw.trim();
    if normalized.is_empty() {
        return None;
    }
    if let Some(enc) = codepage_to_encoding(normalized) {
        return Some(enc);
    }
    Encoding::for_label(normalized.as_bytes())
}

/// 向候选编码列表追加一项；若列表中已存在相同编码则跳过，避免重复。
fn push_unique_candidate(
    candidates: &mut Vec<DecodedCandidate>,
    enc: &'static Encoding,
    source_bonus: i32,
) {
    if candidates.iter().any(|existing| existing.enc == enc) {
        return;
    }
    candidates.push(DecodedCandidate { enc, source_bonus });
}

/// 返回通用回退编码表（GB18030/GBK/BIG5/SHIFT_JIS/EUC_JP/EUC_KR 及多种 windows-125x/874/866），用于低优先级保底解码。
fn common_fallback_encodings() -> &'static [&'static Encoding] {
    static COMMON: [&Encoding; 16] = [
        GB18030,
        GBK,
        BIG5,
        SHIFT_JIS,
        EUC_JP,
        EUC_KR,
        WINDOWS_1251,
        WINDOWS_1252,
        WINDOWS_1250,
        WINDOWS_1253,
        WINDOWS_1254,
        WINDOWS_1255,
        WINDOWS_1256,
        WINDOWS_1258,
        WINDOWS_874,
        IBM866,
    ];
    &COMMON
}

struct DecodedTextMetrics {
    total: i32,
    replacement: i32,
    controls: i32,
    nuls: i32,
    mojibake_markers: i32,
    non_ascii: i32,
}

/// 统计解码文本的质量指标：总长度、替换字符、控制字符、NUL、乱码标记与非 ASCII 字符计数。
fn collect_decoded_text_metrics(text: &str) -> DecodedTextMetrics {
    let mut metrics = DecodedTextMetrics {
        total: 0,
        replacement: 0,
        controls: 0,
        nuls: 0,
        mojibake_markers: 0,
        non_ascii: 0,
    };

    for ch in text.chars() {
        metrics.total += 1;
        if ch == '\u{fffd}' {
            metrics.replacement += 1;
        }
        if ch == '\0' {
            metrics.nuls += 1;
        }
        if ch.is_control() && ch != '\n' && ch != '\r' && ch != '\t' {
            metrics.controls += 1;
        }
        if !ch.is_ascii() {
            metrics.non_ascii += 1;
        }
        if matches!(ch, 'Ã' | 'Â' | 'Ð' | 'Ñ' | 'â' | '�') {
            metrics.mojibake_markers += 1;
        }
    }

    metrics
}

/// 给一段解码文本打分；可打印字符加分、替换/控制/NUL/乱码标记扣分，存在解码错误额外扣分，非 ASCII 轻微加分，用于比较解码质量。
fn score_decoded_text(text: &str, had_errors: bool) -> i32 {
    if text.is_empty() {
        return -1000;
    }

    let metrics = collect_decoded_text_metrics(text);
    let printable = (metrics.total - metrics.controls - metrics.nuls).max(0);
    let mut score = (printable.min(400)) / 4;
    score -= metrics.replacement * 80;
    score -= metrics.controls * 25;
    score -= metrics.nuls * 60;
    score -= metrics.mojibake_markers * 3;
    if had_errors {
        score -= 120;
    }
    if metrics.non_ascii > 0 {
        score += 6;
    }
    score
}

/// 按字节序标记（BOM）解码；识别 UTF-32LE/BE（4 字节 BOM）与 UTF-16LE/BE（2 字节 BOM），解码成功后返回对应编码名。
fn decode_by_bom(bytes: &[u8]) -> Option<(String, &'static str)> {
    if bytes.len() >= 4 {
        if bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
            if let Some(decoded) = decode_utf32_endian(&bytes[4..], true) {
                return Some((decoded, "UTF-32LE"));
            }
        }
        if bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
            if let Some(decoded) = decode_utf32_endian(&bytes[4..], false) {
                return Some((decoded, "UTF-32BE"));
            }
        }
    }
    if bytes.len() >= 2 {
        if bytes.starts_with(&[0xFF, 0xFE]) {
            let (decoded, _, had_errors) = UTF_16LE.decode(&bytes[2..]);
            if !had_errors {
                return Some((decoded.into_owned(), UTF_16LE.name()));
            }
        }
        if bytes.starts_with(&[0xFE, 0xFF]) {
            let (decoded, _, had_errors) = UTF_16BE.decode(&bytes[2..]);
            if !had_errors {
                return Some((decoded.into_owned(), UTF_16BE.name()));
            }
        }
    }
    None
}

/// 对无 BOM 的字节流做 UTF-16 嗅探解码；按奇偶字节 NUL 占比判断大端/小端倾向，解码成功且分数达标、质量校验通过时返回。
fn decode_utf16_without_bom(bytes: &[u8]) -> Option<(String, &'static str)> {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
        return None;
    }

    let sample_len = bytes.len().min(4096);
    let sample = &bytes[..sample_len];
    let mut even_nul = 0usize;
    let mut odd_nul = 0usize;
    let mut even_total = 0usize;
    let mut odd_total = 0usize;
    for (i, b) in sample.iter().enumerate() {
        if i % 2 == 0 {
            even_total += 1;
            if *b == 0 {
                even_nul += 1;
            }
        } else {
            odd_total += 1;
            if *b == 0 {
                odd_nul += 1;
            }
        }
    }
    if even_total == 0 || odd_total == 0 {
        return None;
    }

    let even_ratio = even_nul as f64 / even_total as f64;
    let odd_ratio = odd_nul as f64 / odd_total as f64;
    let le_like = odd_ratio >= 0.30 && even_ratio <= 0.10;
    let be_like = even_ratio >= 0.30 && odd_ratio <= 0.10;

    if le_like {
        let (decoded, _, had_errors) = UTF_16LE.decode(bytes);
        if !had_errors
            && score_decoded_text(&decoded, false) >= 0
            && utf16_decode_quality_ok(&decoded)
        {
            return Some((decoded.into_owned(), "UTF-16LE(no-bom)"));
        }
    }
    if be_like {
        let (decoded, _, had_errors) = UTF_16BE.decode(bytes);
        if !had_errors
            && score_decoded_text(&decoded, false) >= 0
            && utf16_decode_quality_ok(&decoded)
        {
            return Some((decoded.into_owned(), "UTF-16BE(no-bom)"));
        }
    }

    None
}

/// 对无 BOM 的字节流做 UTF-32 嗅探解码；按 4 字节模位置 NUL 占比判断大端/小端倾向，解码成功且质量校验通过时返回。
fn decode_utf32_without_bom(bytes: &[u8]) -> Option<(String, &'static str)> {
    if bytes.len() < 8 || !bytes.len().is_multiple_of(4) {
        return None;
    }

    let sample_len = bytes.len().min(4096);
    let sample = &bytes[..sample_len];
    let mut idx_mod_zero = [0usize; 4];
    let mut idx_mod_total = [0usize; 4];
    for (i, b) in sample.iter().enumerate() {
        let m = i % 4;
        idx_mod_total[m] += 1;
        if *b == 0 {
            idx_mod_zero[m] += 1;
        }
    }
    let z0 = idx_mod_zero[0] as f64 / idx_mod_total[0].max(1) as f64;
    let z1 = idx_mod_zero[1] as f64 / idx_mod_total[1].max(1) as f64;
    let z2 = idx_mod_zero[2] as f64 / idx_mod_total[2].max(1) as f64;
    let z3 = idx_mod_zero[3] as f64 / idx_mod_total[3].max(1) as f64;

    let le_like = z1 >= 0.60 && z2 >= 0.60 && z3 >= 0.60 && z0 <= 0.30;
    let be_like = z0 >= 0.60 && z1 >= 0.60 && z2 >= 0.60 && z3 <= 0.30;

    if le_like {
        if let Some(decoded) = decode_utf32_endian(bytes, true) {
            if utf16_decode_quality_ok(&decoded) {
                return Some((decoded, "UTF-32LE(no-bom)"));
            }
        }
    }
    if be_like {
        if let Some(decoded) = decode_utf32_endian(bytes, false) {
            if utf16_decode_quality_ok(&decoded) {
                return Some((decoded, "UTF-32BE(no-bom)"));
            }
        }
    }
    None
}

/// 按指定端序将 UTF-32 字节解码为字符串；逐 4 字节转码并跳过 NUL，长度非 4 的倍数时返回 None。
fn decode_utf32_endian(bytes: &[u8], little_endian: bool) -> Option<String> {
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut out = String::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(4) {
        let code = if little_endian {
            u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
        } else {
            u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
        };
        if code == 0 {
            continue;
        }
        let ch = char::from_u32(code)?;
        out.push(ch);
    }
    Some(out)
}

/// 判断样本字节是否像无 BOM 的 UTF-16/UTF-32；分别按 2 字节与 4 字节模位置 NUL 占比判定（供二进制守卫豁免使用）。
fn looks_like_utf16_or_utf32_without_bom(sample: &[u8]) -> bool {
    if sample.len() >= 8 && sample.len().is_multiple_of(4) {
        let mut z = [0usize; 4];
        let mut t = [0usize; 4];
        for (i, b) in sample.iter().enumerate() {
            let m = i % 4;
            t[m] += 1;
            if *b == 0 {
                z[m] += 1;
            }
        }
        let z0 = z[0] as f64 / t[0].max(1) as f64;
        let z1 = z[1] as f64 / t[1].max(1) as f64;
        let z2 = z[2] as f64 / t[2].max(1) as f64;
        let z3 = z[3] as f64 / t[3].max(1) as f64;
        if (z1 >= 0.60 && z2 >= 0.60 && z3 >= 0.60 && z0 <= 0.30)
            || (z0 >= 0.60 && z1 >= 0.60 && z2 >= 0.60 && z3 <= 0.30)
        {
            return true;
        }
    }

    if sample.len() >= 6 && sample.len().is_multiple_of(2) {
        let mut even_zero = 0usize;
        let mut odd_zero = 0usize;
        let mut even_total = 0usize;
        let mut odd_total = 0usize;
        for (i, b) in sample.iter().enumerate() {
            if i % 2 == 0 {
                even_total += 1;
                if *b == 0 {
                    even_zero += 1;
                }
            } else {
                odd_total += 1;
                if *b == 0 {
                    odd_zero += 1;
                }
            }
        }
        let even_ratio = even_zero as f64 / even_total.max(1) as f64;
        let odd_ratio = odd_zero as f64 / odd_total.max(1) as f64;
        if (odd_ratio >= 0.30 && even_ratio <= 0.10) || (even_ratio >= 0.30 && odd_ratio <= 0.10) {
            return true;
        }
    }

    false
}

/// 移除不可见控制字符（零宽/方向格式化/BOM 等 Unicode 控制符及常规控制字符，保留换行/回车/制表/NUL），返回清理文本与移除计数。
fn strip_invisible_control_chars(input: &str) -> (String, usize) {
    let mut out = String::with_capacity(input.len());
    let mut removed = 0usize;
    for ch in input.chars() {
        let remove = matches!(
            ch,
            '\u{200B}'
                | '\u{200C}'
                | '\u{200D}'
                | '\u{2060}'
                | '\u{061C}'
                | '\u{200E}'
                | '\u{200F}'
                | '\u{202A}'
                | '\u{202B}'
                | '\u{202C}'
                | '\u{202D}'
                | '\u{202E}'
                | '\u{2066}'
                | '\u{2067}'
                | '\u{2068}'
                | '\u{2069}'
        ) || (ch.is_control() && ch != '\n' && ch != '\r' && ch != '\t' && ch != '\0');
        if remove {
            removed += 1;
        } else {
            out.push(ch);
        }
    }
    (out, removed)
}

/// 判断文本是否含 CJK 乱码特征字符（繧/繝/縺 等 Shift_JIS→UTF-8 误转产生的典型字形），用于触发 cp932 修复与评分加成。
fn contains_cjk_mojibake_signature(text: &str) -> bool {
    text.chars().any(|ch| {
        matches!(
            ch,
            '繧' | '繝' | '縺' | '縲' | '譌' | '譛' | '鬘' | '螟' | '蜈' | '逕' | '邨'
        )
    })
}

/// 校验 UTF-16 解码结果质量；采样统计可打印与控制字符比例，可打印比例足够高、控制比例足够低时视为有效解码。
fn utf16_decode_quality_ok(decoded: &str) -> bool {
    if decoded.is_empty() {
        return false;
    }

    let mut total = 0usize;
    let mut printable = 0usize;
    let mut controls = 0usize;
    for ch in decoded.chars().take(4096) {
        total += 1;
        if ch.is_control() && ch != '\n' && ch != '\r' && ch != '\t' {
            controls += 1;
        } else {
            printable += 1;
        }
    }
    if total == 0 {
        return false;
    }

    let printable_ratio = printable as f64 / total as f64;
    let control_ratio = controls as f64 / total as f64;
    printable_ratio >= 0.72 && control_ratio <= 0.12
}

/// 依据 `LANG`/`LC_ALL` 环境变量推断用户 locale 对应的提示编码（如 zh→GB18030、jp→SHIFT_JIS、ru→windows-1251 等），用于提升回退优先级。
fn locale_hint_encoding() -> Option<&'static Encoding> {
    let locale = std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .unwrap_or_default()
        .to_lowercase();
    if locale.is_empty() {
        return None;
    }

    if locale.contains("zh") || locale.contains("cn") {
        Some(GB18030)
    } else if locale.contains("tw") || locale.contains("hk") {
        Some(BIG5)
    } else if locale.contains("jp") {
        if locale.contains("euc") {
            Some(EUC_JP)
        } else {
            Some(SHIFT_JIS)
        }
    } else if locale.contains("kr") {
        Some(EUC_KR)
    } else if locale.contains("ru")
        || locale.contains("uk")
        || locale.contains("bg")
        || locale.contains("sr")
        || locale.contains("mk")
        || locale.contains("kk")
    {
        Some(WINDOWS_1251)
    } else if locale.contains("pl")
        || locale.contains("cs")
        || locale.contains("hu")
        || locale.contains("hr")
        || locale.contains("sk")
        || locale.contains("sl")
        || locale.contains("ro")
    {
        Some(WINDOWS_1250)
    } else if locale.contains("el") {
        Some(WINDOWS_1253)
    } else if locale.contains("tr") {
        Some(WINDOWS_1254)
    } else if locale.contains("he") || locale.contains("iw") {
        Some(WINDOWS_1255)
    } else if locale.contains("ar") || locale.contains("fa") || locale.contains("ur") {
        Some(WINDOWS_1256)
    } else if locale.contains("th") {
        Some(WINDOWS_874)
    } else if locale.contains("vi") {
        Some(WINDOWS_1258)
    } else if locale.contains("dos") {
        Some(IBM866)
    } else if locale.contains("latin1") || locale.contains("iso-8859-1") {
        Some(WINDOWS_1252)
    } else {
        None
    }
}

/// 探测系统首选代码页；优先读 `CHCP` 环境变量，Windows 下再尝试执行 `chcp` 命令解析活动代码页编号。
fn detect_preferred_codepage() -> Option<String> {
    if let Ok(cp_env) = std::env::var("CHCP") {
        if let Some(cp) = extract_codepage(&cp_env) {
            return Some(cp);
        }
    }

    if cfg!(windows) {
        let result = std::panic::catch_unwind(|| Command::new("cmd").args(["/C", "chcp"]).output());
        if let Ok(Ok(out)) = result {
            let raw = String::from_utf8_lossy(&out.stdout);
            if let Some(cp) = extract_codepage(&raw) {
                return Some(cp);
            }
        }
    }

    None
}

/// 从字符串中提取代码页编号；扫描连续 ASCII 数字（长度 3–6）并返回最后一个匹配（适配 "Active code page: 936" 等输出）。
fn extract_codepage(s: &str) -> Option<String> {
    let mut best: Option<String> = None;
    let mut buf = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            buf.push(ch);
        } else if !buf.is_empty() {
            if (3..=6).contains(&buf.len()) {
                best = Some(buf.clone());
            }
            buf.clear();
        }
    }
    if !buf.is_empty() && (3..=6).contains(&buf.len()) {
        best = Some(buf);
    }
    best
}

/// 归一化代码页键名；去除 `windows-`/`windows`/`cp`/`ibm` 前缀并小写，便于统一映射到编码常量。
fn normalize_codepage_key(cp: &str) -> String {
    let key = cp.trim().to_ascii_lowercase();
    if let Some(rest) = key.strip_prefix("windows-") {
        return rest.to_string();
    }
    if let Some(rest) = key.strip_prefix("windows") {
        if rest.chars().all(|c| c.is_ascii_digit()) {
            return rest.to_string();
        }
    }
    if let Some(rest) = key.strip_prefix("cp") {
        if rest.chars().all(|c| c.is_ascii_digit()) {
            return rest.to_string();
        }
    }
    if let Some(rest) = key.strip_prefix("ibm") {
        if rest.chars().all(|c| c.is_ascii_digit()) {
            return rest.to_string();
        }
    }
    key
}

/// 将代码页键（含数字编号与别名，如 936/utf-8/cp1252/windows-1251）映射到对应的 `encoding_rs` 静态编码常量；未知返回 None。
fn codepage_to_encoding(cp: &str) -> Option<&'static Encoding> {
    let key = normalize_codepage_key(cp);
    match key.as_str() {
        "utf-8" | "utf8" | "65001" => Some(UTF_8),
        "utf-16le" | "utf16le" | "1200" => Some(UTF_16LE),
        "utf-16be" | "utf16be" | "1201" => Some(UTF_16BE),
        "936" => Some(GBK),
        "54936" => Some(GB18030),
        "950" => Some(BIG5),
        "932" => Some(SHIFT_JIS),
        "949" => Some(EUC_KR),
        "20932" => Some(EUC_JP),
        "866" => Some(IBM866),
        "874" => Some(WINDOWS_874),
        "1250" => Some(WINDOWS_1250),
        "1251" => Some(WINDOWS_1251),
        "1252" => Some(WINDOWS_1252),
        "1253" => Some(WINDOWS_1253),
        "1254" => Some(WINDOWS_1254),
        "1255" => Some(WINDOWS_1255),
        "1256" => Some(WINDOWS_1256),
        "1258" => Some(WINDOWS_1258),
        _ => None,
    }
}

/// 判断给定编码名是否可**字节级可逆回写**。
///
/// P1-08 解压侧源编码回写的前置判据：压缩入口 [`decode_with_fallback`] 返回的编码名
/// 并非全部可逆，本函数把不可逆的三类显式钉死，避免解压侧产出「看似还原、实则失真」的字节。
///
/// - `"utf-8"`：恒等，可逆；
/// - `"utf-8-lossy"`：压缩入口已走 `from_utf8_lossy`，原始字节被 U+FFFD 覆盖，**不可恢复**；
/// - `"mixed-auto"`：输入为混合编码（按行/按块分别解码），单一编码回写必然失真；
/// - `"UTF-32LE"` / `"UTF-32BE"`：`encoding_rs` 无 UTF-32 编码器，无法回写；
/// - 其余（GBK/Big5/windows-1252/Shift_JIS/EUC-*/UTF-16 等）：按 `encoding_rs` 是否识别该
///   label 判定。
///
/// 契约：本函数返回 `true` 不代表回写一定成功（还可能存在无法表示的字符），
/// 最终以 [`encode_to_source_encoding`] 是否返回 `Some` 为准。
pub fn is_roundtrip_safe(encoding_name: &str) -> bool {
    match encoding_name {
        // 压缩入口未产生任何字节替换，UTF-8 文本即原始字节。
        "utf-8" => true,
        // UTF-16 由本模块手工回写（encoding_rs 的 UTF-16 输出编码退化为 UTF-8，不可用），
        // 故不依赖 for_label 判定，显式列出四种形态。
        "UTF-16LE" | "UTF-16BE" | "UTF-16LE(no-bom)" | "UTF-16BE(no-bom)" => true,
        // 有损解码：原始字节信息已丢失。
        "utf-8-lossy" => false,
        // 混合编码提升：单一回写无法还原分段差异。
        "mixed-auto" => false,
        // encoding_rs 仅提供 UTF-8/UTF-16 与大量 legacy 单/双字节编码器，无 UTF-32。
        "UTF-32LE" | "UTF-32BE" | "UTF-32LE(no-bom)" | "UTF-32BE(no-bom)" => false,
        _ => Encoding::for_label(encoding_name.as_bytes()).is_some(),
    }
}

/// 按源编码名把 UTF-8 文本重新编码为原始字节，用于解压侧字节级 round-trip 回写。
///
/// 返回 `None` 的三种情形（**均不产出部分结果**，宁可让调用方显式报错）：
/// 1. [`is_roundtrip_safe`] 判定为否（有损/混合/无编码器）；
/// 2. `encoding_rs` 无法识别该编码名；
/// 3. 存在目标编码无法表示的字符（`had_unmappable`）——回写会丢字符，属假可逆。
///
/// BOM 语义：`encoding_rs` 的 UTF-16 编码器会自行前置 BOM。本函数按解码名的大小写来源
/// 区分原始是否带 BOM——小写 `"utf-16le"`/`"utf-16be"` 来自**无 BOM 探测**路径
/// （`decode_utf16_without_bom`），回写时剥掉该 BOM；大写 `"UTF-16LE"`/`"UTF-16BE"`
/// 来自 BOM 路径，保留。
pub fn encode_to_source_encoding(text: &str, encoding_name: &str) -> Option<Vec<u8>> {
    if !is_roundtrip_safe(encoding_name) {
        return None;
    }
    if encoding_name.eq_ignore_ascii_case("utf-8") {
        return Some(text.as_bytes().to_vec());
    }

    // UTF-16 必须自行按码元序列化：`encoding_rs` 遵循 HTML 规范，UTF-16 的**输出**编码
    // 退化为 UTF-8（实测 `UTF_16LE.encode("中文")` 产出 e4 b8 ad e6 96 87，即 UTF-8 字节），
    // 交给 encoding_rs 回写会得到 UTF-8 而非原始字节，属假可逆。
    match encoding_name {
        // 大写名来自 BOM 解码路径 → 原始含 BOM → 回写补 BOM。
        "UTF-16LE" => {
            let mut out = Vec::with_capacity(2 + text.len() * 2);
            out.extend_from_slice(&[0xFF, 0xFE]);
            out.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
            return Some(out);
        }
        "UTF-16BE" => {
            let mut out = Vec::with_capacity(2 + text.len() * 2);
            out.extend_from_slice(&[0xFE, 0xFF]);
            out.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
            return Some(out);
        }
        // 无 BOM 探测路径（`(no-bom)` 后缀名与兼容的小写 label）→ 原始不含 BOM → 回写不补 BOM。
        "UTF-16LE(no-bom)" | "utf-16le" => {
            return Some(text.encode_utf16().flat_map(u16::to_le_bytes).collect());
        }
        "UTF-16BE(no-bom)" | "utf-16be" => {
            return Some(text.encode_utf16().flat_map(u16::to_be_bytes).collect());
        }
        _ => {}
    }

    let encoding = Encoding::for_label(encoding_name.as_bytes())?;
    let (bytes, _actual_encoding, had_unmappable) = encoding.encode(text);
    if had_unmappable {
        return None;
    }

    Some(bytes.into_owned())
}

#[cfg(test)]
mod tests {
    // 测试 fixture 中刻意的 mojibake 字符串（UTF-8 中文被误读为 Latin-1）含软连字符 U+00AD,
    // 属不可见字符，clippy 默认 deny；此处按测试意图允许该 lint。
    #![allow(clippy::invisible_characters)]
    use super::*;

    /// 验证 UTF-8 字节（含中文）可被 `decode_with_fallback` 正确解码且编码标记为 "utf-8"。
    #[test]
    fn test_utf8_decodes_correctly() {
        let utf8_bytes = b"Hello, \xe4\xb8\x96\xe7\x95\x8c!"; // "Hello, 世界!"
        let (decoded, enc) = decode_with_fallback(utf8_bytes);
        assert_eq!(enc, "utf-8");
        assert!(decoded.contains("世界"));
    }

    /// 验证纯 ASCII 字节被解码为 "utf-8" 且内容无损。
    #[test]
    fn test_ascii_decodes_as_utf8() {
        let ascii_bytes = b"Hello World";
        let (decoded, enc) = decode_with_fallback(ascii_bytes);
        assert_eq!(enc, "utf-8");
        assert_eq!(decoded, "Hello World");
    }

    /// 验证非法 UTF-8 的 GBK 字节能通过代码页/启发式/chardet 路径解码出中文，且编码标记非 "utf-8"。
    #[test]
    fn test_invalid_utf8_falls_back() {
        // GBK encoded bytes for "中文" (not valid UTF-8)
        let gbk_bytes: &[u8] = &[0xd6, 0xd0, 0xce, 0xc4];
        let (decoded, enc) = decode_with_fallback(gbk_bytes);
        // Should decode Chinese correctly through codepage/heuristic/chardet path.
        assert!(!decoded.is_empty());
        assert!(decoded.contains("中文"));
        assert_ne!(enc, "utf-8");
    }

    /// 验证 `extract_codepage` 能从 "Active code page: 936" 与 "65001" 提取代码页，对纯非数字 "abc" 返回 None。
    #[test]
    fn test_extract_codepage() {
        assert_eq!(
            extract_codepage("Active code page: 936"),
            Some("936".to_string())
        );
        assert_eq!(extract_codepage("65001"), Some("65001".to_string()));
        assert_eq!(extract_codepage("abc"), None);
    }

    /// 契约测试：`best_reinterpretation_candidate` 能识别并还原「UTF-8 中文被误读为 windows-1252」的乱码文本，
    /// 对正常 ASCII 文本返回 None（不误改），对空输入返回 None。
    #[test]
    fn best_reinterpretation_candidate_recovers_cp1252_mojibake_only() {
        // 用 windows-1252 解码 "世界" 的原始 UTF-8 字节 e4 b8 96 e7 95 8c，
        // 构造出真实的 cp1252 乱码字符串，保证重解释能 round-trip 还原。
        let world_utf8 = "世界";
        let mojibake = WINDOWS_1252.decode(world_utf8.as_bytes()).0;
        assert!(mojibake != "世界", "乱码应与原文不同");
        let recovered = best_reinterpretation_candidate(&mojibake, true);
        assert!(
            recovered.is_some(),
            "windows-1252 乱码应被识别并还原: {mojibake:?}"
        );
        if let Some((text, _label)) = recovered {
            assert_eq!(text, "世界", "乱码应还原为正确中文");
        }

        // 正常 ASCII 文本不应被改写（返回 None 表示无需重解释）。
        let normal = best_reinterpretation_candidate("hello world", false);
        assert!(normal.is_none(), "正常 ASCII 文本不应触发重解释");

        // 空输入不产生候选。
        assert!(best_reinterpretation_candidate("", false).is_none());
    }

    /// 验证带 BOM 的 UTF-16LE 字节被正确解码为 "UTF-16LE" 与 "Hi"。
    #[test]
    fn test_utf16le_bom_decodes_correctly() {
        let bytes = [0xFF, 0xFE, 0x48, 0x00, 0x69, 0x00];
        let (decoded, enc) = decode_with_fallback(&bytes);
        assert_eq!(enc, "UTF-16LE");
        assert_eq!(decoded, "Hi");
    }

    /// 验证无 BOM 的 UTF-16LE 字节被嗅探解码为 "UTF-16LE(no-bom)" 与 "Hi!"。
    #[test]
    fn test_utf16le_without_bom_decodes_correctly() {
        let bytes = [0x48, 0x00, 0x69, 0x00, 0x21, 0x00];
        let (decoded, enc) = decode_with_fallback(&bytes);
        assert_eq!(enc, "UTF-16LE(no-bom)");
        assert_eq!(decoded, "Hi!");
    }

    /// 验证无 BOM 的 UTF-16BE 字节被嗅探解码为 "UTF-16BE(no-bom)" 与 "Hi!"。
    #[test]
    fn test_utf16be_without_bom_decodes_correctly() {
        let bytes = [0x00, 0x48, 0x00, 0x69, 0x00, 0x21];
        let (decoded, enc) = decode_with_fallback(&bytes);
        assert_eq!(enc, "UTF-16BE(no-bom)");
        assert_eq!(decoded, "Hi!");
    }

    /// 验证带 BOM 的 UTF-32LE 字节被正确解码为 "UTF-32LE" 与 "Hi"。
    #[test]
    fn test_utf32le_bom_decodes_correctly() {
        let bytes = [
            0xFF, 0xFE, 0x00, 0x00, 0x48, 0x00, 0x00, 0x00, 0x69, 0x00, 0x00, 0x00,
        ];
        let (decoded, enc) = decode_with_fallback(&bytes);
        assert_eq!(enc, "UTF-32LE");
        assert_eq!(decoded, "Hi");
    }

    /// 验证无 BOM 的 UTF-32BE 字节被嗅探解码为 "UTF-32BE(no-bom)" 与 "Hi"。
    #[test]
    fn test_utf32be_without_bom_decodes_correctly() {
        let bytes = [0x00, 0x00, 0x00, 0x48, 0x00, 0x00, 0x00, 0x69];
        let (decoded, enc) = decode_with_fallback(&bytes);
        assert_eq!(enc, "UTF-32BE(no-bom)");
        assert_eq!(decoded, "Hi");
    }

    /// 验证代码页 1251 能将字节正确解码为俄文 "Привет" 且无解码错误。
    #[test]
    fn test_cp1251_explicit_decode_to_russian() {
        let cp1251_bytes = [0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2];
        let enc = codepage_to_encoding("1251").expect("cp1251 mapping should exist");
        let (decoded, _, had_errors) = enc.decode(&cp1251_bytes);
        assert!(!had_errors);
        assert_eq!(decoded, "Привет");
    }

    /// 验证 `codepage_to_encoding` 对 1251/1256/874/866/cp1252/windows-1251/ibm866/1200/1201 等多种键名均映射到正确的编码名。
    #[test]
    fn test_codepage_mapping_extended() {
        assert_eq!(
            codepage_to_encoding("1251")
                .map(|e| e.name())
                .map(str::to_ascii_lowercase),
            Some("windows-1251".to_string())
        );
        assert_eq!(
            codepage_to_encoding("1256")
                .map(|e| e.name())
                .map(str::to_ascii_lowercase),
            Some("windows-1256".to_string())
        );
        assert_eq!(
            codepage_to_encoding("874")
                .map(|e| e.name())
                .map(str::to_ascii_lowercase),
            Some("windows-874".to_string())
        );
        assert_eq!(
            codepage_to_encoding("866")
                .map(|e| e.name())
                .map(str::to_ascii_lowercase),
            Some("ibm866".to_string())
        );
        assert_eq!(
            codepage_to_encoding("cp1252")
                .map(|e| e.name())
                .map(str::to_ascii_lowercase),
            Some("windows-1252".to_string())
        );
        assert_eq!(
            codepage_to_encoding("windows-1251")
                .map(|e| e.name())
                .map(str::to_ascii_lowercase),
            Some("windows-1251".to_string())
        );
        assert_eq!(
            codepage_to_encoding("ibm866")
                .map(|e| e.name())
                .map(str::to_ascii_lowercase),
            Some("ibm866".to_string())
        );
        assert_eq!(
            codepage_to_encoding("1200")
                .map(|e| e.name())
                .map(str::to_ascii_lowercase),
            Some("utf-16le".to_string())
        );
        assert_eq!(
            codepage_to_encoding("1201")
                .map(|e| e.name())
                .map(str::to_ascii_lowercase),
            Some("utf-16be".to_string())
        );
    }

    /// 验证 `write_utf8` 写入文件不带 BOM 且内容与原文一致（可被 UTF-8 正确读回）。
    #[test]
    fn test_write_utf8_no_bom() {
        let dir = std::env::temp_dir().join("tokenslim-utf8-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.txt");
        let content = "Hello, 世界!";
        write_utf8(&path, content).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        // Must NOT start with BOM
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
        // Must be valid UTF-8
        let decoded = String::from_utf8(bytes).unwrap();
        assert_eq!(decoded, content);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 验证 `write_utf8` 会剥离内容中的前导 BOM 后再写入。
    #[test]
    fn test_write_utf8_strips_bom() {
        let dir = std::env::temp_dir().join("tokenslim-bom-strip-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.txt");
        let content = "\u{feff}Hello"; // Content with BOM prefix
        write_utf8(&path, content).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        // BOM should be stripped
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
        let decoded = String::from_utf8(bytes).unwrap();
        assert_eq!(decoded, "Hello");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 验证 `write_utf8_bom` 写入文件以 UTF-8 BOM 开头且 BOM 后内容为合法 UTF-8。
    #[test]
    fn test_write_utf8_bom_adds_bom() {
        let dir = std::env::temp_dir().join("tokenslim-bom-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.txt");
        let content = "Hello, 世界!";
        write_utf8_bom(&path, content).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        // Must start with BOM
        assert!(bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
        // Content after BOM must be valid UTF-8
        let decoded = String::from_utf8(bytes[3..].to_vec()).unwrap();
        assert_eq!(decoded, content);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 验证 GBK 行与 CP1251 行混合（带换行）的字节能被解码为非空，且编码结果落在 mixed-auto/GBK/GB18030/windows-1251 之一。
    #[test]
    fn test_mixed_encoding_lines_decode_correctly() {
        // line1: GBK "中文\n", line2: CP1251 "Привет\n"
        let mut bytes = vec![0xD6, 0xD0, 0xCE, 0xC4, 0x0A];
        bytes.extend_from_slice(&[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2, 0x0A]);

        let (decoded, enc) = decode_with_fallback(&bytes);
        // 全部字节在 CP1251 下也有效, 统计检测可能偏向 CP1251;
        // 同时接受 GBK/mixed-auto 或 CP1251 结果
        assert!(!decoded.is_empty(), "decoded should not be empty");
        assert!(
            enc == "mixed-auto" || enc == "GBK" || enc == "GB18030" || enc == "windows-1251",
            "unexpected encoding: {enc}"
        );
    }

    /// 验证无换行的 GBK/CP1251 混合字节能被解码为非空，编码结果落在允许集合内。
    #[test]
    fn test_mixed_encoding_without_newline_decode_correctly() {
        // token1: GBK "中文", token2: CP1251 "Привет", separated by spaces.
        let mut bytes = vec![0xD6, 0xD0, 0xCE, 0xC4, 0x20];
        bytes.extend_from_slice(&[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2, 0x20]);
        bytes.extend_from_slice(&[0xD6, 0xD0, 0xCE, 0xC4]);

        let (decoded, enc) = decode_with_fallback(&bytes);
        // 全部字节在 CP1251 下也有效, 统计检测可能偏向 CP1251
        assert!(!decoded.is_empty(), "decoded should not be empty");
        assert!(
            enc == "mixed-auto" || enc == "GBK" || enc == "GB18030" || enc == "windows-1251",
            "unexpected encoding: {enc}"
        );
    }

    /// 验证过小输入（"abc"）下 `try_promote_mixed_decode` 返回 None（不尝试混合解码）。
    #[test]
    fn test_try_promote_mixed_decode_skips_small_payload() {
        let bytes = b"abc";
        let candidates = build_decode_candidates(bytes, None);
        let best_single = decode_best_candidate(bytes, &candidates).expect("best candidate");
        let promoted = try_promote_mixed_decode(bytes, &candidates, &best_single);
        assert!(promoted.is_none());
    }

    /// 验证阿拉伯文经 1256 编码后，用 1256 强制提示能经 `decode_with_fallback_internal` 无损解码回原文。
    #[test]
    fn test_cp1256_arabic_decode_with_hint() {
        let expected = "مرحبا";
        let enc = codepage_to_encoding("1256").expect("cp1256 mapping should exist");
        let (bytes, _, had_errors) = enc.encode(expected);
        assert!(!had_errors);
        let (decoded, used_enc) = decode_with_fallback_internal(bytes.as_ref(), Some(enc));
        assert_eq!(decoded, expected);
        assert_eq!(used_enc.to_ascii_lowercase(), "windows-1256");
    }

    /// 验证希伯来文经 1255 编码后，用 1255 强制提示能无损解码回原文。
    #[test]
    fn test_cp1255_hebrew_decode_with_hint() {
        let expected = "שלום";
        let enc = codepage_to_encoding("1255").expect("cp1255 mapping should exist");
        let (bytes, _, had_errors) = enc.encode(expected);
        assert!(!had_errors);
        let (decoded, used_enc) = decode_with_fallback_internal(bytes.as_ref(), Some(enc));
        assert_eq!(decoded, expected);
        assert_eq!(used_enc.to_ascii_lowercase(), "windows-1255");
    }

    /// 验证泰文经 874 编码后，用 874 强制提示能无损解码回原文。
    #[test]
    fn test_cp874_thai_decode_with_hint() {
        let expected = "สวัสดี";
        let enc = codepage_to_encoding("874").expect("cp874 mapping should exist");
        let (bytes, _, had_errors) = enc.encode(expected);
        assert!(!had_errors);
        let (decoded, used_enc) = decode_with_fallback_internal(bytes.as_ref(), Some(enc));
        assert_eq!(decoded, expected);
        assert_eq!(used_enc.to_ascii_lowercase(), "windows-874");
    }

    /// 验证越南文经 1258 编码后，用 1258 强制提示能无损解码回原文（结果允许 windows-1258 或 mixed-auto）。
    #[test]
    fn test_cp1258_vietnamese_decode_with_hint() {
        let expected = "Xin chào Tôi";
        let enc = codepage_to_encoding("1258").expect("cp1258 mapping should exist");
        let (bytes, _, had_errors) = enc.encode(expected);
        assert!(!had_errors);
        let (decoded, used_enc) = decode_with_fallback_internal(bytes.as_ref(), Some(enc));
        assert_eq!(decoded, expected);
        assert!(
            used_enc.eq_ignore_ascii_case("windows-1258") || used_enc == "mixed-auto",
            "used_enc={used_enc}"
        );
    }

    /// 验证无候选编码时 `decode_with_candidates_or_lossy` 回退为 "utf-8-lossy" 且结果非空。
    #[test]
    fn test_decode_with_candidates_or_lossy_prefers_lossy_when_no_candidate_selected() {
        let bytes = [0xFFu8, 0xFEu8, 0x41u8];
        let decoded = decode_with_candidates_or_lossy(&bytes, &[]);
        assert_eq!(decoded.1, "utf-8-lossy");
        assert!(!decoded.0.is_empty());
    }

    /// 验证 `push_unique_candidate` 对重复编码去重：相同编码第二次入列被忽略。
    /// 契约：同一编码在候选列表中只出现一次，保证后续候选遍历不会重复解码同一编码。
    #[test]
    fn test_push_unique_candidate_dedups_identical_encoding() {
        let mut candidates: Vec<DecodedCandidate> = Vec::new();
        push_unique_candidate(&mut candidates, WINDOWS_1252, 80);
        push_unique_candidate(&mut candidates, WINDOWS_1252, 20);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].enc, WINDOWS_1252);
        // 首次入列附带的 source_bonus 保留，重复以同编码再入不改写。
        assert_eq!(candidates[0].source_bonus, 80);
    }

    /// 验证 `push_unique_candidate` 对不同编码依次追加，不互相覆盖。
    /// 契约：去重仅针对编码属性，不同编码必须全部保留且保持入列顺序。
    #[test]
    fn test_push_unique_candidate_appends_distinct_encodings_in_order() {
        let mut candidates: Vec<DecodedCandidate> = Vec::new();
        push_unique_candidate(&mut candidates, UTF_8, 80);
        push_unique_candidate(&mut candidates, WINDOWS_1252, 60);
        push_unique_candidate(&mut candidates, GBK, 20);
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].enc, UTF_8);
        assert_eq!(candidates[1].enc, WINDOWS_1252);
        assert_eq!(candidates[2].enc, GBK);
    }

    /// 验证 `build_decode_candidates` 将强制编码提示（preferred_hint）提升到候选列表首位。
    /// 契约：显式 hint 是最高优先线索（source_bonus=80），必须排在所有自动检测候选之前。
    #[test]
    fn test_build_decode_candidates_prioritizes_preferred_hint_first() {
        let candidates = build_decode_candidates(&[], Some(WINDOWS_1252));
        assert!(!candidates.is_empty());
        assert_eq!(candidates[0].enc, WINDOWS_1252);
        assert_eq!(candidates[0].source_bonus, 80);
    }

    /// 验证 `build_decode_candidates` 无 hint 时仍生成非空候选列表。
    /// 契约：即使没有强制编码提示，也必须兜底产生统计检测加上通用回退编码构成的候选集，避免解码路径空跑。
    #[test]
    fn test_build_decode_candidates_returns_fallbacks_without_hint() {
        let candidates = build_decode_candidates(&[], None);
        assert!(!candidates.is_empty());
        // 通用回退表固定包含 GBK，无 hint 且非空输入路径下应出现。
        assert!(candidates.iter().any(|c| c.enc == GBK));
    }

    /// 验证 `collect_decoded_text_metrics` 对 "A\u{0}Ã\n" 正确统计总字符数、NUL、控制字符、非 ASCII 与乱码标记。
    #[test]
    fn test_collect_decoded_text_metrics_counts_control_and_markers() {
        let m = collect_decoded_text_metrics("A\u{0}Ã\n");
        assert_eq!(m.total, 4);
        assert_eq!(m.nuls, 1);
        assert_eq!(m.controls, 1);
        assert_eq!(m.non_ascii, 1);
        assert!(m.mojibake_markers >= 1);
    }

    /// 验证典型乱码串被 `is_probable_mojibake_text` 判定为乱码。
    #[test]
    fn test_detects_probable_mojibake_text() {
        let mojibake = "Ã¤Â¸Â­Ã¦â€“â€¡";
        assert!(is_probable_mojibake_text(mojibake));
    }

    /// 验证正常多语言文本（中文/俄文/英文）不被误判为乱码。
    #[test]
    fn test_non_mojibake_text_not_flagged() {
        let normal = "中文 Привет Hello";
        assert!(!is_probable_mojibake_text(normal));
    }

    /// 验证 `repair_text_for_display` 能将 windows-1252→UTF-8 乱码链修复为 "中文"。
    #[test]
    fn test_repair_text_for_display_fixes_mojibake_chain() {
        let broken = "Ã¤Â¸Â­Ã¦â€“â€¡";
        let (fixed, steps) = repair_text_for_display(broken);
        assert_eq!(fixed, "中文");
        assert!(!steps.is_empty());
    }

    /// 验证 `repair_text_for_display` 能将 SHIFT_JIS 误转串修复回原文 "日本"，且步骤含 shift_jis/cp932 标签。
    #[test]
    fn test_repair_text_for_display_fixes_cp932_chain() {
        let original = "日本";
        let (broken, _, had_errors) = SHIFT_JIS.decode(original.as_bytes());
        assert!(!had_errors);
        let broken = broken.into_owned();
        let (fixed, steps) = repair_text_for_display(&broken);
        assert_eq!(fixed, original);
        assert!(steps
            .iter()
            .any(|s| { s.contains("shift_jis->utf8") || s.contains("windows-31j(cp932)->utf8") }));
    }

    /// 验证 `should_skip_repair_pass` 的跳过判定：无信号且无步骤时跳过，任一信号存在则不跳过。
    #[test]
    fn test_should_skip_repair_pass_when_no_signal_and_no_steps() {
        assert!(should_skip_repair_pass(false, false, true));
        assert!(!should_skip_repair_pass(true, false, true));
        assert!(!should_skip_repair_pass(false, true, true));
        assert!(!should_skip_repair_pass(false, false, false));
    }

    /// 验证跳过条件触发时 `run_mojibake_repair_pass` 返回 None。
    #[test]
    fn test_run_mojibake_repair_pass_returns_none_when_skip_triggered() {
        let pass = run_mojibake_repair_pass(1, "normal text", false, false, true);
        assert!(pass.is_none());
    }

    /// 验证 `reinterpretation_gain` 在 marker/replacement 下降且 CJK 特征消失时给出高增益（≥30）。
    #[test]
    fn test_reinterpretation_gain_prefers_marker_and_replacement_drop() {
        let gain = reinterpretation_gain(10, 12, 4, 1, 3, 1, true, "中文", true, false);
        assert!(gain >= 30, "gain={gain}");
    }

    /// 验证 `decode_and_repair_for_display` 能剥离前导 BOM 并将 CRLF 归一为 LF。
    #[test]
    fn test_decode_and_repair_strips_bom_and_normalizes_newline() {
        let bytes = [0xEF, 0xBB, 0xBF, b'a', b'\r', b'\n', b'b'];
        let (fixed, enc, steps) = decode_and_repair_for_display(&bytes);
        assert_eq!(enc, "utf-8");
        assert_eq!(fixed, "a\nb");
        assert!(steps.iter().any(|s| s == "normalize-crlf"));
    }

    /// 验证 `repair_text_for_display` 能剥离行内 BOM 与 NUL 并归一 CR 为 LF。
    #[test]
    fn test_repair_text_for_display_strips_inline_bom_and_nul() {
        let input = "a\u{feff}\0b\r";
        let (fixed, steps) = repair_text_for_display(input);
        assert_eq!(fixed, "ab\n");
        assert!(steps.iter().any(|s| s == "strip-inline-bom"));
        assert!(steps.iter().any(|s| s == "strip-nul"));
        assert!(steps.iter().any(|s| s == "normalize-cr"));
    }

    /// 验证 `repair_text_for_display` 能移除不可见控制字符（零宽/方向符）。
    #[test]
    fn test_repair_text_for_display_strips_invisible_controls() {
        let input = "A\u{200B}\u{202E}B";
        let (fixed, steps) = repair_text_for_display(input);
        assert_eq!(fixed, "AB");
        assert!(steps
            .iter()
            .any(|s| s.starts_with("strip-invisible-controls:")));
    }

    /// 验证 `normalize_display_text` 能正确提取前导 BOM 剥离、CRLF/CR 归一、行内 BOM、不可见控制字符与 NUL 等清理步骤。
    #[test]
    fn test_normalize_display_text_extracts_basic_cleanup_steps() {
        let input = "\u{feff}a\r\nb\u{feff}\0\u{200b}c\r";
        let (normalized, steps) = normalize_display_text(input);
        assert_eq!(normalized, "a\nbc\n");
        assert!(steps.iter().any(|s| s == "strip-leading-bom"));
        assert!(steps.iter().any(|s| s == "normalize-crlf"));
        assert!(steps.iter().any(|s| s == "normalize-cr"));
        assert!(steps.iter().any(|s| s == "strip-inline-bom"));
        assert!(steps
            .iter()
            .any(|s| s.starts_with("strip-invisible-controls:")));
        assert!(steps.iter().any(|s| s == "strip-nul"));
    }

    /// 验证 `try_direct_unicode_decoding` 对合法 UTF-8 字节直接返回 "utf-8" 且内容正确。
    #[test]
    fn test_try_direct_unicode_decoding_prefers_utf8_path() {
        let bytes = "Hello, 世界".as_bytes();
        let (decoded, enc) = try_direct_unicode_decoding(bytes).expect("direct decode");
        assert_eq!(enc, "utf-8");
        assert_eq!(decoded, "Hello, 世界");
    }

    /// 验证 `is_probable_binary_bytes` 能将 ELF 类二进制样本判定为二进制。
    #[test]
    fn test_binary_guard_detects_binary_like_bytes() {
        let bytes = [0x7F, b'E', b'L', b'F', 0x00, 0x01, 0x02, 0x00, 0x03];
        assert!(is_probable_binary_bytes(&bytes));
    }

    /// 验证 `is_probable_binary_bytes` 不会把无 BOM 的 UTF-16 文本误判为二进制。
    #[test]
    fn test_binary_guard_does_not_misclassify_utf16_text() {
        let bytes = [0x48, 0x00, 0x69, 0x00, 0x21, 0x00];
        assert!(!is_probable_binary_bytes(&bytes));
    }

    /// 验证 `decode_and_repair_for_display` 对二进制载荷跳过修复，并记录 "binary-guard-skip-repair" 步骤。
    #[test]
    fn test_decode_and_repair_skips_binary_payload() {
        let bytes = [0x7F, b'E', b'L', b'F', 0x00, 0x01, 0x02, 0x00, 0x03];
        let (_, _, steps) = decode_and_repair_for_display(&bytes);
        assert!(steps.iter().any(|s| s == "binary-guard-skip-repair"));
    }

    /// 验证乱码标记下降时 `evaluate_repair_confidence` 给出 "high" 等级且证据含 mojibake-markers。
    #[test]
    fn test_evaluate_repair_confidence_high_when_markers_drop() {
        let original = "Ã¤Â¸Â­Ã¦â€“â€¡";
        let repaired = "中文";
        let steps = vec!["mojibake-repair-pass-1:windows-1252->utf8".to_string()];
        let (level, evidence) = evaluate_repair_confidence(original, repaired, &steps);
        assert_eq!(level, "high");
        assert!(evidence.iter().any(|x| x.contains("mojibake-markers")));
    }

    /// 验证原文未变且无步骤时 `evaluate_repair_confidence` 给出 "low" 等级且证据含 content-changed=false。
    #[test]
    fn test_evaluate_repair_confidence_low_when_unchanged() {
        let original = "normal text";
        let repaired = "normal text";
        let steps: Vec<String> = Vec::new();
        let (level, evidence) = evaluate_repair_confidence(original, repaired, &steps);
        assert_eq!(level, "low");
        assert!(evidence.iter().any(|x| x.contains("content-changed=false")));
    }

    /// 契约测试：`score_decoded_text` 对空串给最低分；干净文本为正分；
    /// 替换符 U+FFFD 会扣重分；had_errors 带固定罚分；含非 ASCII 获小幅加成。
    #[test]
    fn test_score_decoded_text_contract() {
        assert_eq!(score_decoded_text("", false), -1000, "空串应给最低分");

        let clean = score_decoded_text("hello world and some more padding", false);
        assert!(clean > 0, "干净文本应为正分，实际 {clean}");

        let with_replacement = score_decoded_text("a\u{fffd}b\u{fffd}c", false);
        assert!(
            with_replacement < clean,
            "含 U+FFFD 替换符文本分数应显著低于干净文本"
        );

        let no_error = score_decoded_text("normal text content", false);
        let with_error = score_decoded_text("normal text content", true);
        assert!(
            with_error < no_error,
            "had_errors 应施加罚分（all={no_error} vs {with_error}）"
        );

        let ascii = score_decoded_text("plain ascii", false);
        let unicode = score_decoded_text("世界 peace", false);
        assert!(unicode > ascii, "含非 ASCII 应获得小幅加成");
    }

    /// P1-08：`is_roundtrip_safe` 必须把三类不可逆编码钉死，并识别可逆编码。
    #[test]
    fn p1_08_is_roundtrip_safe_marks_irreversible_names() {
        assert!(is_roundtrip_safe("utf-8"), "utf-8 恒等应可逆");
        assert!(is_roundtrip_safe("GBK"), "GBK 应可逆");
        assert!(is_roundtrip_safe("Big5"), "Big5 应可逆");
        assert!(is_roundtrip_safe("windows-1252"), "windows-1252 应可逆");
        assert!(is_roundtrip_safe("UTF-16LE"), "UTF-16LE 应可逆");
        assert!(
            is_roundtrip_safe("UTF-16LE(no-bom)"),
            "无 BOM UTF-16LE 应可逆"
        );
        assert!(!is_roundtrip_safe("UTF-32LE(no-bom)"), "UTF-32 无编码器");
        // 三类不可逆：有损替换 / 混合编码 / 无编码器
        assert!(!is_roundtrip_safe("utf-8-lossy"), "有损解码不可逆");
        assert!(!is_roundtrip_safe("mixed-auto"), "混合编码不可逆");
        assert!(!is_roundtrip_safe("UTF-32LE"), "UTF-32 无编码器");
        assert!(!is_roundtrip_safe("UTF-32BE"), "UTF-32 无编码器");
    }

    /// P1-08：legacy 单/双字节编码回写必须与 `encoding_rs` 编码输出逐字节一致。
    #[test]
    fn p1_08_encode_to_source_encoding_restores_legacy_bytes() {
        let text = "2026-09-10 12:00:00 [ERROR] db connect failed\n";
        for enc in [GBK, BIG5, WINDOWS_1252, SHIFT_JIS] {
            let (bytes, _, unmappable) = enc.encode(text);
            assert!(!unmappable, "{} 应能表示纯 ASCII 测试文本", enc.name());
            let restored = encode_to_source_encoding(text, enc.name())
                .unwrap_or_else(|| panic!("{} 回写应成功", enc.name()));
            assert_eq!(
                restored,
                bytes.as_ref(),
                "{} 回写字节应与编码输出逐字节一致",
                enc.name()
            );
        }
    }

    /// P1-08：存在无法表示的字符时**拒绝回写**——宁可报错也不产出丢字符的假可逆字节。
    #[test]
    fn p1_08_encode_to_source_encoding_rejects_unmappable_chars() {
        assert!(
            encode_to_source_encoding("数据库连接失败", "windows-1252").is_none(),
            "中文在 windows-1252 不可表示，应拒绝回写"
        );
        assert!(
            encode_to_source_encoding("deploy 🚀 done", "GBK").is_none(),
            "emoji 在 GBK 不可表示，应拒绝回写"
        );
        assert!(
            encode_to_source_encoding("any text", "utf-8-lossy").is_none(),
            "不可逆编码名应一律拒绝回写"
        );
    }

    /// P1-08：UTF-16 BOM 语义定性——带 BOM 输入回写保留 BOM，无 BOM 输入回写不补 BOM。
    /// 本测试同时实证 `encoding_rs` 的 UTF-16 编码器是否自动前置 BOM。
    #[test]
    fn p1_08_encode_to_source_encoding_utf16_bom_semantics() {
        let text = "中文 mixed ascii log line\n";
        // 手工构造 UTF-16LE 样本：`encoding_rs` 的 UTF-16 输出编码按规范退化为 UTF-8，
        // 不能用其 `encode` 造 UTF-16 字节（实测产出 e4 b8 ad…，即 UTF-8）。
        let units: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let with_bom: Vec<u8> = [0xFF, 0xFE].iter().copied().chain(units.clone()).collect();

        // 有 BOM：decode 走 BOM 路径，回写应逐字节还原（含 BOM）
        let (decoded, enc) = decode_with_fallback(&with_bom);
        assert_eq!(enc, UTF_16LE.name(), "带 BOM UTF-16LE 应命中 BOM 解码路径");
        let restored =
            encode_to_source_encoding(&decoded, enc).unwrap_or_else(|| panic!("带 BOM 回写应成功"));
        assert_eq!(restored, with_bom, "带 BOM 输入回写应逐字节一致");

        // 无 BOM：decode 走探测路径（小写名），回写不得补 BOM
        let nobom = &units;
        let (decoded_nb, enc_nb) = decode_with_fallback(nobom);
        assert_eq!(
            enc_nb, "UTF-16LE(no-bom)",
            "无 BOM UTF-16LE 应命中无 BOM 探测路径"
        );
        let restored_nb = encode_to_source_encoding(&decoded_nb, enc_nb)
            .unwrap_or_else(|| panic!("无 BOM 回写应成功"));
        assert_eq!(restored_nb, *nobom, "无 BOM 输入回写不应额外补 BOM");
    }
}
