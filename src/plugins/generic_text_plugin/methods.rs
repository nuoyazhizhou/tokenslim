use super::types::GenericTextPlugin;
use crate::core::compression::Token;
use crate::core::dedup_engine::DedupEngine;
use crate::core::dictionary_engine::DictionaryEngine;
use crate::core::plugin_dispatcher::CompressResult;
use crate::core::text_slicer::Slice;
use bumpalo::Bump;
use std::borrow::Cow;

/// 通用文本压缩：先做轻量清理（剥离 ANSI、进度条覆盖、制表符/尾空白/空行折叠），
/// 再按 `GenericTextConfig` 压缩矩阵逐类应用保语义压缩。
///
/// 流程：逐行轻量清理 → 时间戳归一 → 噪声行裁剪 → 重复行收敛 → 字典化/去重。
/// 锚点约束：不丢弃输入首行（含空行折叠前缀）；命中丢弃/收敛均为「保语义」形态。
pub fn compress_generic_text<'a>(
    plugin: &GenericTextPlugin,
    slice: &'a Slice<'a>,
    dict_engine: &mut DictionaryEngine,
    dedup_engine: &mut DedupEngine,
    arena: &'a Bump,
) -> CompressResult<'a> {
    let config = &plugin.config;
    let text = slice.text.as_ref();
    let ansi_cleaned = plugin.ansi_pattern.replace_all(text, "");
    let trailing_newline = ansi_cleaned.ends_with('\n');

    // 阶段 1：逐行轻量清理（ANSI 已在上面剥离）。用 lines() 避免 split('\n')
    // 在末尾产生多余空串导致尾部凭空多一换行（会破坏 ROI 门禁）。
    let mut lines: Vec<String> = Vec::new();
    let mut blank_streak = 0usize;
    for raw_line in ansi_cleaned.lines() {
        let line_no_cr = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        // 进度条重绘：只保留最后一次覆盖结果。
        let line = line_no_cr
            .rsplit('\r')
            .next()
            .unwrap_or(line_no_cr)
            .to_string();

        let line = if config.normalize_tabs {
            line.replace('\t', " ")
        } else {
            line
        };
        let line = if config.trim_trailing_whitespace {
            line.trim_end_matches([' ', '\t']).to_string()
        } else {
            line
        };

        let is_blank = line.trim().is_empty();
        if is_blank {
            if config.collapse_blank_lines && blank_streak > 0 {
                continue;
            }
            blank_streak += 1;
        } else {
            blank_streak = 0;
        }
        lines.push(line);
    }

    // 阶段 2：压缩矩阵（默认开的高保真策略 + 默认关的高风险策略）。
    if config.normalize_timestamps {
        for line in &mut lines {
            normalize_line_timestamp(plugin, line);
        }
    }
    if config.drop_noise_lines {
        lines.retain(|l| !is_noise_line(l));
    }
    if config.collapse_repeats {
        lines = collapse_repeat_lines(lines);
    }
    if config.enable_dictionary {
        lines = lines
            .into_iter()
            .map(|l| dictionary_line(config, dict_engine, l))
            .collect();
    }
    if config.enable_dedup {
        for line in &mut lines {
            dedup_line(config, dedup_engine, dict_engine, arena, line);
        }
    }

    let mut out = lines.join("\n");
    if trailing_newline {
        out.push('\n');
    }

    CompressResult {
        tokens: vec![Token::Text(Cow::Owned(out))],
        metadata: None,
        plugin_name: Some(plugin.name),
    }
}

/// 行首时间戳归一：若行首命中 `HH:MM(:SS)(.millis)` 或 `YYYY-MM-DD HH:MM:SS(.millis)`，
/// 将时间数值替换为统一占位符 `[T]`（保留"该行带时间戳"这一事实，不保留具体数值）。
fn normalize_line_timestamp(plugin: &GenericTextPlugin, line: &mut String) {
    if plugin.timestamp_pattern.is_match(line) {
        let rest = plugin.timestamp_pattern.replace(line, "");
        *line = format!("[T] {}", rest.trim_start());
    }
}

/// 判定是否为噪声行：全由装饰字符构成，或为纯进度/百分比指示，不含决策语义。
fn is_noise_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        // 空行由 collapse_blank_lines 处理；此处不作为噪声行丢弃。
        return false;
    }
    // 纯装饰行：仅由 `-=_*.~·>` 等字符组成。
    if t.chars()
        .all(|c| matches!(c, '-' | '=' | '_' | '*' | '.' | '~' | '·' | ' ' | '#' | '>'))
    {
        return true;
    }
    // 纯进度指示：整行仅含数字与百分比/进度条符号。
    if t.contains('%')
        && t.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '%' | '.' | '-' | '=' | '#' | '>'))
    {
        return true;
    }
    false
}

/// 连续重复行收敛：相邻归一化后完全相同的行 → 保留首行并追加 ` ×N`（N 为出现次数），中间行剪除。
fn collapse_repeat_lines(lines: Vec<String>) -> Vec<String> {
    fn push_repeat(out: &mut Vec<String>, line: String, count: u64) {
        if count > 1 {
            out.push(format!("{line} ×{count}"));
        } else {
            out.push(line);
        }
    }

    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut prev: Option<String> = None;
    let mut count: u64 = 0;
    for line in lines {
        if let Some(p) = &prev {
            if *p == line {
                count += 1;
                continue;
            }
            push_repeat(&mut out, prev.take().unwrap(), count);
        }
        prev = Some(line);
        count = 1;
    }
    if let Some(p) = prev {
        push_repeat(&mut out, p, count);
    }
    out
}

/// 字典化：对中长重复行调用 `DictionaryEngine::add_macro` 取短引用；
/// 仅在取回的 token 确实比原文短时替换（无字典管理器时自动退化为原文，不扩张）。
fn dictionary_line(
    _config: &super::types::GenericTextConfig,
    dict_engine: &mut DictionaryEngine,
    line: String,
) -> String {
    if line.len() < 24 {
        return line;
    }
    let token = dict_engine.add_macro(&line);
    if token.len() < line.len() {
        token
    } else {
        line
    }
}

/// 去重：对整行尝试 `DedupEngine::dedup_cross_slice`，仅当返回引用比原文短时替换；
/// 无全局管理器时 add_macro 返回原文，自动不替换，保证不扩张。
fn dedup_line(
    _config: &super::types::GenericTextConfig,
    dedup_engine: &mut DedupEngine,
    dict_engine: &mut DictionaryEngine,
    arena: &Bump,
    line: &mut String,
) {
    let Some(result) = dedup_engine.dedup_cross_slice(line, dict_engine, arena) else {
        return;
    };
    let rep: String = result
        .tokens
        .iter()
        .filter_map(|t| match t {
            Token::Text(s) | Token::DictRef(s) => Some(s.to_string()),
            _ => None,
        })
        .collect();
    if !rep.is_empty() && rep.len() < line.len() {
        *line = rep;
    }
}
