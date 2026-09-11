//! noise filter plugin 方法实现

use super::types::*;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use aho_corasick::AhoCorasick;
use bumpalo::Bump;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

// 长 hex 掩码正则（最低长度 32 = 原 NoiseFilterConfig.hex_threshold 的编译期常量形态。
// P2-49 收口：hex_threshold 字段零读取且外部注入链（Plugin::load_config）零调用，属假调参，
// 已删除字段改为此处文档化的编译期常量——保留"32"语义而非凭空消失。）
static HEX_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b[0-9a-fA-F]{32,}\b").unwrap());
static NOISE_LINE_PREFIXES: &[&str] = &[
    "Generating",
    "Copying",
    "Checking",
    "Installing",
    "Skiped",
    "skipping",
];
static AC_NOISE: Lazy<AhoCorasick> = Lazy::new(|| AhoCorasick::new(NOISE_LINE_PREFIXES).unwrap());

impl NoiseFilterPlugin {
    /// 创建 NoiseFilterPlugin 实例（名称 noise_filter，优先级 200，默认配置）。
    pub fn new() -> Self {
        NoiseFilterPlugin {
            name: "noise_filter",
            priority: 200,
            config: NoiseFilterConfig::default(),
        }
    }

    /// 清理进度条行：取最后一个 \r 后的最终重绘内容；空则返回 None 表示丢弃该行。
    fn clean_progress_line<'a>(&self, line: &'a str, arena: &'a Bump) -> Option<Cow<'a, str>> {
        if !self.config.clean_progress_bars {
            return Some(Cow::Borrowed(line));
        }

        // Use memchr to find \r efficiently
        if let Some(pos) = memchr::memrchr(b'\r', line.as_bytes()) {
            let last = line[pos..].trim_start_matches('\r').trim();
            if last.is_empty() {
                return None;
            }
            return Some(Cow::Borrowed(arena.alloc_str(last)));
        }
        Some(Cow::Borrowed(line))
    }
}

impl Plugin for NoiseFilterPlugin {
    /// 返回插件名称 "noise_filter"。
    fn name(&self) -> &'static str {
        self.name
    }
    /// 返回插件优先级 200。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：前 1KB 含控制字符判二进制（1.0）、裸 \r 重绘进度或"数字%"得 0.5~0.7、
    /// 噪声前缀行占比、长 hex 得 0.6。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let text = slice.text.as_ref();

        // Fast binary detection using memchr to find control characters (except common ones)
        // This is a heuristic: check first 1024 bytes
        let bytes = text.as_bytes();
        let check_len = bytes.len().min(1024);
        for &b in &bytes[..check_len] {
            if b < 32 && b != 10 && b != 13 && b != 9 {
                return Some(1.0);
            }
        }

        // 进度/重绘噪声证据判定。注意：CRLF（\r\n）是常见行尾，本身不代表噪声——
        // 若把"文本含 \r"直接判为 0.95，任何 CRLF 日志都会被误判为噪声，并在
        // 插件选择打分中霸榜（如 vcs 的 git merge 冲突样例被 noise_filter 抢占）。
        // 因此仅当存在真实的进度条重绘证据：
        //   - 裸 \r 重绘：\r 后紧跟非换行内容（光标回退重绘进度条）→ 0.7
        //   - "数字%" 占位进度文字 → 0.5（弱证据，不覆盖更具体的内容检测器）
        if memchr::memchr(b'\r', bytes).is_some() {
            let mut has_redraw = false;
            for (i, &b) in bytes.iter().enumerate() {
                if b == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
                    has_redraw = true;
                    break;
                }
            }
            if has_redraw {
                return Some(0.7);
            }
        }
        // 进度文字弱证据：% 前一字符为数字（如 "10%..."）视为进度，而非权限/占比等普通文本。
        if let Some(pct_pos) = memchr::memchr(b'%', bytes) {
            if pct_pos > 0 && bytes[pct_pos - 1].is_ascii_digit() {
                return Some(0.5);
            }
        }

        let mut noise_lines = 0;
        let lines: Vec<&str> = text.lines().collect();
        for line in &lines {
            // Aho-Corasick for fast prefix matching
            if AC_NOISE.find(line).map(|m| m.start() == 0).unwrap_or(false) {
                noise_lines += 1;
            }
        }
        if noise_lines > 0 {
            let score = (noise_lines as f32 / lines.len() as f32).max(0.4);
            return Some(score);
        }
        if HEX_RE.is_match(text) {
            return Some(0.6);
        }
        None
    }

    /// 压缩切片：二进制数据折叠为 MD5 标记；进度条清理、长 hex 掩码、重复噪声折叠。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        arena: &'a Bump,
    ) -> CompressResult<'a> {
        let text = slice.text.as_ref();
        let bytes = text.as_bytes();

        // Binary data check
        let is_binary = bytes
            .iter()
            .any(|&b| b < 32 && b != 10 && b != 13 && b != 9);

        if is_binary {
            let hash = format!("{:x}", md5::compute(bytes));
            let marker = bumpalo::format!(in arena, "[BINARY_DATA: Size={}B, MD5={}]", text.len(), &hash[..8]);
            return CompressResult {
                tokens: vec![Token::Text(Cow::Borrowed(marker.into_bump_str()))],
                metadata: None,
                plugin_name: Some(self.name()),
            };
        }

        let mut result = bumpalo::collections::String::new_in(arena);
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            let cleaned = match self.clean_progress_line(line, arena) {
                Some(c) => c,
                None => {
                    i += 1;
                    continue;
                }
            };

            let final_line: Cow<'a, str> = if self.config.mask_long_hex {
                let mut replaced = false;
                let new_s = HEX_RE.replace_all(&cleaned, |caps: &regex::Captures| {
                    replaced = true;
                    let hex = caps.get(0).unwrap().as_str();
                    if hex.len() > 128 {
                        format!("[HEX_DUMP: {} chars]", hex.len())
                    } else {
                        dict_engine.add_path_layered(hex)
                    }
                });
                if replaced {
                    Cow::Borrowed(arena.alloc_str(&new_s))
                } else {
                    cleaned
                }
            } else {
                cleaned
            };

            if self.config.fold_repetitive_noise
                && AC_NOISE
                    .find(&*final_line)
                    .map(|m| m.start() == 0)
                    .unwrap_or(false)
            {
                let mut count = 1;
                while i + count < lines.len() {
                    let next_line = lines[i + count];
                    if AC_NOISE
                        .find(next_line)
                        .map(|m| m.start() == 0)
                        .unwrap_or(false)
                    {
                        count += 1;
                    } else {
                        break;
                    }
                }
                if count > 5 {
                    result.push_str(bumpalo::format!(in arena, "{} (and {} similar lines suppressed)\n", final_line, count - 1).into_bump_str());
                    i += count;
                    continue;
                }
            }

            result.push_str(&final_line);
            result.push('\n');
            i += 1;
        }

        CompressResult {
            tokens: vec![Token::Text(Cow::Borrowed(result.into_bump_str()))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 解压：原文透传。
    fn decompress(&self, compressed: &str, _dict: &Dictionary) -> String {
        compressed.to_string()
    }

    /// 归一化：将进度条 [N%] 替换为统一 [...]% 形态（用于 diff 比对）。
    fn normalize(&self, text: &str) -> String {
        let progress_re = regex::Regex::new(r"\[\s*\d+%\s*\]").unwrap();
        progress_re.replace_all(text, "[...%]").into_owned()
    }
}
