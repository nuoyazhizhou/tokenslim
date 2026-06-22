//! Compression Protocol V1 法则 A 的 ROI 门控辅助函数。
//!
//! 该模块把 `vcs_plugin/methods.rs` 里的私有 `prefer_non_expanding` 提炼为
//! `crate::core::utils::roi::prefer_non_expanding`，供所有插件共享使用。
//! 语义与原版一致：压缩后输出若体积反而变大（按 `trim_end_matches('\n' | '\r')`
//! 对齐后比较），则回退原文；否则保留压缩结果。
//!
//! 非 VCS 插件必须在 `compress()` 的最外层用它包裹最终字符串，否则会触发
//! `docs/prompts/non_vcs_classical_prompts.md` 的 1.3 条约束违规。

/// 若 `compacted` 的有效字节数（忽略尾部换行/回车）超过 `raw`，返回 `raw.to_string()`；
/// 否则返回原 `compacted`。
///
/// # 设计要点
/// - **等长判定**用 `trim_end_matches('\n' | '\r')` 对齐，避免 `\r\n` 与 `\n` 的干扰。
/// - **真正的字节比较**再走一次完整 `len()`：如果 `compacted.len() > raw.len()`
///   （即便 trimmed 相等），仍返回 raw——这是 showcase 报告实际度量的维度。
/// - 允许等长输出通过，覆盖「无实质变化的行级重写」场景。
pub fn prefer_non_expanding(raw: &str, compacted: String) -> String {
    let raw_trim = raw.trim_end_matches('\n').trim_end_matches('\r').len();
    let comp_trim = compacted
        .trim_end_matches('\n')
        .trim_end_matches('\r')
        .len();
    if comp_trim > raw_trim {
        return raw.to_string();
    }
    // 即使 trimmed 相等，若完整字节数扩张，也应回退 raw：
    // showcase 报告用 `.len()` 度量 compression_pct，忽略尾换行差异会被审计判为 G1_ROI 负。
    if compacted.len() > raw.len() {
        return raw.to_string();
    }
    compacted
}

#[cfg(test)]
mod tests {
    use super::prefer_non_expanding;

    #[test]
    fn keeps_compacted_when_equal_or_shorter() {
        assert_eq!(prefer_non_expanding("abc", "ab".to_string()), "ab");
        assert_eq!(prefer_non_expanding("abc", "abc".to_string()), "abc");
    }

    #[test]
    fn falls_back_to_raw_when_compacted_expands() {
        let raw = "hi";
        let compact = "hello".to_string();
        assert_eq!(prefer_non_expanding(raw, compact), "hi");
    }

    #[test]
    fn falls_back_when_trailing_newline_grows_full_bytes() {
        // trimmed 相等但 compacted 完整字节多 1，应回退 raw。
        // showcase 报告用 `.len()` 度量，不回退会被审计判为 G1_ROI 负。
        let raw = "abc";
        let compact = "abc\n".to_string();
        assert_eq!(prefer_non_expanding(raw, compact), "abc");
    }

    #[test]
    fn keeps_compacted_when_full_bytes_equal() {
        // 两端都带尾换行且完整字节相等，保留 compacted。
        let raw = "abc\n";
        let compact = "abc\n".to_string();
        assert_eq!(prefer_non_expanding(raw, compact), "abc\n");
    }

    #[test]
    fn trims_crlf_symmetrically() {
        // Windows 尾换行 `\r\n`（2 bytes）vs Unix `\n`（1 byte）：
        // raw 完整 6B、compact 完整 5B，compact 更短 → 保留 compact。
        let raw = "line\r\n";
        let compact = "line\n".to_string();
        assert_eq!(prefer_non_expanding(raw, compact), "line\n");
    }
}
