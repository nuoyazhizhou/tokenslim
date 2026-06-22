pub(crate) fn is_path_token_boundary_next(next: Option<u8>) -> bool {
    !matches!(
        next,
        Some(b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'-')
    )
}

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
