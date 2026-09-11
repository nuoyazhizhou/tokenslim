/// 判定 next 字节是否为路径 token 的合法续字符：非 `0-9/a-z/A-Z/_/-` 即视为边界（token 在此结束）。
pub(crate) fn is_path_token_boundary_next(next: Option<u8>) -> bool {
    !matches!(
        next,
        Some(b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'-')
    )
}

/// 判断 text 中是否出现 token 且其后紧跟 token 边界（即 token 作为完整独立片段出现）。
pub(crate) fn contains_path_token_boundary(text: &str, token: &str) -> bool {
    let mut start = 0usize;
    while let Some(pos) = text[start..].find(token) {
        let idx = start + pos;
        let end = idx + token.len();
        let next = text.as_bytes().get(end).copied();
        if is_path_token_boundary_next(next) {
            return true;
        }
        start = end;
    }
    false
}

/// 替换 text 中所有"后紧跟边界"的 token 出现为 replacement，边界处保留 token 本身，避免误伤 token-like 片段（如 `$P1-notes` 中的 `$P1`）。
pub(crate) fn replace_path_token_boundary(text: &str, token: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = 0usize;
    while let Some(pos) = text[start..].find(token) {
        let idx = start + pos;
        let end = idx + token.len();
        let next = text.as_bytes().get(end).copied();
        out.push_str(&text[start..idx]);
        if is_path_token_boundary_next(next) {
            out.push_str(replacement);
        } else {
            out.push_str(token);
        }
        start = end;
    }
    out.push_str(&text[start..]);
    out
}

#[cfg(test)]
mod tests {
    use super::{contains_path_token_boundary, replace_path_token_boundary};

    /// 测试：验证 `contains_path_token_boundary("$P1-notes", "$P1")` 返回 false，且 `replace_path_token_boundary` 不会把 `$P1-notes` 中的 `$P1` 误替换。
    #[test]
    fn token_like_segment_suffix_is_not_boundary() {
        assert!(!contains_path_token_boundary(
            "docs/$P1-notes/readme.md",
            "$P1"
        ));
        assert_eq!(
            replace_path_token_boundary("$P1-notes", "$P1", "docs/design"),
            "$P1-notes"
        );
    }
}
