//! text slicer 测试模块
//!
//! # 测试概述
//!
//! 本模块包含 text slicer 模块的单元测试 and 集成测试。
//! 测试覆盖了主要功能 and 边界情况。

#[cfg(test)]
mod tests {
    use crate::core::dictionary_manager::DictionaryManager;
    use crate::core::stream_reader::SliceInput;
    use crate::core::text_slicer::*;
    use std::borrow::Cow;
    use std::sync::Arc;

    fn setup_slicer() -> TextSlicer {
        let config = SlicerConfig::default();
        let dict_manager = Arc::new(DictionaryManager::new());
        TextSlicer::with_dict_manager(config, dict_manager)
    }

    #[test]
    fn test_new() {
        let slicer = setup_slicer();
        // 验证初始化状态
        assert_eq!(slicer.next_id.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(slicer.paragraph_buffer.is_empty());
    }

    #[test]
    fn test_slice_line() {
        let slicer = setup_slicer();

        // 创建测试输入
        let input = SliceInput {
            raw: Cow::Borrowed("Hello World"),
            offset: 0,
            line_number: 1,
            file_metadata: None,
        };

        // 测试切片
        let slice = slicer.slice_line(&input);
        assert_eq!(slice.id, 1);
        assert_eq!(slice.text, "Hello World");
        assert_eq!(slice.slice_type, SliceType::Line);
        assert_eq!(slice.offset, 0);
        assert_eq!(slice.line_start, 1);
        assert_eq!(slice.line_end, 1);
        assert!(slice.file_metadata.is_none());

        // 测试 ID 递增
        let input2 = SliceInput {
            raw: Cow::Borrowed("Second line"),
            offset: 12,
            line_number: 2,
            file_metadata: None,
        };
        let slice2 = slicer.slice_line(&input2);
        assert_eq!(slice2.id, 2);
    }

    #[test]
    fn test_slice_paragraph() {
        let mut slicer = setup_slicer();

        // 测试非空行累积
        let input1 = SliceInput {
            raw: Cow::Borrowed("First line"),
            offset: 0,
            line_number: 1,
            file_metadata: None,
        };
        assert!(slicer.slice_paragraph(&input1).is_none());

        let input2 = SliceInput {
            raw: Cow::Borrowed("Second line"),
            offset: 10,
            line_number: 2,
            file_metadata: None,
        };
        assert!(slicer.slice_paragraph(&input2).is_none());

        // 测试空行触发段落输出
        let input3 = SliceInput {
            raw: Cow::Borrowed(""),
            offset: 21,
            line_number: 3,
            file_metadata: None,
        };
        let slice = slicer.slice_paragraph(&input3).unwrap();
        assert_eq!(slice.id, 1);
        assert_eq!(slice.text, "First line\nSecond line");
        assert_eq!(slice.slice_type, SliceType::Paragraph);
        assert_eq!(slice.offset, 0);
        assert_eq!(slice.line_start, 1);
        assert_eq!(slice.line_end, 2);
    }

    #[test]
    fn test_flush() {
        let mut slicer = setup_slicer();

        // 累积一些行
        let input1 = SliceInput {
            raw: Cow::Borrowed("First line"),
            offset: 0,
            line_number: 1,
            file_metadata: None,
        };
        assert!(slicer.slice_paragraph(&input1).is_none());

        let input2 = SliceInput {
            raw: Cow::Borrowed("Second line"),
            offset: 10,
            line_number: 2,
            file_metadata: None,
        };
        assert!(slicer.slice_paragraph(&input2).is_none());

        // 刷新缓冲区
        let slices = slicer.flush();
        assert_eq!(slices.len(), 1);
        let slice = &slices[0];
        assert_eq!(slice.id, 1);
        assert_eq!(slice.text, "First line\nSecond line");
        assert_eq!(slice.slice_type, SliceType::Paragraph);

        // 再次刷新，应该返回空
        let slices2 = slicer.flush();
        assert!(slices2.is_empty());
    }

    #[test]
    fn test_empty_line_handling() {
        let mut slicer = setup_slicer();

        // 测试空行处理
        let input = SliceInput {
            raw: Cow::Borrowed(""),
            offset: 0,
            line_number: 1,
            file_metadata: None,
        };
        let slice = slicer.slice_paragraph(&input).unwrap();
        assert_eq!(slice.id, 1);
        assert_eq!(slice.text, "");
        assert_eq!(slice.slice_type, SliceType::Line);
        assert_eq!(slice.line_start, 1);
        assert_eq!(slice.line_end, 1);
    }

    #[test]
    fn test_push_slices_preserve_usage_blank_separators_when_not_skipping() {
        let config = SlicerConfig {
            mode: SliceMode::Paragraph,
            skip_empty_lines: false,
        };
        let dict_manager = Arc::new(DictionaryManager::new());
        let mut slicer = TextSlicer::with_dict_manager(config, dict_manager);
        let mut out = Vec::new();

        let lines = [
            "npm <command>",
            "",
            "Usage:",
            "",
            "npm install        install all the dependencies in your project",
        ];

        for (idx, line) in lines.iter().enumerate() {
            let input = SliceInput {
                raw: Cow::Borrowed(*line),
                offset: idx,
                line_number: idx + 1,
                file_metadata: None,
            };
            slicer.push_slices_by_mode(&input, &mut out);
        }
        out.extend(slicer.flush());

        let merged = out
            .iter()
            .map(|s| s.text.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(
            merged,
            "npm <command>\n\nUsage:\n\nnpm install        install all the dependencies in your project"
        );
    }

    #[test]
    fn test_push_slices_preserve_consecutive_blank_lines_when_not_skipping() {
        let config = SlicerConfig {
            mode: SliceMode::Paragraph,
            skip_empty_lines: false,
        };
        let dict_manager = Arc::new(DictionaryManager::new());
        let mut slicer = TextSlicer::with_dict_manager(config, dict_manager);
        let mut out = Vec::new();

        let lines = ["A", "", "", "B"];
        for (idx, line) in lines.iter().enumerate() {
            let input = SliceInput {
                raw: Cow::Borrowed(*line),
                offset: idx,
                line_number: idx + 1,
                file_metadata: None,
            };
            slicer.push_slices_by_mode(&input, &mut out);
        }
        out.extend(slicer.flush());

        let merged = out
            .iter()
            .map(|s| s.text.as_ref())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(merged, "A\n\n\nB");
    }
}
