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

/// 判断给定行是否为版本控制 diff 的头部行（如 `--- ` / `+++ ` / `*** ` / `Index: ` / `diff --git ` / `rename from/to` 等）。
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

        // 执行文本替换逻辑。
        // P2-59：① 按 prefix 长度降序替换（HashMap 旧顺序下短前缀可能先命中
        // 长前缀的头部，如 `/usr` 先于 `/usr/local`，把后者破坏成 `$P1/local`）；
        // ② 改用带边界的替换（见 `replace_prefix_with_boundary`），消除对更长
        // 路径（`/usr/local-old`）的子串级破坏。
        let mut result = text.to_string();
        let mut pairs: Vec<(&String, &String)> = self.get_prefix_map().iter().collect();
        // 等长平局按字典序破平（确定性，见 types.rs extract_common_prefixes 注）
        pairs.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.1.cmp(&b.1)));
        for (token, prefix) in pairs {
            result = replace_prefix_with_boundary(&result, prefix, token);
        }

        result
    }
}

/// P2-59：带边界检查的前缀替换——仅当匹配位置的后继字符不是路径字符时才
/// 替换为 token。无边界 `str::replace` 会把 `/usr/local` 命中进
/// `/usr/local-old`/`/usr/local2` 等更长路径，破坏成 `$P1-old`/`$P12`
/// （后者还会被 token 正则 `\$P\d+` 贪婪吞并成 undefined placeholder，属
/// 不可逆破坏；vcs_p4/vcs_svn depot 路径共享前缀极多，属生产路径）。
///
/// 后继为路径分隔符 `/`、`\` 时属合法折叠延续（prefix 后接更深层级），不算
/// 边界违规；其余 `[\w.\-_+~=@#]` 后继与 `replace_paths_in_text_scoped` 的
/// PATH_SCANNER_RE 字符类对齐，一律不替换。
///
/// 前导不做检查（实证教训，v20260909_r3）：相对路径场景 `src/core/...` 被
/// path_regex 匹配为 `/core/...`，前导 `src` 的 `c` 会把全部合法替换挡掉
/// （case_230 压缩率 49%→0% 功能回退）；且 `src$P1/` 产物可逆、语义门禁
/// 可过，与旧版行为一致。
fn replace_prefix_with_boundary(text: &str, prefix: &str, token: &str) -> String {
    if prefix.is_empty() {
        return text.to_string();
    }
    // 后继为路径分隔符 `/`、`\` 时属合法折叠延续（prefix 后接更深层级），
    // 不算边界违规；真正的破坏是紧贴 `-`/字母数字/`.` 等组成新路径段（src-old/src2）。
    let is_trail_path_char = |c: char| {
        c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | '+' | '~' | '=' | '@' | '#')
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        match rest.find(prefix) {
            Some(pos) => {
                let end = pos + prefix.len();
                let next_ok = end >= rest.len() || {
                    let mut e = end;
                    while e < rest.len() && !rest.is_char_boundary(e) {
                        e += 1;
                    }
                    !rest[e..]
                        .chars()
                        .next()
                        .map(is_trail_path_char)
                        .unwrap_or(false)
                };
                if next_ok {
                    out.push_str(&rest[..pos]);
                    out.push_str(token);
                } else {
                    // 边界冲突：原样保留该段，继续扫描剩余文本
                    out.push_str(&rest[..end]);
                }
                rest = &rest[end..];
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }
    out
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

/// 在文本中扫描并替换所有路径为字典引擎中的简短占位符，返回静态生命周期的压缩文本（内部委托 `replace_paths_in_text_scoped`）。
pub fn replace_paths_in_text(text: &str, dict_engine: &mut DictionaryEngine) -> Cow<'static, str> {
    let res = replace_paths_in_text_scoped(text, dict_engine, None);
    Cow::Owned(res.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试：前缀替换的边界检查（P2-59 处置）。
    #[test]
    /// 契约：公共前缀 `/vagrant/build_orama/src` 折叠为 `$P1`，但不得命中
    /// `/vagrant/build_orama/src-old`、`/vagrant/build_orama/src2` 等更长
    /// 路径的子串（无边界 `str::replace` 会破坏成 `$P1-old`/`$P12`）。
    fn p2_59_boundary_safe_prefix_replacement() {
        let mut c = PathCompressor::new();
        let text = "/vagrant/build_orama/src/main.rs
/vagrant/build_orama/src/lib.rs
/vagrant/build_orama/src-old/legacy.rs
/vagrant/build_orama/src2/other.rs
";
        let out = c.extract_and_compress_from_text(text);
        // 两条路径正常折叠（$P1 = /vagrant/build_orama/src，24 字符 ≥ min 20、出现 2 次 ≥ min 2）
        assert!(out.contains("$P1"), "应有前缀折叠发生: {out}");
        assert!(out.contains("$P1/main.rs"), "main.rs 应折叠为 $P1/main.rs: {out}");
        // 更长路径不被子串级破坏
        assert!(out.contains("/vagrant/build_orama/src-old"), "src-old 路径应原样保留: {out}");
        assert!(out.contains("/vagrant/build_orama/src2"), "src2 路径应原样保留: {out}");
        assert!(!out.contains("$P1-"), "不得破坏 src-old: {out}");
        assert!(!out.contains("$P12"), "不得破坏 src2: {out}");
    }

    /// 测试：等长前缀的 token 分配确定性（P2-44 执行期新发现缺陷修复）。
    /// 契约：两个前缀长度相等（均为 10 字符）且各自出现次数达标时，$P1 必须分配给
    /// 字典序较小者（/config.rs），$P2 分配给 /logger.rs——不得依赖 HashMap 迭代序
    /// （每进程随机），否则冻结基线会随机漂移（vcs_svn case_76_svn_unlock 实例）。
    #[test]
    fn equal_length_prefixes_get_deterministic_tokens() {
        // case_76_svn_unlock 原文（min_prefix_length=10、min_occurrences=2，与 svn showcase 一致）
        let text = "svn unlock src/main.rs src/config.rs src/logger.rs\n'src/main.rs' unlocked.\n'src/config.rs' unlocked.\n'src/logger.rs' unlocked.\n";
        let mut c = PathCompressor::new();
        c.set_min_prefix_length(10);
        c.set_min_occurrences(2);
        let out = c.extract_and_compress_from_text(text);
        assert!(
            out.contains("src$P1 src$P2"),
            "命令行中 config.rs(字典序在前) 应得 $P1、logger.rs 应得 $P2: {out}"
        );
        let map = c.get_prefix_map();
        assert_eq!(map.get("$P1").map(String::as_str), Some("/config.rs"));
        assert_eq!(map.get("$P2").map(String::as_str), Some("/logger.rs"));
    }
}
