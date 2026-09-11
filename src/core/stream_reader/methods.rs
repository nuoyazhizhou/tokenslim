//! stream reader 模块方法实现
//!
//! # 模块概述
//!
//! 本模块实现了流式读取器的核心逻辑，支持并行读取、自动识别编码和文件类型等功能。
//!
//! # 功能说明
//!
//! 提供高效的日志和文本文件读取能力，能够自动处理不同操作系统的换行符和编码。

use super::types::*;
use crate::core::observability::{log_object_size, ScopeProbe};
use std::borrow::Cow;
use std::fs;
use std::io::Read;
use std::path::Path;

/// P2-22：mmap 读取路径已整体移除（read_inner 统一 `read_to_end` 整读到
/// 自有缓冲）。原因：mmap 的经典 soundness 陷阱——映射建立后文件被外部截断
/// （logrotate / truncate，本项目处理对象恰恰是活日志），访问「映射长度内但
/// 已超出新文件尾」的页在 Windows/Linux 上都是不可捕获的 SIGSEGV；且日志为
/// 顺序读，mmap 零拷贝收益本就有限。原 mmap 动态阈值计算（sysinfo 全量刷新，
/// 曾为性能热点 P2-23）随路径一并移除。

/// P1-08：文件头部样本的内容分类结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ByteKind {
    /// 纯 UTF-8（含 UTF-8 BOM）——整读入自有 Buffer
    Utf8,
    /// 二进制内容——保持原始字节，metadata 标记 Binary
    Binary,
    /// 非 UTF-8 文本 / UTF-16/32——需经 decode_with_fallback 转码
    Transcode,
}

impl<'a> StreamReader<'a> {
    const MIN_PARALLEL_CHUNK_SIZE: usize = 256 * 1024;
    const MAX_PARALLEL_CHUNK_SIZE: usize = 5 * 1024 * 1024;
    const SEMANTIC_SCAN_MULTIPLIER: usize = 2;

    /// 从文件创建 StreamReader。自动检测文件类型、编码、BOM 和操作系统来源。
    ///
    /// P1-08 修复：本方法此前把 `file_type`/`encoding`/`bom` 全部硬编码占位
    /// （GBK/UTF-16 等编码文件经 `from_utf8_lossy` 不可逆损坏）。现在：
    /// ① 先读 ≤8KB 头部样本做 BOM/二进制/UTF-8 有效性分类；② 非 UTF-8 文本
    ///   转码为 UTF-8 后装入 Buffer，编码信息如实填入 metadata；③ 二进制文件
    ///   标记 `FileType::Binary`。
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, StreamError> {
        let _probe = ScopeProbe::new("stream_reader", "from_file");
        let path = path.as_ref();
        let metadata = fs::metadata(path)?;
        let file_size = metadata.len();

        let file = fs::File::open(path)?;

        // P1-08：头部样本检测（≤8KB）
        let head = Self::read_head(&file, 8192);
        let (kind, bom) = Self::classify_bytes(&head);

        let (inner, encoding, file_type) = match kind {
            // 二进制：保持原始字节，交由上层按 metadata 处理
            ByteKind::Binary => (
                Self::read_inner(&file, "from_file")?,
                CharsetEncoding::Unknown,
                FileType::Binary,
            ),
            // 纯 UTF-8（含 UTF-8 BOM）：整读入自有缓冲
            ByteKind::Utf8 => (
                Self::read_inner(&file, "from_file")?,
                CharsetEncoding::Utf8,
                FileType::Text,
            ),
            // 非 UTF-8 文本 / UTF-16/32：全量读入并转码为 UTF-8
            ByteKind::Transcode => {
                let mut buffer = Vec::new();
                let mut file_clone = file.try_clone()?;
                file_clone.read_to_end(&mut buffer)?;
                let (decoded, enc_name) =
                    crate::core::encoding_fallback::decode_with_fallback(&buffer);
                log_object_size(
                    "stream_reader",
                    "from_file.transcode",
                    "decoded_bytes",
                    decoded.len(),
                );
                let encoding = Self::encoding_from_name(&enc_name);
                (
                    Inner::Buffer(decoded.into_bytes()),
                    encoding,
                    FileType::Text,
                )
            }
        };

        let file_metadata = FileMetadata {
            path: Some(path.to_path_buf()),
            size: file_size,
            file_type,
            encoding,
            bom,
            created: metadata.created().ok(),
            modified: metadata.modified().ok(),
            accessed: metadata.accessed().ok(),
            permissions: Some(metadata.permissions()),
            owner: None,
            fs_type: None,
            origin_os: None,
        };

        Ok(StreamReader {
            inner,
            metadata: Some(file_metadata),
        })
    }

    /// 从文件句柄头部读取至多 `cap` 字节的样本（P1-08）。
    /// 注意：`try_clone` 复制的句柄与原句柄**共享游标**，读完后必须 seek 回
    /// 起点，否则后续 `read_inner` 将从文件中部甚至 EOF 开始。
    fn read_head(file: &fs::File, cap: usize) -> Vec<u8> {
        use std::io::{Seek, SeekFrom};
        let mut preview = match file.try_clone() {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        if preview.seek(SeekFrom::Start(0)).is_err() {
            return Vec::new();
        }
        let mut head = Vec::with_capacity(cap);
        let mut chunk = [0u8; 1024];
        while head.len() < cap {
            match preview.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    let take = n.min(cap - head.len());
                    head.extend_from_slice(&chunk[..take]);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        let _ = preview.seek(SeekFrom::Start(0));
        head
    }

    /// P1-08：字节内容分类——纯 UTF-8 / 二进制 / 需转码（非 UTF-8 文本或 UTF-16/32）。
    /// 返回 `(kind, bom)`。
    fn classify_bytes(bytes: &[u8]) -> (ByteKind, Option<Bom>) {
        let bom = Self::detect_bom(bytes);
        if crate::core::encoding_fallback::is_probable_binary_bytes(bytes) {
            return (ByteKind::Binary, bom);
        }
        match bom {
            // UTF-16/32 BOM：交解码器直转
            Some(Bom::Utf16Le) | Some(Bom::Utf16Be) | Some(Bom::Utf32Le) | Some(Bom::Utf32Be) => {
                (ByteKind::Transcode, bom)
            }
            // UTF-8 BOM 或无 BOM：验证 UTF-8 有效性
            Some(Bom::Utf8) | None => {
                if std::str::from_utf8(bytes).is_ok() {
                    (ByteKind::Utf8, bom)
                } else {
                    (ByteKind::Transcode, bom)
                }
            }
        }
    }

    /// 整文件读取（P2-22：统一 `read_to_end` 到自有缓冲，不再走 mmap——
    /// 映射期间文件被外部截断（logrotate/活日志）会触发不可捕获的 SIGSEGV，
    /// 自有缓冲则天然是读入瞬间的快照，截断无影响）。
    fn read_inner(file: &fs::File, scope: &str) -> Result<Inner<'static>, StreamError> {
        let mut buffer = Vec::new();
        let mut file_clone = file.try_clone()?;
        file_clone.read_to_end(&mut buffer)?;
        log_object_size(
            "stream_reader",
            &format!("{scope}.buffer"),
            "buffer_bytes",
            buffer.len(),
        );
        Ok(Inner::Buffer(buffer))
    }

    /// P1-08：`decode_with_fallback` 返回的编码名 → `CharsetEncoding` 枚举映射。
    fn encoding_from_name(name: &str) -> CharsetEncoding {
        match name.to_ascii_lowercase().as_str() {
            "utf-8" => CharsetEncoding::Utf8,
            "utf-16le" => CharsetEncoding::Utf16Le,
            "utf-16be" => CharsetEncoding::Utf16Be,
            "utf-32le" => CharsetEncoding::Utf32Le,
            "utf-32be" => CharsetEncoding::Utf32Be,
            "gbk" | "936" => CharsetEncoding::Gbk,
            "gb18030" => CharsetEncoding::Gb2312,
            "big5" => CharsetEncoding::Big5,
            "shift_jis" | "windows-31j" | "cp932" => CharsetEncoding::ShiftJis,
            "euc-kr" => CharsetEncoding::EucKr,
            "cp949" => CharsetEncoding::Cp949,
            "windows-1251" => CharsetEncoding::Windows1251,
            "koi8-r" => CharsetEncoding::Koi8R,
            "iso-8859-5" => CharsetEncoding::Iso8859_5,
            "windows-1256" => CharsetEncoding::Windows1256,
            "iso-8859-6" => CharsetEncoding::Iso8859_6,
            "windows-1255" => CharsetEncoding::Windows1255,
            "iso-8859-8" => CharsetEncoding::Iso8859_8,
            "windows-1252" | "latin-1" | "iso-8859-1" => CharsetEncoding::Windows1252,
            _ => CharsetEncoding::Unknown,
        }
    }

    /// 从字符串直接创建 StreamReader。适用于处理已加载到内存的小文本或测试场景。
    pub fn from_str(text: &'a str) -> Self {
        let _probe =
            ScopeProbe::new("stream_reader", "from_str").add_field("input_bytes", text.len());
        StreamReader {
            inner: Inner::Bytes(text.as_bytes()),
            metadata: None,
        }
    }

    /// 从拥有所有权的 String 创建读取器。
    pub fn from_str_owned(text: String) -> Self {
        let _probe =
            ScopeProbe::new("stream_reader", "from_str_owned").add_field("input_bytes", text.len());
        StreamReader {
            inner: Inner::Buffer(text.into_bytes()),
            metadata: None,
        }
    }

    /// 获取文件元数据。
    pub fn metadata(&self) -> Option<&FileMetadata> {
        self.metadata.as_ref()
    }

    /// 获取原始字节切片。
    pub fn get_data(&self) -> &[u8] {
        match &self.inner {
            Inner::Buffer(buffer) => buffer.as_slice(),
            Inner::Bytes(bytes) => bytes,
        }
    }

    /// 获取数据源大小（字节）。
    pub fn size(&self) -> usize {
        self.metadata
            .as_ref()
            .map(|m| m.size as usize)
            .unwrap_or_else(|| match &self.inner {
                Inner::Buffer(b) => b.len(),
                Inner::Bytes(b) => b.len(),
                _ => 0,
            })
    }

    /// 判断当前内容是否为纯文本。通过检测样本字节中是否包含 NULL 字符（0x00）来判断。
    pub fn is_text(&self) -> bool {
        match &self.inner {
            Inner::Buffer(buffer) => !Self::detect_binary(buffer.as_slice()),
            Inner::Bytes(bytes) => !Self::detect_binary(bytes),
        }
    }

    /// 创建逐行迭代器。自动映射数据源并提供零拷贝行遍历。支持 LF 和 CRLF 换行符。
    pub fn iter_lines(&self) -> LineIterator<'_> {
        let _probe =
            ScopeProbe::new("stream_reader", "iter_lines").add_field("source_size", self.size());
        let data = match &self.inner {
            Inner::Buffer(buffer) => buffer.as_slice(),
            Inner::Bytes(bytes) => bytes,
        };

        LineIterator {
            data,
            current_offset: 0,
            current_line_number: 1,
            file_metadata: self.metadata.as_ref(),
        }
    }

    /// 创建块迭代器。按指定大小切分原始字节流，适用于大内容的分片分析。
    pub fn iter_blocks(&self, block_size: usize) -> Result<BlockIterator<'_>, StreamError> {
        let _probe = ScopeProbe::new("stream_reader", "iter_blocks")
            .add_field("source_size", self.size())
            .add_field("block_size", block_size);
        if block_size == 0 {
            return Err(StreamError::InvalidArgument);
        }

        let data = match &self.inner {
            Inner::Buffer(buffer) => buffer.as_slice(),
            Inner::Bytes(bytes) => bytes,
        };

        Ok(BlockIterator {
            data,
            current_offset: 0,
            block_size,
            file_metadata: self.metadata.as_ref(),
        })
    }

    /// 计算并行切块的目标大小（仅做切块规划，不含调度逻辑）。
    pub fn calculate_dynamic_chunk_size(total_size: usize, num_workers: usize) -> usize {
        if total_size == 0 {
            return 0;
        }

        let workers = num_workers.max(1);
        let target = total_size / workers;

        target
            .max(Self::MIN_PARALLEL_CHUNK_SIZE)
            .min(Self::MAX_PARALLEL_CHUNK_SIZE)
    }

    /// 将输入按语义锚点切成并行安全块。
    ///
    /// 注意：本方法只负责“切块与边界判定”，不涉及任何线程调度策略。
    pub fn split_for_parallel(&'a self, num_workers: usize) -> Vec<SliceInput<'a>> {
        let data = self.get_data();
        if data.is_empty() {
            return Vec::new();
        }

        let target = Self::calculate_dynamic_chunk_size(data.len(), num_workers);
        let mut chunks = Vec::new();
        let mut start = 0;
        let mut line_number = 1;

        while start < data.len() {
            let mut end = Self::split_by_semantic_anchors(data, start, target);

            if end <= start {
                end = (start + target.max(1)).min(data.len());
                while end > start && !is_utf8_boundary(data, end) {
                    end -= 1;
                }
                if end == start {
                    end = data.len();
                }
            }

            let block = &data[start..end];
            let raw = match std::str::from_utf8(block) {
                Ok(s) => Cow::Borrowed(s),
                Err(_) => Cow::Owned(String::from_utf8_lossy(block).into_owned()),
            };

            chunks.push(SliceInput {
                raw,
                offset: start,
                line_number,
                file_metadata: self.metadata.as_ref(),
            });

            line_number += memchr::memchr_iter(b'\n', block).count();
            start = end;
        }

        chunks
    }

    /// 从 `start + target_chunk_size` 附近寻找语义安全切分点。
    ///
    /// 优先规则：
    /// 1) 不切断行；
    /// 2) 倾向切在“新语义块起点”（空行、时间戳行、`[` 开头行、非缩进行）；
    /// 3) 受扫描上限保护，避免无限扩张。
    pub(crate) fn split_by_semantic_anchors(
        data: &'a [u8],
        start: usize,
        target_chunk_size: usize,
    ) -> usize {
        if start >= data.len() {
            return data.len();
        }

        let target_end = (start + target_chunk_size.max(1)).min(data.len());
        if target_end >= data.len() {
            return data.len();
        }

        let initial_break = find_next_line_start(data, target_end);
        if initial_break >= data.len() {
            return data.len();
        }

        let scan_cap =
            (start + target_chunk_size.max(1) * Self::SEMANTIC_SCAN_MULTIPLIER).min(data.len());

        let mut cursor = initial_break;
        while cursor < scan_cap {
            let line_end = find_line_end(data, cursor);
            let line = trim_cr(&data[cursor..line_end]);

            if is_semantic_safe_break_line(line) {
                return cursor;
            }

            if line_end >= data.len() {
                return data.len();
            }

            cursor = (line_end + 1).min(data.len());
        }

        initial_break
    }

    /// 二进制文件检测。扫描前 8KB 字节，如果发现 NULL 字节则判定为二进制文件。
    pub fn detect_binary(data: &[u8]) -> bool {
        let check_len = std::cmp::min(data.len(), 8192);
        if check_len == 0 {
            return false;
        }

        // 使用 memchr 进行 SIMD 加速的 NULL 字节搜索
        memchr::memchr(0, &data[..check_len]).is_some()
    }

    /// 辅助方法：通过文件头字节序列检测 BOM (Byte Order Mark) 类型。
    pub fn detect_bom(header: &[u8]) -> Option<Bom> {
        if header.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
            Some(Bom::Utf32Le)
        } else if header.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
            Some(Bom::Utf32Be)
        } else if header.starts_with(&[0xEF, 0xBB, 0xBF]) {
            Some(Bom::Utf8)
        } else if header.len() >= 2 && header[0] == 0xFF && header[1] == 0xFE {
            Some(Bom::Utf16Le)
        } else if header.starts_with(&[0xFE, 0xFF]) {
            Some(Bom::Utf16Be)
        } else {
            None
        }
    }
}

/// 从指定偏移起寻找下一行的起始位置：若偏移恰在换行符之后则原样返回，
/// 否则向前搜索下一个
///  并返回其后的位置；找不到则返回数据末尾。
fn find_next_line_start(data: &[u8], from: usize) -> usize {
    if from == 0 {
        return 0;
    }
    if from >= data.len() {
        return data.len();
    }

    if data[from - 1] == b'\n' {
        return from;
    }

    if let Some(pos) = memchr::memchr(b'\n', &data[from..]) {
        (from + pos + 1).min(data.len())
    } else {
        data.len()
    }
}

/// 从指定偏移起寻找当前行的结束位置：返回第一个
///  的索引（不含换行符），
/// 无换行符时返回数据末尾。
fn find_line_end(data: &[u8], start: usize) -> usize {
    if start >= data.len() {
        return data.len();
    }

    if let Some(pos) = memchr::memchr(b'\n', &data[start..]) {
        start + pos
    } else {
        data.len()
    }
}

/// 去掉行尾的 \r 字符（CRLF 行尾），纯 LF 行原样返回。
fn trim_cr(line: &[u8]) -> &[u8] {
    if line.last() == Some(&b'\r') {
        &line[..line.len().saturating_sub(1)]
    } else {
        line
    }
}

/// 判断某行是否适合作为语义安全切分点：空行、以数字或 [ 开头的行、
/// 以及非缩进（非空格/制表符开头）的行均可安全断开，缩进的续行不可断开。
fn is_semantic_safe_break_line(line: &[u8]) -> bool {
    if line.is_empty() {
        return true;
    }

    let first = line[0];
    if first.is_ascii_digit() || first == b'[' {
        return true;
    }

    first != b' ' && first != b'\t'
}

/// 判断 index 是否为 UTF-8 字符边界：位于开头/末尾或字节高位不为连续字节前缀时视为边界。
fn is_utf8_boundary(data: &[u8], index: usize) -> bool {
    if index == 0 || index == data.len() {
        return true;
    }
    let b = data[index];
    (b as i8) >= -0x40
}
