//! 路径压缩器方法实现

use super::types::PathCompressor;
use crate::core::dictionary_engine::DictionaryEngine;
use bumpalo::Bump;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

pub static PATH_SCANNER_RE: Lazy<Regex> = Lazy::new(|| {
    // 极其进取的路径扫描：只要包含斜杠且由合法路径字符组成
    // 覆盖 /usr/bin/gcc, C:\Windows, ./file.txt, ../file.txt, src/core/mod.rs 等
    Regex::new(
        r#"(?:[a-zA-Z]:\\|//|\./|\.\./|/|[\w\.-]+/)[\w\.\-\+_~=@#]+(?:[/\\][\w\.\-\+_~=@#]+)*"#,
    )
    .unwrap()
});

#[tracing::instrument(level = "debug", skip_all)]
pub(crate) fn is_vcs_diff_header_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("--- ")
        || trimmed.starts_with("+++ ")
        || trimmed.starts_with("*** ")
        || trimmed.starts_with("=== ")
        || trimmed.starts_with("Index: ")
        || trimmed.starts_with("diff --git ")
        || trimmed.starts_with("diff -r ")
        || trimmed.starts_with("rename from ")
        || trimmed.starts_with("rename to ")
        || trimmed.starts_with("copy from ")
        || trimmed.starts_with("copy to ")
}

impl PathCompressor {
    /// 从给定的文本中正则表达式提取所有疑似路径，并应用压缩转换。
    ///
    /// # 参数
    /// - `text`: 待处理的原始文本。
    ///
    /// # 返回
    /// 替换路径为公共前缀 Token 后的文本。
    pub fn extract_and_compress_from_text(&mut self, text: &str) -> String {
        // 匹配 Linux 风格的路径（简单的正向扫描）
        let path_regex = regex::Regex::new(r"(/[a-zA-Z0-9_./-]+)").unwrap();

        // 收集所有匹配到的路径字符串
        let paths: Vec<&str> = path_regex.find_iter(text).map(|m| m.as_str()).collect();

        if paths.is_empty() {
            return text.to_string();
        }

        // 分析这些路径并识别公共前缀
        self.extract_common_prefixes(&paths);

        // 执行文本替换逻辑
        let mut result = text.to_string();
        for (token, prefix) in self.get_prefix_map() {
            result = result.replace(prefix, token);
        }

        result
    }
}

/// 核心路径优化函数：支持 Arena 分配以实现零拷贝
pub fn replace_paths_in_text_scoped<'a>(
    text: &'a str,
    dict_engine: &mut DictionaryEngine,
    arena: Option<&'a Bump>,
) -> Cow<'a, str> {
    if !text.contains('/') && !text.contains('\\') {
        return Cow::Borrowed(text);
    }

    let mut has_match = false;
    let mut changed = false;
    let mut out = String::with_capacity(text.len());

    for chunk in text.split_inclusive('\n') {
        let (line_with_cr, has_newline) = if let Some(line) = chunk.strip_suffix('\n') {
            (line, true)
        } else {
            (chunk, false)
        };
        let (line, has_cr) = if let Some(line) = line_with_cr.strip_suffix('\r') {
            (line, true)
        } else {
            (line_with_cr, false)
        };

        if is_vcs_diff_header_line(line) {
            out.push_str(line);
        } else {
            let mut line_has_match = false;
            let replaced = PATH_SCANNER_RE.replace_all(line, |caps: &regex::Captures| {
                let m = caps.get(0).unwrap();
                let path = m.as_str();

                let preceding = &line[..m.start()];
                if preceding.ends_with("http:") || preceding.ends_with("https:") {
                    return path.to_string();
                }

                if path.contains('@')
                    || path.starts_with("http://")
                    || path.starts_with("https://")
                    || path.contains('(')
                    || path.contains(')')
                    || path.contains(';')
                    || path.contains('=')
                {
                    return path.to_string();
                }
                if path.starts_with('.')
                    && !path.contains('/')
                    && !path.contains('\\')
                    && path
                        .chars()
                        .skip(1)
                        .all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    return path.to_string();
                }
                line_has_match = true;
                dict_engine.add_path_layered(path)
            });

            if line_has_match {
                has_match = true;
                if replaced.as_ref() != line {
                    changed = true;
                }
            }

            out.push_str(replaced.as_ref());
        }

        if has_cr {
            out.push('\r');
        }
        if has_newline {
            out.push('\n');
        }
    }

    if !has_match || !changed {
        return Cow::Borrowed(text);
    }

    if let Some(a) = arena {
        Cow::Borrowed(a.alloc_str(&out))
    } else {
        Cow::Owned(out)
    }
}

pub fn replace_paths_in_text(text: &str, dict_engine: &mut DictionaryEngine) -> Cow<'static, str> {
    let res = replace_paths_in_text_scoped(text, dict_engine, None);
    Cow::Owned(res.into_owned())
}
