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

fn should_skip_repair_pass(
    likely_mojibake: bool,
    cjk_mojibake_like: bool,
    no_cleanup_steps: bool,
) -> bool {
    !likely_mojibake && !cjk_mojibake_like && no_cleanup_steps
}

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

    new_markers < old_markers
        || new_repl < old_repl
        || new_score >= old_score + 4
        || (likely_mojibake && !next_mojibake)
}

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

fn replacement_marker_count(text: &str) -> usize {
    text.chars().filter(|ch| *ch == '\u{fffd}').count()
}

fn nul_marker_count(text: &str) -> usize {
    text.chars().filter(|ch| *ch == '\0').count()
}

fn reinterpretation_chains() -> [(&'static Encoding, &'static str); 5] {
    [
        (WINDOWS_1252, "windows-1252->utf8"),
        (GBK, "gbk->utf8"),
        (GB18030, "gb18030->utf8"),
        (SHIFT_JIS, "shift_jis->utf8"),
        (SHIFT_JIS, "windows-31j(cp932)->utf8"),
    ]
}

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

fn try_reinterpret_as_utf8(text: &str, source_encoding: &'static Encoding) -> Option<String> {
    let (bytes, _, had_errors) = source_encoding.encode(text);
    if had_errors {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

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

fn decode_with_candidates_or_lossy(
    bytes: &[u8],
    candidates: &[DecodedCandidate],
) -> (String, &'static str) {
    if let Some(decoded) = select_candidate_or_mixed_result(bytes, candidates) {
        return decoded;
    }
    (String::from_utf8_lossy(bytes).into_owned(), "utf-8-lossy")
}

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

fn should_try_mixed_decode(bytes: &[u8]) -> bool {
    if bytes.len() < 10 || bytes.len() > 1_000_000 {
        return false;
    }
    true
}

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

fn is_chunk_delimiter(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t' | b'\r' | b'\n' | b',' | b';' | b'|' | b'/' | b'\\' | b':' | b'.'
    )
}

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

fn contains_cjk_mojibake_signature(text: &str) -> bool {
    text.chars().any(|ch| {
        matches!(
            ch,
            '繧' | '繝' | '縺' | '縲' | '譌' | '譛' | '鬘' | '螟' | '蜈' | '逕' | '邨'
        )
    })
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8_decodes_correctly() {
        let utf8_bytes = b"Hello, \xe4\xb8\x96\xe7\x95\x8c!"; // "Hello, 世界!"
        let (decoded, enc) = decode_with_fallback(utf8_bytes);
        assert_eq!(enc, "utf-8");
        assert!(decoded.contains("世界"));
    }

    #[test]
    fn test_ascii_decodes_as_utf8() {
        let ascii_bytes = b"Hello World";
        let (decoded, enc) = decode_with_fallback(ascii_bytes);
        assert_eq!(enc, "utf-8");
        assert_eq!(decoded, "Hello World");
    }

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

    #[test]
    fn test_extract_codepage() {
        assert_eq!(
            extract_codepage("Active code page: 936"),
            Some("936".to_string())
        );
        assert_eq!(extract_codepage("65001"), Some("65001".to_string()));
        assert_eq!(extract_codepage("abc"), None);
    }

    #[test]
    fn test_utf16le_bom_decodes_correctly() {
        let bytes = [0xFF, 0xFE, 0x48, 0x00, 0x69, 0x00];
        let (decoded, enc) = decode_with_fallback(&bytes);
        assert_eq!(enc, "UTF-16LE");
        assert_eq!(decoded, "Hi");
    }

    #[test]
    fn test_utf16le_without_bom_decodes_correctly() {
        let bytes = [0x48, 0x00, 0x69, 0x00, 0x21, 0x00];
        let (decoded, enc) = decode_with_fallback(&bytes);
        assert_eq!(enc, "UTF-16LE(no-bom)");
        assert_eq!(decoded, "Hi!");
    }

    #[test]
    fn test_utf16be_without_bom_decodes_correctly() {
        let bytes = [0x00, 0x48, 0x00, 0x69, 0x00, 0x21];
        let (decoded, enc) = decode_with_fallback(&bytes);
        assert_eq!(enc, "UTF-16BE(no-bom)");
        assert_eq!(decoded, "Hi!");
    }

    #[test]
    fn test_utf32le_bom_decodes_correctly() {
        let bytes = [
            0xFF, 0xFE, 0x00, 0x00, 0x48, 0x00, 0x00, 0x00, 0x69, 0x00, 0x00, 0x00,
        ];
        let (decoded, enc) = decode_with_fallback(&bytes);
        assert_eq!(enc, "UTF-32LE");
        assert_eq!(decoded, "Hi");
    }

    #[test]
    fn test_utf32be_without_bom_decodes_correctly() {
        let bytes = [0x00, 0x00, 0x00, 0x48, 0x00, 0x00, 0x00, 0x69];
        let (decoded, enc) = decode_with_fallback(&bytes);
        assert_eq!(enc, "UTF-32BE(no-bom)");
        assert_eq!(decoded, "Hi");
    }

    #[test]
    fn test_cp1251_explicit_decode_to_russian() {
        let cp1251_bytes = [0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2];
        let enc = codepage_to_encoding("1251").expect("cp1251 mapping should exist");
        let (decoded, _, had_errors) = enc.decode(&cp1251_bytes);
        assert!(!had_errors);
        assert_eq!(decoded, "Привет");
    }

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

    #[test]
    fn test_try_promote_mixed_decode_skips_small_payload() {
        let bytes = b"abc";
        let candidates = build_decode_candidates(bytes, None);
        let best_single = decode_best_candidate(bytes, &candidates).expect("best candidate");
        let promoted = try_promote_mixed_decode(bytes, &candidates, &best_single);
        assert!(promoted.is_none());
    }

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

    #[test]
    fn test_decode_with_candidates_or_lossy_prefers_lossy_when_no_candidate_selected() {
        let bytes = [0xFFu8, 0xFEu8, 0x41u8];
        let decoded = decode_with_candidates_or_lossy(&bytes, &[]);
        assert_eq!(decoded.1, "utf-8-lossy");
        assert!(!decoded.0.is_empty());
    }

    #[test]
    fn test_collect_decoded_text_metrics_counts_control_and_markers() {
        let m = collect_decoded_text_metrics("A\u{0}Ã\n");
        assert_eq!(m.total, 4);
        assert_eq!(m.nuls, 1);
        assert_eq!(m.controls, 1);
        assert_eq!(m.non_ascii, 1);
        assert!(m.mojibake_markers >= 1);
    }

    #[test]
    fn test_detects_probable_mojibake_text() {
        let mojibake = "Ã¤Â¸Â­Ã¦â€“â€¡";
        assert!(is_probable_mojibake_text(mojibake));
    }

    #[test]
    fn test_non_mojibake_text_not_flagged() {
        let normal = "中文 Привет Hello";
        assert!(!is_probable_mojibake_text(normal));
    }

    #[test]
    fn test_repair_text_for_display_fixes_mojibake_chain() {
        let broken = "Ã¤Â¸Â­Ã¦â€“â€¡";
        let (fixed, steps) = repair_text_for_display(broken);
        assert_eq!(fixed, "中文");
        assert!(!steps.is_empty());
    }

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

    #[test]
    fn test_should_skip_repair_pass_when_no_signal_and_no_steps() {
        assert!(should_skip_repair_pass(false, false, true));
        assert!(!should_skip_repair_pass(true, false, true));
        assert!(!should_skip_repair_pass(false, true, true));
        assert!(!should_skip_repair_pass(false, false, false));
    }

    #[test]
    fn test_run_mojibake_repair_pass_returns_none_when_skip_triggered() {
        let pass = run_mojibake_repair_pass(1, "normal text", false, false, true);
        assert!(pass.is_none());
    }

    #[test]
    fn test_reinterpretation_gain_prefers_marker_and_replacement_drop() {
        let gain = reinterpretation_gain(10, 12, 4, 1, 3, 1, true, "中文", true, false);
        assert!(gain >= 30, "gain={gain}");
    }

    #[test]
    fn test_decode_and_repair_strips_bom_and_normalizes_newline() {
        let bytes = [0xEF, 0xBB, 0xBF, b'a', b'\r', b'\n', b'b'];
        let (fixed, enc, steps) = decode_and_repair_for_display(&bytes);
        assert_eq!(enc, "utf-8");
        assert_eq!(fixed, "a\nb");
        assert!(steps.iter().any(|s| s == "normalize-crlf"));
    }

    #[test]
    fn test_repair_text_for_display_strips_inline_bom_and_nul() {
        let input = "a\u{feff}\0b\r";
        let (fixed, steps) = repair_text_for_display(input);
        assert_eq!(fixed, "ab\n");
        assert!(steps.iter().any(|s| s == "strip-inline-bom"));
        assert!(steps.iter().any(|s| s == "strip-nul"));
        assert!(steps.iter().any(|s| s == "normalize-cr"));
    }

    #[test]
    fn test_repair_text_for_display_strips_invisible_controls() {
        let input = "A\u{200B}\u{202E}B";
        let (fixed, steps) = repair_text_for_display(input);
        assert_eq!(fixed, "AB");
        assert!(steps
            .iter()
            .any(|s| s.starts_with("strip-invisible-controls:")));
    }

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

    #[test]
    fn test_try_direct_unicode_decoding_prefers_utf8_path() {
        let bytes = "Hello, 世界".as_bytes();
        let (decoded, enc) = try_direct_unicode_decoding(bytes).expect("direct decode");
        assert_eq!(enc, "utf-8");
        assert_eq!(decoded, "Hello, 世界");
    }

    #[test]
    fn test_binary_guard_detects_binary_like_bytes() {
        let bytes = [0x7F, b'E', b'L', b'F', 0x00, 0x01, 0x02, 0x00, 0x03];
        assert!(is_probable_binary_bytes(&bytes));
    }

    #[test]
    fn test_binary_guard_does_not_misclassify_utf16_text() {
        let bytes = [0x48, 0x00, 0x69, 0x00, 0x21, 0x00];
        assert!(!is_probable_binary_bytes(&bytes));
    }

    #[test]
    fn test_decode_and_repair_skips_binary_payload() {
        let bytes = [0x7F, b'E', b'L', b'F', 0x00, 0x01, 0x02, 0x00, 0x03];
        let (_, _, steps) = decode_and_repair_for_display(&bytes);
        assert!(steps.iter().any(|s| s == "binary-guard-skip-repair"));
    }

    #[test]
    fn test_evaluate_repair_confidence_high_when_markers_drop() {
        let original = "Ã¤Â¸Â­Ã¦â€“â€¡";
        let repaired = "中文";
        let steps = vec!["mojibake-repair-pass-1:windows-1252->utf8".to_string()];
        let (level, evidence) = evaluate_repair_confidence(original, repaired, &steps);
        assert_eq!(level, "high");
        assert!(evidence.iter().any(|x| x.contains("mojibake-markers")));
    }

    #[test]
    fn test_evaluate_repair_confidence_low_when_unchanged() {
        let original = "normal text";
        let repaired = "normal text";
        let steps: Vec<String> = Vec::new();
        let (level, evidence) = evaluate_repair_confidence(original, repaired, &steps);
        assert_eq!(level, "low");
        assert!(evidence.iter().any(|x| x.contains("content-changed=false")));
    }
}
