#[cfg(test)]
mod tests {
    use crate::core::stream_reader::StreamReader;

    #[test]
    fn test_from_str() {
        let text = "Hello, world!";
        let reader = StreamReader::from_str(text);
        assert_eq!(reader.size(), text.len());
        assert!(reader.is_text());
    }

    #[test]
    fn test_empty_string() {
        let reader = StreamReader::from_str("");
        assert_eq!(reader.size(), 0);
        assert!(reader.is_text());
    }

    #[test]
    fn test_metadata() {
        let text = "test";
        let reader = StreamReader::from_str(text);
        assert!(reader.metadata().is_none());
    }

    #[test]
    fn test_is_text() {
        let reader = StreamReader::from_str("Normal text");
        assert!(reader.is_text());

        let bin_data = [0u8, 1, 2, 0, 4];
        let reader_bin = StreamReader {
            inner: crate::core::stream_reader::types::Inner::Bytes(&bin_data),
            metadata: None,
        };
        assert!(!reader_bin.is_text());
    }

    #[test]
    fn test_detect_binary() {
        let text_data = b"Normal text";
        assert!(!StreamReader::detect_binary(text_data));
        let bin_data = [0u8; 10];
        assert!(StreamReader::detect_binary(&bin_data));
    }

    #[test]
    fn test_iter_lines() {
        let text = "line1\nline2\r\nline3";
        let reader = StreamReader::from_str(text);
        let lines: Vec<_> = reader.iter_lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].raw, "line1");
        assert_eq!(lines[1].raw, "line2");
        assert_eq!(lines[2].raw, "line3");
    }

    #[test]
    fn test_iter_blocks() {
        let text = "1234567890";
        let reader = StreamReader::from_str(text);
        let blocks: Vec<_> = reader.iter_blocks(3).unwrap().collect();
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].raw, "123");
        assert_eq!(blocks[1].raw, "456");
        assert_eq!(blocks[2].raw, "789");
        assert_eq!(blocks[3].raw, "0");
    }

    #[test]
    fn test_calculate_dynamic_chunk_size_bounds() {
        let tiny = StreamReader::calculate_dynamic_chunk_size(8 * 1024, 32);
        assert_eq!(tiny, 256 * 1024);

        let medium = StreamReader::calculate_dynamic_chunk_size(20 * 1024 * 1024, 32);
        assert!(medium >= 256 * 1024);
        assert!(medium <= 5 * 1024 * 1024);

        let huge = StreamReader::calculate_dynamic_chunk_size(2 * 1024 * 1024 * 1024, 1);
        assert_eq!(huge, 5 * 1024 * 1024);
    }

    #[test]
    fn test_split_by_semantic_anchors_avoids_stack_trace_continuation_boundary() {
        let text = concat!(
            "2026-03-26 10:00:00 ERROR Crash happened\n",
            "java.lang.RuntimeException: boom\n",
            "\tat com.example.Main.main(Main.java:10)\n",
            "\tat com.example.Helper.call(Helper.java:20)\n",
            "Caused by: java.lang.IllegalStateException: bad state\n",
            "\tat com.example.Service.run(Service.java:30)\n",
            "2026-03-26 10:00:01 INFO recovered\n"
        );

        let bytes = text.as_bytes();
        let target_inside_continuation = text
            .find("\tat com.example.Helper")
            .expect("test data must contain continuation line")
            + 8;

        let end = StreamReader::split_by_semantic_anchors(bytes, 0, target_inside_continuation);
        let caused_by_start = text
            .find("Caused by:")
            .expect("test data must contain caused by line");

        assert_eq!(end, caused_by_start);
    }

    #[test]
    fn test_split_for_parallel_tiny_input_single_chunk() {
        let reader = StreamReader::from_str("only one short line");
        let chunks = reader.split_for_parallel(32);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].raw, "only one short line");
        assert_eq!(chunks[0].offset, 0);
    }

    #[test]
    fn test_split_by_semantic_anchors_huge_single_line_falls_back_to_end() {
        let huge_line = "A".repeat(700_000);
        let bytes = huge_line.as_bytes();

        let end = StreamReader::split_by_semantic_anchors(bytes, 0, 256 * 1024);
        assert_eq!(end, bytes.len());
    }
}
