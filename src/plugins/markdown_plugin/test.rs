//! markdown_plugin 测试模块（文件驱动，严禁 Hardcode）

#[cfg(test)]
mod tests {
    use crate::core::plugin_dispatcher::Plugin;
    use crate::core::text_slicer::SliceType;
    use crate::plugins::markdown_plugin::types::MarkdownPlugin;
    use crate::plugins::test_utils::*;

    #[test]
    fn detects_links_sample() {
        let plugin = MarkdownPlugin::new();
        let raw = read_sample_file("markdown_plugin", "case_004_links.md");
        let score = plugin.detect(&make_test_slice(&raw, SliceType::Paragraph));
        assert!(score.is_some());
        assert!(score.unwrap() >= 0.5);
    }

    #[test]
    fn detects_headers_sample() {
        let plugin = MarkdownPlugin::new();
        let raw = read_sample_file("markdown_plugin", "case_005_headers.md");
        assert!(plugin
            .detect(&make_test_slice(&raw, SliceType::Paragraph))
            .is_some());
    }

    #[test]
    fn compresses_links_sample_preserves_renderable_markdown_without_expansion() {
        let plugin = MarkdownPlugin::new();
        let raw = read_sample_file("markdown_plugin", "case_004_links.md");
        let out = compress_to_string(&plugin, &raw, SliceType::Paragraph);
        assert!(out.contains("[Google](https://www.google.com)"), "{out}");
        assert!(
            out.contains("![Image](https://example.com/image.png)"),
            "{out}"
        );
        assert!(
            out.contains("[Link with title](https://example.com \"Title\")"),
            "{out}"
        );
        assert!(
            out.len() <= raw.len(),
            "markdown 压缩不得显著扩张: raw={} out={}",
            raw.len(),
            out.len()
        );
    }
}
