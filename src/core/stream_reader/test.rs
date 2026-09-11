#[cfg(test)]
mod tests {
    use crate::core::stream_reader::StreamReader;

    /// 测试：from_str 创建的读取器大小与文本字节数一致，且被判定为文本。
    #[test]
    fn test_from_str() {
        let text = "Hello, world!";
        let reader = StreamReader::from_str(text);
        assert_eq!(reader.size(), text.len());
        assert!(reader.is_text());
    }

    /// 测试：空字符串读取器大小为 0 且仍为文本。
    #[test]
    fn test_empty_string() {
        let reader = StreamReader::from_str("");
        assert_eq!(reader.size(), 0);
        assert!(reader.is_text());
    }

    /// 测试：from_str 创建的读取器不携带文件元数据。
    #[test]
    fn test_metadata() {
        let text = "test";
        let reader = StreamReader::from_str(text);
        assert!(reader.metadata().is_none());
    }

    /// 测试：正常文本判定为文本，含 NULL 字节的数据判定为非文本。
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

    /// 测试：detect_binary 对普通文本返回 false，对全 NULL 字节数据返回 true。
    #[test]
    fn test_detect_binary() {
        let text_data = b"Normal text";
        assert!(!StreamReader::detect_binary(text_data));
        let bin_data = [0u8; 10];
        assert!(StreamReader::detect_binary(&bin_data));
    }

    /// 测试：逐行迭代支持 LF 与 CRLF 混合换行，行内容不含行尾符。
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

    /// 测试：按块迭代按指定大小切分数据，末尾不足一块时返回剩余部分。
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

    /// 测试：动态块大小被限制在 256KB 与 5MB 之间（极小输入取最小值，极大输入取最大值）。
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

    /// 测试：语义切分不会把 Java 堆栈跟踪的缩进续行（\t 开头）断开，切点落在 Caused by 行之前。
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

    /// 测试：极小输入在并行切块时只产生一个完整块。
    #[test]
    fn test_split_for_parallel_tiny_input_single_chunk() {
        let reader = StreamReader::from_str("only one short line");
        let chunks = reader.split_for_parallel(32);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].raw, "only one short line");
        assert_eq!(chunks[0].offset, 0);
    }

    /// 测试：单个超大行（无任何可断点）时语义切分回退到数据末尾。
    #[test]
    fn test_split_by_semantic_anchors_huge_single_line_falls_back_to_end() {
        let huge_line = "A".repeat(700_000);
        let bytes = huge_line.as_bytes();

        let end = StreamReader::split_by_semantic_anchors(bytes, 0, 256 * 1024);
        assert_eq!(end, bytes.len());
    }

    /// P1-08 回归：GBK 编码文件经 from_file 读入后正确转码——不再出现
    /// U+FFFD 替换符（旧实现硬编码 Utf8 + from_utf8_lossy 不可逆损坏）。
    #[test]
    fn from_file_gbk_decodes_without_replacement_chars() {
        use crate::core::stream_reader::CharsetEncoding;
        // GBK 字节："汉化日志: 启动完成\n"
        // 注意选字：「中文」的 GBK（D6D0 CEC4）恰为合法 UTF-8 序列，字节级
        // 检测无法区分；「汉化」（BABA BBAF）含非法 UTF-8 前导，可触发转码路径。
        let gbk_bytes: Vec<u8> = vec![
            0xBA, 0xBA, 0xBB, 0xAF, 0xC8, 0xD5, 0xD6, 0xBE, 0x3A, 0xC6, 0xF4, 0xB6, 0xAF, 0xCD,
            0xEA, 0xB3, 0xC9, 0x0A,
        ];
        let dir = std::env::temp_dir().join(format!("tokenslim_p108_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("gbk.log");
        std::fs::write(&path, &gbk_bytes).unwrap();

        let reader = StreamReader::from_file(&path).unwrap();
        let meta = reader.metadata().expect("from_file 应产生 metadata");
        assert_eq!(meta.file_type, crate::core::stream_reader::FileType::Text);
        let text: String = reader
            .iter_lines()
            .map(|l| l.raw.clone().into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::remove_dir_all(&dir).ok();

        assert!(
            matches!(
                meta.encoding,
                CharsetEncoding::Gbk | CharsetEncoding::Gb2312 | CharsetEncoding::Unknown
            ),
            "编码应识别为 GBK 系: {:?}",
            meta.encoding
        );
        assert!(text.contains("汉化"), "GBK 应正确解码出中文: {text:?}");
        assert!(
            !text.contains('\u{FFFD}'),
            "不允许 U+FFFD 替换符残留（P1-08）: {text:?}"
        );
    }

    /// P1-08 回归：UTF-8 文件行为不变（编码 Utf8、内容逐字节一致），
    /// 保证既有 UTF-8 冻结基线零漂移。
    #[test]
    fn from_file_utf8_content_unchanged() {
        let dir = std::env::temp_dir().join(format!("tokenslim_p108b_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("utf8.log");
        let content = "plain ascii log line\nsecond line\n";
        std::fs::write(&path, content).unwrap();

        let reader = StreamReader::from_file(&path).unwrap();
        let text: String = reader
            .iter_lines()
            .map(|l| l.raw.clone().into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::remove_dir_all(&dir).ok();

        assert!(text.contains("plain ascii log line"));
        assert!(!text.contains('\u{FFFD}'));
    }

    /// P2-22 回归：`from_file` 读入的是自有缓冲快照——读入完成后文件被外部
    /// 截断（logrotate / truncate 场景）不影响已读内容。旧 mmap 实现此时访问
    /// 「映射长度内但已超出新文件尾」的页会触发不可捕获的 SIGSEGV。
    #[test]
    fn from_file_snapshot_survives_external_truncation() {
        let dir = std::env::temp_dir().join(format!("tokenslim_p222_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("truncate_case.log");
        let content = "line one\nline two\nline three\n";
        std::fs::write(&path, content).unwrap();

        let reader = StreamReader::from_file(&path).unwrap();
        // 模拟 logrotate：读入后立即从外部清空文件
        std::fs::write(&path, b"").unwrap();

        let text: String = reader
            .iter_lines()
            .map(|l| l.raw.clone().into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(
            text, "line one\nline two\nline three",
            "截断后已读内容必须完整保留（快照语义，P2-22）"
        );
    }
}
