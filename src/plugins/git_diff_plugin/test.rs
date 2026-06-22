//! git_diff_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::text_slicer::SliceType;
    use crate::plugins::git_diff_plugin::GitDiffPlugin;
    use crate::plugins::test_utils::*;
    /// Git diff 头部与路径前缀必须原样保留，不得被路径字典替换。
    /// 关键的 `diff --git a/...`、`--- a/...`、`+++ b/...`、`@@ ...` 四类行必须存在。
    #[test]
    fn git_diff_headers_remain_literal_in_sample_compression() {
        let plugin = GitDiffPlugin::new();
        let raw = read_sample_log("git_diff_plugin", "case_001_simple_change");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(out.contains("diff --git "), "缺少 `diff --git` 头: {out}");
        assert!(out.contains("--- a/"), "缺少 `--- a/` 头: {out}");
        assert!(out.contains("+++ b/"), "缺少 `+++ b/` 头: {out}");
        assert!(out.contains("@@ "), "缺少 hunk header: {out}");
        assert!(!out.contains("$DIFF"), "不得再生成 $DIFF 标记: {out}");
        assert!(!out.contains("--- $P"), "--- 头的路径不得被字典替换: {out}");
        assert!(!out.contains("+++ $P"), "+++ 头的路径不得被字典替换: {out}");
    }

    #[test]
    fn compresses_multiple_hunks_sample_without_expansion() {
        let plugin = GitDiffPlugin::new();
        let raw = read_sample_log("git_diff_plugin", "case_012_multiple_hunks");
        let out = compress_to_string(&plugin, &raw, SliceType::LogBlock);
        assert!(
            out.len() <= raw.len() + 16,
            "git_diff 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }
}
