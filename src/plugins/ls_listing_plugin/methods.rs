//! ls_listing 插件方法实现。

use super::types::LsListingPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::{Dictionary, DictionaryEngine};
use crate::core::plugin_dispatcher::{CompressResult, Plugin};
use crate::core::text_slicer::Slice;
use crate::plugins::infra_tools_common::decompress_with_dict;
use bumpalo::Bump;
use regex::Regex;
use std::borrow::Cow;
use std::sync::OnceLock;

impl LsListingPlugin {
    /// 创建 LsListingPlugin 实例（名称 ls_listing，优先级 165）。
    pub fn new() -> Self {
        Self {
            name: "ls_listing",
            priority: 165,
        }
    }
}

impl Plugin for LsListingPlugin {
    /// 返回插件名称 "ls_listing"。
    fn name(&self) -> &'static str {
        self.name
    }

    /// 返回插件优先级 165。
    fn priority(&self) -> u8 {
        self.priority
    }

    /// 检测：切片内「`YYYY-MM-DD HH:MM:SS <右对齐尺寸> <路径>`」列式行
    /// ≥ [`LS_DETECT_MIN_MATCHES`] 行且占非空行 ≥50% 时命中（0.9）。
    ///
    /// 口径刻意收窄（设计稿 §四 约束）：ISO 日期 + 时间 + 右对齐数字列 + 路径
    /// 四元组同时成立才算列式清单行——`syslog`（英文月名）、`web_log`（无右对齐
    /// 尺寸列）、`git status` / `ps aux`（无该列结构）均不匹配，避免与
    /// `shell_session`（无结构终端兜底）抢样本。
    fn detect<'a>(&self, slice: &'a Slice<'a>) -> Option<f32> {
        let re = listing_line_re();
        let mut matches = 0usize;
        let mut non_empty = 0usize;
        for line in slice.text.lines() {
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                continue;
            }
            non_empty += 1;
            if re.is_match(trimmed) {
                matches += 1;
            }
        }
        if matches >= LS_DETECT_MIN_MATCHES && non_empty > 0 && matches * 2 >= non_empty {
            Some(0.9)
        } else {
            None
        }
    }

    /// 压缩切片：逐行规约列式清单行的对齐填充（日期时间、尺寸、完整路径
    /// 逐条保留），杂散行原样保留；无收益时回退原文。
    fn compress<'a>(
        &self,
        slice: &'a Slice<'a>,
        _dict_engine: &mut DictionaryEngine,
        _dedup_engine: &mut DedupEngine,
        _arena: &'a Bump,
    ) -> CompressResult<'a> {
        let raw = slice.text.as_ref();
        let cleaned = crate::core::utils::strip_ansi(raw);
        let compacted = compact_listing(&cleaned);
        let final_text = crate::core::utils::roi::prefer_non_expanding(raw, compacted);
        CompressResult {
            tokens: vec![Token::Text(Cow::Owned(final_text))],
            metadata: None,
            plugin_name: Some(self.name()),
        }
    }

    /// 解压：本插件为骨架化口径（对齐填充不可逆），仅还原可能存在的字典 token。
    fn decompress(&self, compressed: &str, dict: &Dictionary) -> String {
        decompress_with_dict(compressed, dict)
    }
}

/// detect/折叠共用的最小列式行数（低于此视为普通文本，不认领）。
const LS_DETECT_MIN_MATCHES: usize = 5;

/// 列式清单行的共享判定：`YYYY-MM-DD HH:MM:SS` + 右对齐尺寸（十进制）+ 路径。
///
/// 供 [`LsListingPlugin::detect`] 与 [`compact_listing`] 复用，避免两处口径漂移。
fn listing_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\s+\d+\s+\S").unwrap())
}

/// 核心压缩逻辑（设计稿 §8.3 选项 1：纯填充规约）：锚点行（首个非空行，通常是
/// 命令行）原样保留；其后逐条列式清单行归一化为 `YYYY-MM-DD HH:MM:SS size path`
/// 单空格分隔——日期时间与尺寸数值逐条完整保留（语义门禁 rule 4 口径），路径
/// 全量保留、无需任何字典即可解析（rule 5/7 口径），仅规约右对齐填充空格。
/// 杂散行原样保留；列式行不足 [`LS_DETECT_MIN_MATCHES`] 时回退原文。
fn compact_listing(text: &str) -> String {
    static ENTRY_RE: OnceLock<Regex> = OnceLock::new();
    let entry_re = ENTRY_RE.get_or_init(|| {
        Regex::new(
            r"^(?P<date>\d{4}-\d{2}-\d{2}) (?P<time>\d{2}:\d{2}:\d{2})\s+(?P<size>\d+)\s+(?P<path>\S.*)$",
        )
        .unwrap()
    });

    let mut out: Vec<String> = Vec::new();
    let mut entries: Vec<String> = Vec::new();
    let mut matched = 0usize;
    let mut stray: Vec<String> = Vec::new();
    let mut seen_anchor = false;

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if !seen_anchor {
            // 锚点行：首个非空行原样保留（通常是命令行）。
            seen_anchor = true;
            out.push(trimmed.to_string());
            continue;
        }
        match entry_re.captures(trimmed) {
            Some(caps) => {
                matched += 1;
                // "YYYY-MM-DD HH:MM:SS"（前 19 字节）+ 尺寸 + 完整路径，逐条保留。
                let datetime = &trimmed[..19];
                let size = caps.name("size").unwrap().as_str();
                let path = caps.name("path").unwrap().as_str().trim_end();
                entries.push(format!("{datetime} {size} {path}"));
            }
            None => stray.push(trimmed.to_string()),
        }
    }

    // 不像列式清单（记录太少）→ 回退原文（ROI 门控之外的第二道不扩张防线）。
    if matched < LS_DETECT_MIN_MATCHES {
        return text.to_string();
    }

    // 头部：条目总数（与逐条记录数严格一致，门禁 rule 9 口径）。
    out.push(format!("[LS] {matched} entries"));
    out.extend(entries);
    // 杂散行原样保留（PRE 行等可能含路径信息）。
    out.extend(stray);
    out.join("\n")
}
