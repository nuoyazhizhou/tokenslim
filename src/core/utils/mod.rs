pub mod json;
pub mod roi;

use regex::Regex;
use std::sync::OnceLock;

/// 剥离终端 ANSI 转义序列，供非 VCS 插件在入口第一步统一净化。
pub fn strip_ansi(text: &str) -> String {
    if !text.as_bytes().contains(&0x1b) {
        return text.to_string();
    }
    static ANSI_RE: OnceLock<Regex> = OnceLock::new();
    ANSI_RE
        .get_or_init(|| Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]").unwrap())
        .replace_all(text, "")
        .into_owned()
}
