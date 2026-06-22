use super::types::GenericTextPlugin;
use crate::core::compression::Token;
use crate::core::plugin_dispatcher::CompressResult;
use crate::core::text_slicer::Slice;
use std::borrow::Cow;

pub fn compress_generic_text<'a>(
    plugin: &GenericTextPlugin,
    slice: &'a Slice<'a>,
) -> CompressResult<'a> {
    let text = slice.text.as_ref();
    let ansi_cleaned = plugin.ansi_pattern.replace_all(text, "");

    let mut out = String::with_capacity(ansi_cleaned.len());
    let mut blank_streak = 0usize;

    for chunk in ansi_cleaned.split_inclusive('\n') {
        let has_newline = chunk.ends_with('\n');
        let line_no_nl = chunk.strip_suffix('\n').unwrap_or(chunk);

        // 处理 CRLF 换行符，移除可能残留的 \r
        let line_no_cr = line_no_nl.strip_suffix('\r').unwrap_or(line_no_nl);

        // 处理进度条覆盖行：只保留最后一次重绘结果。
        let mut line = line_no_cr
            .rsplit('\r')
            .next()
            .unwrap_or(line_no_cr)
            .to_string();

        if plugin.config.normalize_tabs {
            line = line.replace('\t', " ");
        }

        if plugin.config.trim_trailing_whitespace {
            line = line.trim_end_matches([' ', '\t']).to_string();
        }

        let is_blank = line.trim().is_empty();
        if is_blank {
            if plugin.config.collapse_blank_lines && blank_streak > 0 {
                continue;
            }
            blank_streak += 1;
        } else {
            blank_streak = 0;
        }

        out.push_str(&line);
        if has_newline {
            out.push('\n');
        }
    }

    CompressResult {
        tokens: vec![Token::Text(Cow::Owned(out))],
        metadata: None,
        plugin_name: Some(plugin.name),
    }
}
